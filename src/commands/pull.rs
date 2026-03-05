use indicatif::ProgressBar;
use rayon::prelude::*;

use crate::feed::FeedMeta;
use crate::store::Transaction;
use crate::tables::{FeedItem, FeedSource};

pub(crate) type FetchResult = (FeedSource, Result<(FeedMeta, Vec<FeedItem>), String>);

/// Fetch all feeds in parallel.
pub(crate) fn fetch_feeds(sources: &[FeedSource], pb: &ProgressBar) -> Vec<FetchResult> {
    let client = match crate::http::http_client() {
        Ok(c) => c,
        Err(e) => {
            pb.suspend(|| eprintln!("Error creating HTTP client: {e}"));
            return Vec::new();
        }
    };
    pb.set_length(sources.len() as u64);

    sources
        .par_iter()
        .map(|source| {
            pb.set_message(source.url.clone());
            let result = crate::feed::fetch(&client, &source.url).map_err(|e| e.to_string());
            pb.inc(1);
            (source.clone(), result)
        })
        .collect()
}

/// Apply fetched feed results to the store.
pub(crate) fn apply_fetched(
    tx: &mut Transaction,
    results: Vec<FetchResult>,
    pb: &ProgressBar,
) -> anyhow::Result<()> {
    for (source, result) in results {
        let (meta, items) = match result {
            Ok(r) => r,
            Err(e) => {
                pb.suspend(|| eprintln!("Error fetching {}: {}", source.url, e));
                continue;
            }
        };
        let feed_id = tx.feeds.id_of(&source);
        for mut item in items {
            item.feed = feed_id.clone();
            tx.posts.upsert(item);
        }
        let mut updated = source.clone();
        updated.title = meta.title;
        updated.site_url = meta.site_url;
        updated.description = meta.description;
        tx.feeds.upsert(updated);
    }
    Ok(())
}
