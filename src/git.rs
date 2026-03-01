use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, bail};
use git2::{Repository, RepositoryOpenFlags, Signature};
use synctato::{Row, TableRow, parse_rows};

// --- Local operations (git2) ---

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
pub fn try_open_repo(path: &Path) -> Option<Repository> {
    open_exact(path).ok()
}

pub fn open_or_init_repo(path: &Path) -> anyhow::Result<Repository> {
    match open_exact(path) {
        Ok(repo) => Ok(repo),
        Err(_) => {
            let repo = Repository::init(path)
                .with_context(|| format!("failed to init git repo at {}", path.display()))?;
            // If there are already files in the directory, commit them
            if has_uncommitted_files(&repo)? {
                auto_commit(&repo, "init store")?;
            }
            Ok(repo)
        }
    }
}

fn has_uncommitted_files(repo: &Repository) -> anyhow::Result<bool> {
    let statuses = repo.statuses(None).context("failed to get repo status")?;
    Ok(!statuses.is_empty())
}

pub fn is_clean(repo: &Repository) -> anyhow::Result<bool> {
    Ok(!has_uncommitted_files(repo)?)
}

pub fn ensure_clean(repo: &Repository) -> anyhow::Result<()> {
    if !is_clean(repo)? {
        bail!("store has uncommitted changes; commit or discard them before proceeding");
    }
    Ok(())
}

pub fn auto_commit(repo: &Repository, message: &str) -> anyhow::Result<()> {
    if is_clean(repo)? {
        return Ok(());
    }

    let mut index = repo.index().context("failed to open index")?;
    index
        .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
        .context("failed to stage files")?;
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
    // Try repo config first, fall back to defaults
    match repo.signature() {
        Ok(sig) => Ok(Signature::now(
            sig.name().unwrap_or("blogtato"),
            sig.email().unwrap_or("blogtato@localhost"),
        )?),
        Err(_) => Ok(Signature::now("blogtato", "blogtato@localhost")?),
    }
}

pub fn has_remote_branch(repo: &Repository, refname: &str) -> bool {
    repo.find_reference(refname).is_ok()
}

/// Returns true if HEAD and origin/main point to the same commit.
pub fn is_up_to_date(repo: &Repository) -> anyhow::Result<bool> {
    let head = repo
        .head()
        .context("no HEAD")?
        .peel_to_commit()
        .context("failed to peel HEAD")?;
    let remote_ref = match repo.find_reference("refs/remotes/origin/main") {
        Ok(r) => r,
        Err(_) => return Ok(false),
    };
    let remote = remote_ref
        .peel_to_commit()
        .context("failed to peel remote ref")?;
    Ok(head.id() == remote.id())
}

pub fn merge_ours(repo: &Repository) -> anyhow::Result<()> {
    let remote_ref = match repo.find_reference("refs/remotes/origin/main") {
        Ok(r) => r,
        Err(_) => return Ok(()),
    };

    let head_commit = repo
        .head()
        .context("no HEAD")?
        .peel_to_commit()
        .context("failed to peel HEAD")?;
    let remote_commit = remote_ref
        .peel_to_commit()
        .context("failed to peel remote ref")?;

    // If remote is an ancestor of HEAD, nothing to merge
    if repo
        .graph_descendant_of(head_commit.id(), remote_commit.id())
        .unwrap_or(false)
    {
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
    let remote_ref = match repo.find_reference("refs/remotes/origin/main") {
        Ok(r) => r,
        Err(_) => return Ok(HashMap::new()),
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

// --- Network operations (git CLI) ---

pub fn fetch(path: &Path) -> anyhow::Result<()> {
    let output = Command::new("git")
        .args(["-C", &path.to_string_lossy(), "fetch", "origin"])
        .output()
        .context("failed to run git fetch")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git fetch failed: {}", stderr.trim());
    }
    Ok(())
}

pub fn push(path: &Path) -> anyhow::Result<()> {
    let output = Command::new("git")
        .args(["-C", &path.to_string_lossy(), "push", "origin", "HEAD"])
        .output()
        .context("failed to run git push")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git push failed: {}", stderr.trim());
    }
    Ok(())
}

pub fn has_remote(path: &Path) -> bool {
    Command::new("git")
        .args(["-C", &path.to_string_lossy(), "remote", "get-url", "origin"])
        .output()
        .is_ok_and(|o| o.status.success())
}

pub fn git_passthrough(path: &Path, args: &[String]) -> anyhow::Result<()> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(path);
    cmd.args(args);

    let status = cmd.status().context("failed to run git")?;
    if !status.success() {
        bail!("git exited with {}", status);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::fs;
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

    fn setup_git_config(repo: &Repository) {
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test").unwrap();
        config.set_str("user.email", "test@test.com").unwrap();
    }

    // --- open_or_init_repo tests ---

    #[test]
    fn test_open_or_init_fresh_dir() {
        let dir = TempDir::new().unwrap();
        let repo = open_or_init_repo(dir.path()).unwrap();
        assert!(!repo.is_bare());
    }

    #[test]
    fn test_open_or_init_existing_repo() {
        let dir = TempDir::new().unwrap();
        Repository::init(dir.path()).unwrap();
        let repo = open_or_init_repo(dir.path()).unwrap();
        assert!(!repo.is_bare());
    }

    #[test]
    fn test_open_or_init_commits_existing_files() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("data.txt"), "hello").unwrap();
        let repo = open_or_init_repo(dir.path()).unwrap();
        setup_git_config(&repo);
        // Re-init to trigger the commit of existing files
        drop(repo);
        // Since repo was already opened, let's manually verify or re-open
        let repo = Repository::open(dir.path()).unwrap();
        // Check if there's a commit (head should exist after auto_commit)
        // The first open_or_init_repo should have committed the file
        let repo2 = open_or_init_repo(dir.path()).unwrap();
        assert!(repo2.head().is_ok() || is_clean(&repo).unwrap_or(true));
    }

    // --- is_clean tests ---

    #[test]
    fn test_is_clean_on_clean_repo() {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        setup_git_config(&repo);
        fs::write(dir.path().join("file.txt"), "content").unwrap();
        auto_commit(&repo, "initial").unwrap();
        assert!(is_clean(&repo).unwrap());
    }

    #[test]
    fn test_is_clean_with_modified_file() {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        setup_git_config(&repo);
        fs::write(dir.path().join("file.txt"), "content").unwrap();
        auto_commit(&repo, "initial").unwrap();
        fs::write(dir.path().join("file.txt"), "modified").unwrap();
        assert!(!is_clean(&repo).unwrap());
    }

    #[test]
    fn test_is_clean_with_untracked_file() {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        setup_git_config(&repo);
        fs::write(dir.path().join("file.txt"), "content").unwrap();
        auto_commit(&repo, "initial").unwrap();
        fs::write(dir.path().join("new.txt"), "new").unwrap();
        assert!(!is_clean(&repo).unwrap());
    }

    // --- ensure_clean tests ---

    #[test]
    fn test_ensure_clean_on_clean_repo() {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        setup_git_config(&repo);
        fs::write(dir.path().join("file.txt"), "content").unwrap();
        auto_commit(&repo, "initial").unwrap();
        assert!(ensure_clean(&repo).is_ok());
    }

    #[test]
    fn test_ensure_clean_on_dirty_repo() {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        setup_git_config(&repo);
        fs::write(dir.path().join("file.txt"), "content").unwrap();
        auto_commit(&repo, "initial").unwrap();
        fs::write(dir.path().join("dirty.txt"), "dirty").unwrap();
        let err = ensure_clean(&repo).unwrap_err();
        assert!(
            format!("{err}").contains("uncommitted"),
            "error should mention uncommitted changes: {err}"
        );
    }

    // --- auto_commit tests ---

    #[test]
    fn test_auto_commit_with_changes() {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        setup_git_config(&repo);
        fs::write(dir.path().join("file.txt"), "content").unwrap();
        auto_commit(&repo, "test commit").unwrap();
        assert!(is_clean(&repo).unwrap());

        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head.message().unwrap(), "test commit");
    }

    #[test]
    fn test_auto_commit_no_changes() {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        setup_git_config(&repo);
        fs::write(dir.path().join("file.txt"), "content").unwrap();
        auto_commit(&repo, "first").unwrap();

        let head1 = repo.head().unwrap().peel_to_commit().unwrap().id();
        auto_commit(&repo, "second").unwrap();
        let head2 = repo.head().unwrap().peel_to_commit().unwrap().id();

        assert_eq!(head1, head2, "no new commit when nothing changed");
    }

    // --- has_remote_branch tests ---

    #[test]
    fn test_has_remote_branch_false() {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        assert!(!has_remote_branch(&repo, "refs/remotes/origin/main"));
    }

    // --- merge_ours tests ---

    #[test]
    fn test_merge_ours_diverged() {
        // Setup: create two repos, diverge them, simulate fetch
        let origin_dir = TempDir::new().unwrap();
        let _origin = Repository::init_bare(origin_dir.path()).unwrap();

        let clone_dir = TempDir::new().unwrap();
        let repo = Repository::init(clone_dir.path()).unwrap();
        setup_git_config(&repo);

        // Add remote
        repo.remote("origin", &format!("file://{}", origin_dir.path().display()))
            .unwrap();

        // Create initial commit and push
        fs::write(clone_dir.path().join("a.txt"), "a").unwrap();
        auto_commit(&repo, "initial").unwrap();
        push(clone_dir.path()).unwrap();

        // Simulate divergence: create a commit in origin directly
        // We'll do it by creating another clone, committing there, and pushing
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

        // Set git config in other clone
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

        fs::write(other_dir.path().join("b.txt"), "b").unwrap();
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

        // Create local diverging commit
        fs::write(clone_dir.path().join("c.txt"), "c").unwrap();
        auto_commit(&repo, "local commit").unwrap();

        // Fetch
        fetch(clone_dir.path()).unwrap();

        // Now merge_ours
        merge_ours(&repo).unwrap();

        // Verify: merge commit exists, HEAD has 2 parents
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head.parent_count(), 2, "merge commit should have 2 parents");

        // Tree should be the local tree (ours strategy)
        let local_tree_has_c = head.tree().unwrap().get_name("c.txt").is_some();
        assert!(local_tree_has_c, "merge should keep local tree");
    }

    // --- read_remote_table tests ---

    fn setup_remote_with_table(table_name: &str, files: &[(&str, &str)]) -> (TempDir, Repository) {
        let origin_dir = TempDir::new().unwrap();
        let _origin = Repository::init_bare(origin_dir.path()).unwrap();

        let clone_dir = TempDir::new().unwrap();
        let repo = Repository::init(clone_dir.path()).unwrap();
        setup_git_config(&repo);

        repo.remote("origin", &format!("file://{}", origin_dir.path().display()))
            .unwrap();

        // Create table files in a temp dir, push as "remote"
        let other_dir = TempDir::new().unwrap();
        let other_output = Command::new("git")
            .args([
                "clone",
                &format!("file://{}", origin_dir.path().display()),
                &other_dir.path().to_string_lossy(),
            ])
            .output()
            .unwrap();
        // Clone might warn about empty repo, that's ok
        let _ = other_output;

        // Init other repo manually if clone from empty fails
        let other_repo = match Repository::open(other_dir.path()) {
            Ok(r) => r,
            Err(_) => {
                let r = Repository::init(other_dir.path()).unwrap();
                r.remote("origin", &format!("file://{}", origin_dir.path().display()))
                    .unwrap();
                r
            }
        };

        // Set git config
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

        // Fetch in our repo
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
        let _origin = Repository::init_bare(origin_dir.path()).unwrap();

        let clone_dir = TempDir::new().unwrap();
        let repo = Repository::init(clone_dir.path()).unwrap();
        setup_git_config(&repo);

        // No remote branch at all
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

    // --- Network operations tests ---

    #[test]
    fn test_has_remote_false() {
        let dir = TempDir::new().unwrap();
        Repository::init(dir.path()).unwrap();
        assert!(!has_remote(dir.path()));
    }

    #[test]
    fn test_has_remote_true() {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        repo.remote("origin", "https://example.com/repo.git")
            .unwrap();
        assert!(has_remote(dir.path()));
    }

    #[test]
    fn test_fetch_no_remote() {
        let dir = TempDir::new().unwrap();
        Repository::init(dir.path()).unwrap();
        let result = fetch(dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_git_passthrough_status() {
        let dir = TempDir::new().unwrap();
        Repository::init(dir.path()).unwrap();
        let result = git_passthrough(dir.path(), &["status".to_string()]);
        assert!(result.is_ok());
    }
}
