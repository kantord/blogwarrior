use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};
use synctato::Database;

use crate::git;
use crate::store::Store;

use super::pull::cmd_pull;

fn new_spinner(msg: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}

pub(crate) fn cmd_sync(store: &mut Store) -> anyhow::Result<()> {
    let path = store.path().to_path_buf();
    let repo = git::try_open_repo(&path);

    // If git exists, ensure working tree is clean before we start
    if let Some(ref repo) = repo {
        git::ensure_clean(repo)?;
    }

    // Always pull feeds
    let pb = ProgressBar::new(0);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.cyan} Pulling feeds [{bar:20.cyan/dim}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("=> "),
    );
    pb.enable_steady_tick(Duration::from_millis(100));
    store.transaction(|tx| cmd_pull(tx, &pb))?;
    pb.finish_and_clear();

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
    if !git::has_remote_branch(&repo) {
        let sp = new_spinner("Pushing to remote (first sync)...");
        git::push(&path)?;
        sp.finish_with_message("Pushing to remote (first sync)... done.");
        return Ok(());
    }

    // Fetch
    let sp = new_spinner("Fetching...");
    git::fetch(&path)?;
    sp.finish_with_message("Fetching... done.");

    // Already up-to-date
    if git::is_up_to_date(&repo)? {
        eprintln!("Already up to date.");
        return Ok(());
    }

    // Local is strictly ahead (remote is ancestor) → just push, no merge needed
    if git::is_remote_ancestor(&repo)? {
        let sp = new_spinner("Pushing...");
        git::push(&path)?;
        sp.finish_with_message("Pushing... done.");
        return Ok(());
    }

    // Diverged → merge remote data
    let sp = new_spinner("Merging remote data...");
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
    sp.finish_with_message(format!(
        "Merging remote data... done ({} feeds, {} posts from remote).",
        feeds_count, posts_count
    ));

    git::auto_commit(&repo, "sync")?;
    // Data is already merged above; this just records both git parents
    git::merge_ours(&repo)?;

    let sp = new_spinner("Pushing...");
    git::push(&path)?;
    sp.finish_with_message("Pushing... done.");

    Ok(())
}
