use indicatif::ProgressBar;
use rayon::prelude::*;

use crate::feed::{FeedItem, FeedMeta};
use crate::feed_source::FeedSource;
use crate::store::Transaction;

pub(crate) fn http_client() -> anyhow::Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .user_agent(format!("blogtato/{}", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| anyhow::anyhow!("failed to build HTTP client: {}", e))
}

type FetchResult = (FeedSource, Result<(FeedMeta, Vec<FeedItem>), String>);

pub(crate) fn cmd_pull(tx: &mut Transaction, pb: &ProgressBar) -> anyhow::Result<()> {
    let client = http_client()?;
    let sources = tx.feeds.items();
    pb.set_length(sources.len() as u64);

    // Fetch all feeds in parallel
    let results: Vec<FetchResult> = sources
        .par_iter()
        .map(|source| {
            pb.set_message(source.url.clone());
            let result = crate::feed::fetch(&client, &source.url).map_err(|e| e.to_string());
            pb.inc(1);
            (source.clone(), result)
        })
        .collect();

    // Apply results sequentially
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
