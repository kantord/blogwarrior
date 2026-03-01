use synctato::Database;

use crate::git;
use crate::store::Store;

use super::pull::cmd_pull;

pub(crate) fn cmd_sync(store: &mut Store) -> anyhow::Result<()> {
    let path = store.path().to_path_buf();
    let repo = git::try_open_repo(&path);

    // If git exists, ensure working tree is clean before we start
    if let Some(ref repo) = repo {
        git::ensure_clean(repo)?;
    }

    // Always pull feeds
    eprint!("Pulling feeds...");
    store.transaction(cmd_pull)?;
    eprintln!(" done.");

    // Auto-commit pulled data (if git exists)
    if let Some(ref repo) = repo {
        git::auto_commit(repo, "pull feeds")?;
    }

    // No git repo → we're done (offline / no-git usage)
    let repo = match repo {
        Some(r) => r,
        None => return Ok(()),
    };

    // No remote configured → warn and stop (not an error)
    if !git::has_remote(&path) {
        eprintln!(
            "warning: no remote configured; run `blog git remote add origin <url>` to enable sync"
        );
        return Ok(());
    }

    // No remote branch yet → first push
    if !git::has_remote_branch(&repo, "refs/remotes/origin/main") {
        eprintln!("First sync — pushing to remote...");
        git::push(&path)?;
        eprintln!("Done.");
        return Ok(());
    }

    // Fetch
    eprint!("Fetching...");
    git::fetch(&path)?;
    eprintln!(" done.");

    // Already up-to-date
    if git::is_up_to_date(&repo)? {
        eprintln!("Already up to date.");
        return Ok(());
    }

    // Local is strictly ahead (remote is ancestor) → just push, no merge needed
    if git::is_remote_ancestor(&repo)? {
        eprint!("Pushing...");
        git::push(&path)?;
        eprintln!(" done.");
        return Ok(());
    }

    // Diverged → merge remote data
    eprint!("Merging remote data...");
    let remote_feeds = git::read_remote_table(&repo, "feeds")?;
    let remote_posts = git::read_remote_table(&repo, "posts")?;

    let feeds_count = remote_feeds.len();
    let posts_count = remote_posts.len();

    {
        let tx = store.begin();
        tx.feeds.merge_remote(remote_feeds);
        tx.posts.merge_remote(remote_posts);
    }
    store.save()?;
    eprintln!(
        " done ({} feeds, {} posts from remote).",
        feeds_count, posts_count
    );

    git::auto_commit(&repo, "sync")?;
    git::merge_ours(&repo)?;

    eprint!("Pushing...");
    git::push(&path)?;
    eprintln!(" done.");

    Ok(())
}
