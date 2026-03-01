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
        // First sync — just commit and push
        git::auto_commit(&repo, "sync")?;
        git::push(&path)?;
        return Ok(());
    }

    git::fetch(&path)?;

    // Merge each table from remote
    let remote_feeds = git::read_remote_table(&repo, "feeds")?;
    let remote_posts = git::read_remote_table(&repo, "posts")?;

    {
        let tx = store.begin();
        tx.feeds.merge_remote(remote_feeds);
        tx.posts.merge_remote(remote_posts);
    }
    store.save()?;

    git::auto_commit(&repo, "sync")?;
    git::merge_ours(&repo)?;
    git::push(&path)?;

    Ok(())
}
