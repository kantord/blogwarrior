use indicatif::ProgressBar;
use rayon::prelude::*;

use crate::data::Transaction;
use crate::data::schema::{FeedItem, FeedSource};
use crate::feed::FeedMeta;

pub(crate) type FetchResult = (FeedSource, Result<(FeedMeta, Vec<FeedItem>), String>);

const FETCH_THREADS: usize = 48;

/// Fetch all feeds in parallel.
pub(crate) fn fetch_feeds(sources: &[FeedSource], pb: &ProgressBar) -> Vec<FetchResult> {
    let client = match crate::utils::http::http_client() {
        Ok(c) => c,
        Err(e) => {
            pb.suspend(|| eprintln!("Error creating HTTP client: {e}"));
            return Vec::new();
        }
    };
    pb.set_length(sources.len() as u64);

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(FETCH_THREADS)
        .build()
        .expect("failed to build fetch thread pool");

    pool.install(|| {
        sources
            .par_iter()
            .map(|source| {
                pb.set_message(source.url.clone());
                let result = crate::feed::fetch(&client, &source.url).map_err(|e| e.to_string());
                pb.inc(1);
                (source.clone(), result)
            })
            .collect()
    })
}

/// Apply fetched feed results to the store.
pub(crate) fn apply_fetched(
    tx: &mut Transaction,
    results: Vec<FetchResult>,
    pb: &ProgressBar,
) -> anyhow::Result<()> {
    for (mut source, result) in results {
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
        source.title = meta.title;
        source.site_url = meta.site_url;
        source.description = meta.description;
        tx.feeds.upsert(source);
    }
    Ok(())
}
