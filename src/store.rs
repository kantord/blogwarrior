use std::path::Path;

use crate::feed::FeedItem;
use crate::feed_source::FeedSource;
use crate::git;
use crate::read_mark::ReadMark;
// Required by the `database!` macro expansion (provides `transaction`, `save`, `begin`).
use crate::synctato::Database;

crate::database!(pub(crate) Store {
    feeds: FeedSource,
    posts: FeedItem,
    reads: ReadMark,
});

pub(crate) type Transaction<'a> = StoreTransaction<'a>;

pub(crate) enum SyncEvent {
    Fetching,
    FetchDone,
    Pushing { first_push: bool },
    PushDone { first_push: bool },
    MergingRemote,
    MergeDone { feeds: usize, posts: usize },
}

pub(crate) enum SyncResult {
    NoGitRepo,
    NoRemote,
    AlreadyUpToDate,
    Synced,
}

impl Store {
    /// Git-aware transaction: ensure_clean → run closure → save → auto_commit.
    pub(crate) fn transact(
        &mut self,
        msg: &str,
        f: impl FnOnce(&mut Transaction<'_>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let repo = git::try_open_repo(self.path());
        if let Some(ref repo) = repo {
            git::ensure_clean(repo)?;
        }
        self.transaction(f)?;
        if let Some(ref repo) = repo {
            git::auto_commit(repo, msg)?;
        }
        Ok(())
    }

    /// Sync with git remote (fetch, merge, push).
    pub(crate) fn sync_remote(
        &mut self,
        mut on_progress: impl FnMut(SyncEvent),
    ) -> anyhow::Result<SyncResult> {
        let path = self.path().to_path_buf();
        let repo = match git::try_open_repo(&path) {
            Some(r) => r,
            None => return Ok(SyncResult::NoGitRepo),
        };

        git::ensure_clean(&repo)?;

        if !git::has_remote(&path) {
            return Ok(SyncResult::NoRemote);
        }

        if !git::has_remote_branch(&repo) {
            on_progress(SyncEvent::Pushing { first_push: true });
            git::push(&path)?;
            on_progress(SyncEvent::PushDone { first_push: true });
            return Ok(SyncResult::Synced);
        }

        on_progress(SyncEvent::Fetching);
        git::fetch(&path)?;
        on_progress(SyncEvent::FetchDone);

        if git::is_up_to_date(&repo)? {
            return Ok(SyncResult::AlreadyUpToDate);
        }

        // Local is strictly ahead → just push
        if git::is_remote_ancestor(&repo)? {
            on_progress(SyncEvent::Pushing { first_push: false });
            git::push(&path)?;
            on_progress(SyncEvent::PushDone { first_push: false });
            return Ok(SyncResult::Synced);
        }

        // Diverged → merge remote data
        on_progress(SyncEvent::MergingRemote);
        let remote_feeds = git::read_remote_table(&repo, "feeds")?;
        let remote_posts = git::read_remote_table(&repo, "posts")?;
        let remote_reads = git::read_remote_table(&repo, "reads")?;

        let feeds_count = remote_feeds.len();
        let posts_count = remote_posts.len();

        {
            let tx = self.begin();
            tx.feeds.merge_remote(remote_feeds);
            tx.posts.merge_remote(remote_posts);
            tx.reads.merge_remote(remote_reads);
        }
        self.save()?;
        on_progress(SyncEvent::MergeDone {
            feeds: feeds_count,
            posts: posts_count,
        });

        git::auto_commit(&repo, "sync")?;
        git::merge_ours(&repo)?;

        on_progress(SyncEvent::Pushing { first_push: false });
        git::push(&path)?;
        on_progress(SyncEvent::PushDone { first_push: false });

        Ok(SyncResult::Synced)
    }

    /// Run a raw git command in the store directory.
    pub(crate) fn git_passthrough(&self, args: &[String]) -> anyhow::Result<()> {
        git::git_passthrough(self.path(), args)
    }
}

/// Clone a git remote into a new store directory.
pub(crate) fn clone_store(dir: &Path, url: &str) -> anyhow::Result<()> {
    let output = git::git_output(&["clone", "--depth", "1", url, &dir.to_string_lossy()])?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git clone failed: {}", stderr.trim());
    }
    Ok(())
}
