use std::collections::HashMap;
use std::path::Path;

use crate::{Row, TableRow, parse_rows};
use anyhow::{Context, bail};
use git2::{Repository, RepositoryOpenFlags, Signature};

/// Open a git repo at exactly `path`, without searching parent directories.
fn open_exact(path: &Path) -> Result<Repository, git2::Error> {
    Repository::open_ext(
        path,
        RepositoryOpenFlags::NO_SEARCH,
        &[] as &[&std::ffi::OsStr],
    )
}

/// Try to open a git repo at exactly `path`. Returns None if no repo exists there.
/// Does NOT search parent directories.
pub(crate) fn try_open_repo(path: &Path) -> Option<Repository> {
    open_exact(path).ok()
}

/// Open a git repo at exactly `path`, returning an error if none exists.
pub fn open_repo(path: &Path) -> anyhow::Result<Repository> {
    open_exact(path).context("failed to open git repository")
}

#[cfg(test)]
pub fn open_or_init_repo(path: &Path) -> anyhow::Result<Repository> {
    match open_exact(path) {
        Ok(repo) => Ok(repo),
        Err(_) => {
            let repo = Repository::init(path)
                .with_context(|| format!("failed to init git repo at {}", path.display()))?;
            if !is_clean(&repo)? {
                auto_commit(&repo, "init store")?;
            }
            Ok(repo)
        }
    }
}

fn is_data_file(path: &str) -> bool {
    path.contains('/') && path.ends_with(".jsonl")
}

pub(crate) fn is_clean(repo: &Repository) -> anyhow::Result<bool> {
    let statuses = repo.statuses(None).context("failed to get repo status")?;
    let dirty = statuses
        .iter()
        .any(|entry| entry.path().is_some_and(is_data_file));
    Ok(!dirty)
}

pub(crate) fn ensure_clean(repo: &Repository) -> anyhow::Result<()> {
    if !is_clean(repo)? {
        bail!("store has uncommitted changes; commit or discard them before proceeding");
    }
    Ok(())
}

pub(crate) fn auto_commit(repo: &Repository, message: &str) -> anyhow::Result<()> {
    if is_clean(repo)? {
        return Ok(());
    }

    let mut index = repo.index().context("failed to open index")?;
    index
        .add_all(["*/items_*.jsonl"], git2::IndexAddOption::DEFAULT, None)
        .context("failed to stage files")?;
    // Also stage deletions: remove index entries whose files no longer exist on disk
    let workdir = repo.workdir().context("bare repo not supported")?;
    let stale: Vec<Vec<u8>> = index
        .iter()
        .filter(|entry| {
            let path = String::from_utf8_lossy(&entry.path);
            path.ends_with(".jsonl") && !workdir.join(path.as_ref()).exists()
        })
        .map(|entry| entry.path.clone())
        .collect();
    for path in stale {
        index.remove_path(Path::new(std::str::from_utf8(&path).unwrap_or("")))?;
    }
    index.write().context("failed to write index")?;

    let tree_oid = index.write_tree().context("failed to write tree")?;
    let tree = repo.find_tree(tree_oid).context("failed to find tree")?;

    let sig = signature(repo)?;

    let parent = match repo.head() {
        Ok(head) => Some(
            head.peel_to_commit()
                .context("failed to peel HEAD to commit")?,
        ),
        Err(_) => None,
    };

    let parents: Vec<&git2::Commit> = parent.iter().collect();
    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
        .context("failed to create commit")?;

    Ok(())
}

fn signature(repo: &Repository) -> anyhow::Result<Signature<'static>> {
    match repo.signature() {
        Ok(sig) => Ok(Signature::now(
            sig.name().unwrap_or("synctato"),
            sig.email().unwrap_or("synctato@localhost"),
        )?),
        Err(_) => Ok(Signature::now("synctato", "synctato@localhost")?),
    }
}

/// Find the remote tracking branch for origin (e.g. "refs/remotes/origin/main").
/// Tries the local HEAD branch name first, then falls back to common defaults.
fn find_remote_ref(repo: &Repository) -> Option<git2::Reference<'_>> {
    if let Ok(head) = repo.head()
        && let Some(branch) = head.shorthand()
    {
        let refname = format!("refs/remotes/origin/{branch}");
        if let Ok(r) = repo.find_reference(&refname) {
            return Some(r);
        }
    }
    for name in ["main", "master"] {
        let refname = format!("refs/remotes/origin/{name}");
        if let Ok(r) = repo.find_reference(&refname) {
            return Some(r);
        }
    }
    None
}

pub(crate) fn has_remote_branch(repo: &Repository) -> bool {
    find_remote_ref(repo).is_some()
}

/// Returns true if HEAD and the remote tracking branch point to the same commit.
pub(crate) fn is_up_to_date(repo: &Repository) -> anyhow::Result<bool> {
    let head = repo
        .head()
        .context("no HEAD")?
        .peel_to_commit()
        .context("failed to peel HEAD")?;
    let remote_ref = match find_remote_ref(repo) {
        Some(r) => r,
        None => return Ok(false),
    };
    let remote = remote_ref
        .peel_to_commit()
        .context("failed to peel remote ref")?;
    Ok(head.id() == remote.id())
}

/// Returns true when the remote tracking branch is a strict ancestor of HEAD (local is ahead, just push).
pub(crate) fn is_remote_ancestor(repo: &Repository) -> anyhow::Result<bool> {
    let head = repo.head()?.peel_to_commit()?;
    let remote_ref = match find_remote_ref(repo) {
        Some(r) => r,
        None => return Ok(false),
    };
    let remote = remote_ref.peel_to_commit()?;
    if head.id() == remote.id() {
        return Ok(false);
    }
    Ok(repo
        .graph_descendant_of(head.id(), remote.id())
        .unwrap_or(false))
}

/// Record a merge commit using the local tree (git "ours" strategy).
///
/// The actual data merge (CRDT last-writer-wins) has already happened at
/// the application layer via `Table::merge_remote` before this is called.
/// This commit just unifies the git history so future pulls see both lineages.
pub(crate) fn merge_ours(repo: &Repository) -> anyhow::Result<()> {
    let remote_ref = match find_remote_ref(repo) {
        Some(r) => r,
        None => return Ok(()),
    };

    let head_commit = repo
        .head()
        .context("no HEAD")?
        .peel_to_commit()
        .context("failed to peel HEAD")?;
    let remote_commit = remote_ref
        .peel_to_commit()
        .context("failed to peel remote ref")?;

    if repo
        .graph_descendant_of(head_commit.id(), remote_commit.id())
        .unwrap_or(false)
    {
        return Ok(());
    }

    // Both sides produced identical data — fast-forward to the remote commit
    // instead of creating an empty merge commit.
    if head_commit.tree_id() == remote_commit.tree_id() {
        let head_ref = repo.head().context("no HEAD")?;
        let branch_name = head_ref.name().context("HEAD has no name")?;
        repo.reference(
            branch_name,
            remote_commit.id(),
            true,
            "fast-forward to remote",
        )?;
        return Ok(());
    }

    let sig = signature(repo)?;
    let tree = head_commit.tree().context("failed to get HEAD tree")?;

    repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        "merge remote (ours)",
        &tree,
        &[&head_commit, &remote_commit],
    )
    .context("failed to create merge commit")?;

    Ok(())
}

pub fn read_remote_table<T: TableRow>(
    repo: &Repository,
    table_name: &str,
) -> anyhow::Result<HashMap<String, Row<T>>> {
    let remote_ref = match find_remote_ref(repo) {
        Some(r) => r,
        None => return Ok(HashMap::new()),
    };

    let commit = remote_ref
        .peel_to_commit()
        .context("failed to peel remote ref to commit")?;
    let tree = commit.tree().context("failed to get remote tree")?;

    let subtree = match tree.get_name(table_name) {
        Some(entry) => entry
            .to_object(repo)
            .context("failed to resolve table dir")?
            .peel_to_tree()
            .context("table entry is not a directory")?,
        None => return Ok(HashMap::new()),
    };

    let mut all_rows = HashMap::new();
    for entry in subtree.iter() {
        let name = entry.name().unwrap_or("");
        if name.starts_with("items_") && name.ends_with(".jsonl") {
            let blob = entry
                .to_object(repo)
                .context("failed to resolve blob")?
                .peel_to_blob()
                .context("entry is not a blob")?;
            let content = std::str::from_utf8(blob.content())
                .with_context(|| format!("non-UTF8 content in {}/{}", table_name, name))?;
            let rows: HashMap<String, Row<T>> = parse_rows(content)
                .with_context(|| format!("failed to parse {}/{}", table_name, name))?;
            all_rows.extend(rows);
        }
    }

    Ok(all_rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{fetch, push};
    use serde::{Deserialize, Serialize};
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct GitTestItem {
        #[serde(default)]
        raw_id: String,
        title: String,
    }

    impl TableRow for GitTestItem {
        fn key(&self) -> String {
            self.raw_id.clone()
        }
        const TABLE_NAME: &'static str = "test_table";
        const SHARD_CHARACTERS: usize = 0;
        const EXPECTED_CAPACITY: usize = 1000;
    }

    fn init_repo(path: &Path) -> Repository {
        let mut opts = git2::RepositoryInitOptions::new();
        opts.initial_head("main");
        Repository::init_opts(path, &opts).unwrap()
    }

    fn init_bare_repo(path: &Path) -> Repository {
        let mut opts = git2::RepositoryInitOptions::new();
        opts.initial_head("main");
        opts.bare(true);
        Repository::init_opts(path, &opts).unwrap()
    }

    fn setup_git_config(repo: &Repository) {
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test").unwrap();
        config.set_str("user.email", "test@test.com").unwrap();
    }

    /// Write a data file into a table directory (the kind auto_commit should track).
    fn write_data(dir: &Path, table: &str, file: &str, content: &str) {
        let table_dir = dir.join(table);
        fs::create_dir_all(&table_dir).unwrap();
        fs::write(table_dir.join(file), content).unwrap();
    }

    #[test]
    fn test_open_or_init_fresh_dir() {
        let dir = TempDir::new().unwrap();
        let repo = open_or_init_repo(dir.path()).unwrap();
        assert!(!repo.is_bare());
    }

    #[test]
    fn test_open_or_init_existing_repo() {
        let dir = TempDir::new().unwrap();
        init_repo(dir.path());
        let repo = open_or_init_repo(dir.path()).unwrap();
        assert!(!repo.is_bare());
    }

    #[test]
    fn test_open_or_init_commits_existing_data() {
        let dir = TempDir::new().unwrap();
        write_data(dir.path(), "feeds", "items_.jsonl", "{\"id\":\"a\"}\n");
        let repo = open_or_init_repo(dir.path()).unwrap();
        setup_git_config(&repo);
        assert!(repo.head().is_ok());
        assert!(is_clean(&repo).unwrap());
    }

    #[test]
    fn test_is_clean_on_clean_repo() {
        let dir = TempDir::new().unwrap();
        let repo = init_repo(dir.path());
        setup_git_config(&repo);
        write_data(dir.path(), "feeds", "items_.jsonl", "{\"id\":\"a\"}\n");
        auto_commit(&repo, "initial").unwrap();
        assert!(is_clean(&repo).unwrap());
    }

    #[test]
    fn test_is_clean_with_modified_data_file() {
        let dir = TempDir::new().unwrap();
        let repo = init_repo(dir.path());
        setup_git_config(&repo);
        write_data(dir.path(), "feeds", "items_.jsonl", "{\"id\":\"a\"}\n");
        auto_commit(&repo, "initial").unwrap();
        write_data(dir.path(), "feeds", "items_.jsonl", "{\"id\":\"b\"}\n");
        assert!(!is_clean(&repo).unwrap());
    }

    #[test]
    fn test_is_clean_with_new_data_file() {
        let dir = TempDir::new().unwrap();
        let repo = init_repo(dir.path());
        setup_git_config(&repo);
        write_data(dir.path(), "feeds", "items_.jsonl", "{\"id\":\"a\"}\n");
        auto_commit(&repo, "initial").unwrap();
        write_data(dir.path(), "posts", "items_a.jsonl", "{\"id\":\"p\"}\n");
        assert!(!is_clean(&repo).unwrap());
    }

    #[test]
    fn test_is_clean_ignores_lock_files() {
        let dir = TempDir::new().unwrap();
        let repo = init_repo(dir.path());
        setup_git_config(&repo);
        write_data(dir.path(), "feeds", "items_.jsonl", "{\"id\":\"a\"}\n");
        auto_commit(&repo, "initial").unwrap();
        fs::write(dir.path().join("feeds").join(".lock"), "").unwrap();
        assert!(is_clean(&repo).unwrap());
    }

    #[test]
    fn test_is_clean_ignores_unrelated_files() {
        let dir = TempDir::new().unwrap();
        let repo = init_repo(dir.path());
        setup_git_config(&repo);
        write_data(dir.path(), "feeds", "items_.jsonl", "{\"id\":\"a\"}\n");
        auto_commit(&repo, "initial").unwrap();
        fs::write(dir.path().join("random.txt"), "whatever").unwrap();
        assert!(is_clean(&repo).unwrap());
    }

    #[test]
    fn test_ensure_clean_on_clean_repo() {
        let dir = TempDir::new().unwrap();
        let repo = init_repo(dir.path());
        setup_git_config(&repo);
        write_data(dir.path(), "feeds", "items_.jsonl", "{\"id\":\"a\"}\n");
        auto_commit(&repo, "initial").unwrap();
        assert!(ensure_clean(&repo).is_ok());
    }

    #[test]
    fn test_ensure_clean_on_dirty_repo() {
        let dir = TempDir::new().unwrap();
        let repo = init_repo(dir.path());
        setup_git_config(&repo);
        write_data(dir.path(), "feeds", "items_.jsonl", "{\"id\":\"a\"}\n");
        auto_commit(&repo, "initial").unwrap();
        write_data(dir.path(), "feeds", "items_.jsonl", "{\"id\":\"b\"}\n");
        let err = ensure_clean(&repo).unwrap_err();
        assert!(
            format!("{err}").contains("uncommitted"),
            "error should mention uncommitted changes: {err}"
        );
    }

    #[test]
    fn test_auto_commit_with_changes() {
        let dir = TempDir::new().unwrap();
        let repo = init_repo(dir.path());
        setup_git_config(&repo);
        write_data(dir.path(), "feeds", "items_.jsonl", "{\"id\":\"a\"}\n");
        auto_commit(&repo, "test commit").unwrap();
        assert!(is_clean(&repo).unwrap());

        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head.message().unwrap(), "test commit");
    }

    #[test]
    fn test_auto_commit_no_changes() {
        let dir = TempDir::new().unwrap();
        let repo = init_repo(dir.path());
        setup_git_config(&repo);
        write_data(dir.path(), "feeds", "items_.jsonl", "{\"id\":\"a\"}\n");
        auto_commit(&repo, "first").unwrap();

        let head1 = repo.head().unwrap().peel_to_commit().unwrap().id();
        auto_commit(&repo, "second").unwrap();
        let head2 = repo.head().unwrap().peel_to_commit().unwrap().id();

        assert_eq!(head1, head2, "no new commit when nothing changed");
    }

    #[test]
    fn test_auto_commit_does_not_stage_lock_files() {
        let dir = TempDir::new().unwrap();
        let repo = init_repo(dir.path());
        setup_git_config(&repo);

        let table_dir = dir.path().join("feeds");
        fs::create_dir_all(&table_dir).unwrap();
        fs::write(
            table_dir.join("items_.jsonl"),
            "{\"id\":\"aa\",\"url\":\"https://example.com\"}\n",
        )
        .unwrap();
        fs::write(table_dir.join(".lock"), "").unwrap();

        auto_commit(&repo, "add data").unwrap();

        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let tree = head.tree().unwrap();
        let feeds_tree = tree
            .get_name("feeds")
            .unwrap()
            .to_object(&repo)
            .unwrap()
            .peel_to_tree()
            .unwrap();
        assert!(
            feeds_tree.get_name(".lock").is_none(),
            ".lock file should not be committed"
        );
        assert!(
            feeds_tree.get_name("items_.jsonl").is_some(),
            "data file should be committed"
        );
    }

    #[test]
    fn test_auto_commit_does_not_stage_unrelated_files() {
        let dir = TempDir::new().unwrap();
        let repo = init_repo(dir.path());
        setup_git_config(&repo);

        let table_dir = dir.path().join("feeds");
        fs::create_dir_all(&table_dir).unwrap();
        fs::write(
            table_dir.join("items_.jsonl"),
            "{\"id\":\"aa\",\"url\":\"https://example.com\"}\n",
        )
        .unwrap();
        fs::write(dir.path().join("random.txt"), "should not be committed").unwrap();

        auto_commit(&repo, "add data").unwrap();

        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let tree = head.tree().unwrap();
        assert!(
            tree.get_name("random.txt").is_none(),
            "unrelated files should not be committed"
        );
        assert!(
            tree.get_name("feeds").is_some(),
            "table directory should be committed"
        );
    }

    #[test]
    fn test_has_remote_branch_false() {
        let dir = TempDir::new().unwrap();
        let repo = init_repo(dir.path());
        assert!(!has_remote_branch(&repo));
    }

    #[test]
    fn test_merge_ours_diverged() {
        let origin_dir = TempDir::new().unwrap();
        let _origin = init_bare_repo(origin_dir.path());

        let clone_dir = TempDir::new().unwrap();
        let repo = init_repo(clone_dir.path());
        setup_git_config(&repo);

        repo.remote("origin", &format!("file://{}", origin_dir.path().display()))
            .unwrap();

        write_data(
            clone_dir.path(),
            "feeds",
            "items_.jsonl",
            "{\"id\":\"a\"}\n",
        );
        auto_commit(&repo, "initial").unwrap();
        push(clone_dir.path()).unwrap();

        let other_dir = TempDir::new().unwrap();
        let other_output = Command::new("git")
            .args([
                "clone",
                &format!("file://{}", origin_dir.path().display()),
                &other_dir.path().to_string_lossy(),
            ])
            .output()
            .unwrap();
        assert!(
            other_output.status.success(),
            "clone failed: {}",
            String::from_utf8_lossy(&other_output.stderr)
        );

        Command::new("git")
            .args([
                "-C",
                &other_dir.path().to_string_lossy(),
                "config",
                "user.name",
                "Other",
            ])
            .output()
            .unwrap();
        Command::new("git")
            .args([
                "-C",
                &other_dir.path().to_string_lossy(),
                "config",
                "user.email",
                "other@test.com",
            ])
            .output()
            .unwrap();

        write_data(
            other_dir.path(),
            "posts",
            "items_b.jsonl",
            "{\"id\":\"b\"}\n",
        );
        Command::new("git")
            .args(["-C", &other_dir.path().to_string_lossy(), "add", "."])
            .output()
            .unwrap();
        Command::new("git")
            .args([
                "-C",
                &other_dir.path().to_string_lossy(),
                "commit",
                "-m",
                "other commit",
            ])
            .output()
            .unwrap();
        push(other_dir.path()).unwrap();

        write_data(
            clone_dir.path(),
            "posts",
            "items_c.jsonl",
            "{\"id\":\"c\"}\n",
        );
        auto_commit(&repo, "local commit").unwrap();

        fetch(clone_dir.path()).unwrap();

        merge_ours(&repo).unwrap();

        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head.parent_count(), 2, "merge commit should have 2 parents");

        let posts_tree = head
            .tree()
            .unwrap()
            .get_name("posts")
            .unwrap()
            .to_object(&repo)
            .unwrap()
            .peel_to_tree()
            .unwrap();
        assert!(
            posts_tree.get_name("items_c.jsonl").is_some(),
            "merge should keep local tree"
        );
    }

    fn setup_remote_with_table(table_name: &str, files: &[(&str, &str)]) -> (TempDir, Repository) {
        let origin_dir = TempDir::new().unwrap();
        let _origin = init_bare_repo(origin_dir.path());

        let clone_dir = TempDir::new().unwrap();
        let repo = init_repo(clone_dir.path());
        setup_git_config(&repo);

        repo.remote("origin", &format!("file://{}", origin_dir.path().display()))
            .unwrap();

        let other_dir = TempDir::new().unwrap();
        let other_output = Command::new("git")
            .args([
                "clone",
                &format!("file://{}", origin_dir.path().display()),
                &other_dir.path().to_string_lossy(),
            ])
            .output()
            .unwrap();
        let _ = other_output;

        let other_repo = match Repository::open(other_dir.path()) {
            Ok(r) => r,
            Err(_) => {
                let r = init_repo(other_dir.path());
                r.remote("origin", &format!("file://{}", origin_dir.path().display()))
                    .unwrap();
                r
            }
        };

        let mut config = other_repo.config().unwrap();
        config.set_str("user.name", "Other").unwrap();
        config.set_str("user.email", "other@test.com").unwrap();

        let table_dir = other_dir.path().join(table_name);
        fs::create_dir_all(&table_dir).unwrap();
        for (fname, content) in files {
            fs::write(table_dir.join(fname), content).unwrap();
        }
        auto_commit(&other_repo, "add table data").unwrap();
        push(other_dir.path()).unwrap();

        fetch(clone_dir.path()).unwrap();

        (clone_dir, repo)
    }

    #[test]
    fn test_read_remote_table_one_shard() {
        let content = "{\"id\":\"aa\",\"title\":\"Remote Item\"}\n";
        let (_dir, repo) = setup_remote_with_table("test_table", &[("items_.jsonl", content)]);

        let rows: HashMap<String, Row<GitTestItem>> =
            read_remote_table(&repo, "test_table").unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows.contains_key("aa"));
    }

    #[test]
    fn test_read_remote_table_multiple_shards() {
        let content1 = "{\"id\":\"aa\",\"title\":\"Item A\"}\n";
        let content2 = "{\"id\":\"bb\",\"title\":\"Item B\"}\n";
        let (_dir, repo) = setup_remote_with_table(
            "test_table",
            &[("items_a.jsonl", content1), ("items_b.jsonl", content2)],
        );

        let rows: HashMap<String, Row<GitTestItem>> =
            read_remote_table(&repo, "test_table").unwrap();
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn test_read_remote_table_missing_dir() {
        let origin_dir = TempDir::new().unwrap();
        let _origin = init_bare_repo(origin_dir.path());

        let clone_dir = TempDir::new().unwrap();
        let repo = init_repo(clone_dir.path());
        setup_git_config(&repo);

        let rows: HashMap<String, Row<GitTestItem>> =
            read_remote_table(&repo, "nonexistent").unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn test_read_remote_table_corrupted_jsonl() {
        let content = "not valid json\n";
        let (_dir, repo) = setup_remote_with_table("test_table", &[("items_.jsonl", content)]);

        let result: anyhow::Result<HashMap<String, Row<GitTestItem>>> =
            read_remote_table(&repo, "test_table");
        assert!(result.is_err());
    }

    #[test]
    fn test_is_remote_ancestor_when_ahead() {
        let origin_dir = TempDir::new().unwrap();
        let _origin = init_bare_repo(origin_dir.path());

        let clone_dir = TempDir::new().unwrap();
        let repo = init_repo(clone_dir.path());
        setup_git_config(&repo);
        repo.remote("origin", &format!("file://{}", origin_dir.path().display()))
            .unwrap();

        write_data(
            clone_dir.path(),
            "feeds",
            "items_.jsonl",
            "{\"id\":\"a\"}\n",
        );
        auto_commit(&repo, "initial").unwrap();
        push(clone_dir.path()).unwrap();
        fetch(clone_dir.path()).unwrap();

        write_data(
            clone_dir.path(),
            "feeds",
            "items_.jsonl",
            "{\"id\":\"b\"}\n",
        );
        auto_commit(&repo, "local ahead").unwrap();

        assert!(is_remote_ancestor(&repo).unwrap());
    }

    #[test]
    fn test_is_remote_ancestor_when_diverged() {
        let origin_dir = TempDir::new().unwrap();
        let _origin = init_bare_repo(origin_dir.path());

        let clone_dir = TempDir::new().unwrap();
        let repo = init_repo(clone_dir.path());
        setup_git_config(&repo);
        repo.remote("origin", &format!("file://{}", origin_dir.path().display()))
            .unwrap();

        write_data(
            clone_dir.path(),
            "feeds",
            "items_.jsonl",
            "{\"id\":\"a\"}\n",
        );
        auto_commit(&repo, "initial").unwrap();
        push(clone_dir.path()).unwrap();

        let other_dir = TempDir::new().unwrap();
        let other_output = Command::new("git")
            .args([
                "clone",
                &format!("file://{}", origin_dir.path().display()),
                &other_dir.path().to_string_lossy().as_ref(),
            ])
            .output()
            .unwrap();
        assert!(other_output.status.success());
        Command::new("git")
            .args([
                "-C",
                &other_dir.path().to_string_lossy(),
                "config",
                "user.name",
                "Other",
            ])
            .output()
            .unwrap();
        Command::new("git")
            .args([
                "-C",
                &other_dir.path().to_string_lossy(),
                "config",
                "user.email",
                "o@t.com",
            ])
            .output()
            .unwrap();
        write_data(
            other_dir.path(),
            "posts",
            "items_b.jsonl",
            "{\"id\":\"b\"}\n",
        );
        Command::new("git")
            .args(["-C", &other_dir.path().to_string_lossy(), "add", "."])
            .output()
            .unwrap();
        Command::new("git")
            .args([
                "-C",
                &other_dir.path().to_string_lossy(),
                "commit",
                "-m",
                "remote",
            ])
            .output()
            .unwrap();
        push(other_dir.path()).unwrap();

        write_data(
            clone_dir.path(),
            "posts",
            "items_c.jsonl",
            "{\"id\":\"c\"}\n",
        );
        auto_commit(&repo, "local diverge").unwrap();

        fetch(clone_dir.path()).unwrap();

        assert!(!is_remote_ancestor(&repo).unwrap());
    }

    #[test]
    fn test_is_remote_ancestor_when_equal() {
        let origin_dir = TempDir::new().unwrap();
        let _origin = init_bare_repo(origin_dir.path());

        let clone_dir = TempDir::new().unwrap();
        let repo = init_repo(clone_dir.path());
        setup_git_config(&repo);
        repo.remote("origin", &format!("file://{}", origin_dir.path().display()))
            .unwrap();

        write_data(
            clone_dir.path(),
            "feeds",
            "items_.jsonl",
            "{\"id\":\"a\"}\n",
        );
        auto_commit(&repo, "initial").unwrap();
        push(clone_dir.path()).unwrap();
        fetch(clone_dir.path()).unwrap();

        assert!(!is_remote_ancestor(&repo).unwrap());
    }

    #[test]
    fn test_is_remote_ancestor_no_remote_branch() {
        let dir = TempDir::new().unwrap();
        let repo = init_repo(dir.path());
        setup_git_config(&repo);

        write_data(dir.path(), "feeds", "items_.jsonl", "{\"id\":\"a\"}\n");
        auto_commit(&repo, "initial").unwrap();

        assert!(!is_remote_ancestor(&repo).unwrap());
    }
}
