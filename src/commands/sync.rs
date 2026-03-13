use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};
use synctato::{SyncEvent, SyncResult};

use crate::data::BlogData;
use crate::utils::progress::spinner;
use crate::utils::version_check::check_for_newer_version;

use crate::feed::pull::{apply_fetched, fetch_feeds};

const CRATES_IO_URL: &str = "https://crates.io/api/v1/crates/blogtato";

fn do_sync_remote(store: &mut BlogData) -> anyhow::Result<SyncResult> {
    let mut sp: Option<ProgressBar> = None;
    store.sync_remote(|event| match event {
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
    })
}

pub(crate) fn cmd_sync(store: &mut BlogData) -> anyhow::Result<()> {
    // Sync with remote first so we discover feeds added on other devices
    let result = do_sync_remote(store)?;

    let needs_push = match result {
        SyncResult::NoRemote => {
            eprintln!(
                "warning: no remote configured; run `blog git remote add origin <url>` to enable sync"
            );
            false
        }
        SyncResult::NoGitRepo => false,
        SyncResult::Synced | SyncResult::AlreadyUpToDate => true,
    };

    // Fetch feeds outside the transaction (network I/O, no lock held)
    let pb = ProgressBar::new(0);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.cyan} Pulling feeds [{bar:20.cyan/dim}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("=> "),
    );
    pb.enable_steady_tick(Duration::from_millis(100));

    let sources = store.feeds().items();
    let results = fetch_feeds(&sources, &pb);
    pb.finish_and_clear();

    // Apply results inside a locked transaction
    store.transact("pull feeds", |tx| apply_fetched(tx, results, &pb))?;

    // Sync again to push the freshly fetched feed data back to remote
    if needs_push {
        let push_result = do_sync_remote(store)?;
        match push_result {
            SyncResult::Synced => {} // pushed successfully, spinners already shown
            SyncResult::AlreadyUpToDate => {
                eprintln!("Already up to date.");
            }
            SyncResult::NoRemote | SyncResult::NoGitRepo => {
                // Shouldn't happen since we already confirmed remote exists
            }
        }
    }

    if let Ok(Some(status)) = check_for_newer_version(CRATES_IO_URL, env!("CARGO_PKG_VERSION")) {
        eprintln!(
            "Note: blogtato {} is available (you have {}). Run `cargo install blogtato` to update.",
            status.latest, status.current
        );
    }

    Ok(())
}
