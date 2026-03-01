use anyhow::bail;
use synctato::Database;

use crate::git;
use crate::store::Store;

pub(crate) fn cmd_sync(store: &mut Store) -> anyhow::Result<()> {
    let path = store.path().to_path_buf();
    let repo = git::open_or_init_repo(&path)?;
    git::ensure_clean(&repo)?;

    if !git::has_remote(&path) {
        bail!("no remote configured; run `blog git remote add origin <url>` first");
    }

    if !git::has_remote_branch(&repo, "refs/remotes/origin/main") {
        eprintln!("First sync — pushing to remote...");
        git::auto_commit(&repo, "sync")?;
        git::push(&path)?;
        eprintln!("Done.");
        return Ok(());
    }

    eprint!("Fetching...");
    git::fetch(&path)?;
    eprintln!(" done.");

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
