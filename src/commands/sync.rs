use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};
use synctato::{SyncEvent, SyncResult};

use crate::data::BlogData;
use crate::utils::progress::spinner;

use crate::feed::pull::{apply_fetched, fetch_feeds};

pub(crate) fn cmd_sync(store: &mut BlogData) -> anyhow::Result<()> {
    let pb = ProgressBar::new(0);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.cyan} Pulling feeds [{bar:20.cyan/dim}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("=> "),
    );
    pb.enable_steady_tick(Duration::from_millis(100));

    // Fetch feeds outside the transaction (network I/O, no lock held)
    let sources = store.feeds().items();
    let results = fetch_feeds(&sources, &pb);
    pb.finish_and_clear();

    // Apply results inside a locked transaction
    store.transact("pull feeds", |tx| apply_fetched(tx, results, &pb))?;

    let mut sp: Option<ProgressBar> = None;
    let result = store.sync_remote(|event| match event {
        SyncEvent::Fetching => {
            sp = Some(spinner("Fetching..."));
        }
        SyncEvent::FetchDone => {
            if let Some(s) = sp.take() {
                s.finish_with_message("Fetching... done.");
            }
        }
        SyncEvent::Pushing { first_push } => {
            let msg = if first_push {
                "Pushing to remote (first sync)..."
            } else {
                "Pushing..."
            };
            sp = Some(spinner(msg));
        }
        SyncEvent::PushDone { first_push } => {
            let msg = if first_push {
                "Pushing to remote (first sync)... done."
            } else {
                "Pushing... done."
            };
            if let Some(s) = sp.take() {
                s.finish_with_message(msg);
            }
        }
        SyncEvent::MergingRemote => {
            sp = Some(spinner("Merging remote data..."));
        }
        SyncEvent::MergeDone { counts } => {
            if let Some(s) = sp.take() {
                let detail: Vec<String> = counts
                    .iter()
                    .map(|(name, count)| format!("{} {}", count, name))
                    .collect();
                s.finish_with_message(format!(
                    "Merging remote data... done ({} from remote).",
                    detail.join(", ")
                ));
            }
        }
    })?;

    match result {
        SyncResult::NoGitRepo | SyncResult::Synced => {}
        SyncResult::NoRemote => {
            eprintln!(
                "warning: no remote configured; run `blog git remote add origin <url>` to enable sync"
            );
        }
        SyncResult::AlreadyUpToDate => {
            eprintln!("Already up to date.");
        }
    }

    Ok(())
}
