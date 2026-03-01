use indicatif::ProgressBar;

use crate::store::Transaction;

fn http_client() -> anyhow::Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .user_agent(format!("blogtato/{}", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| anyhow::anyhow!("failed to build HTTP client: {}", e))
}

pub(crate) fn cmd_pull(tx: &mut Transaction, pb: &ProgressBar) -> anyhow::Result<()> {
    let client = http_client()?;
    let sources = tx.feeds.items();
    pb.set_length(sources.len() as u64);
    for source in &sources {
        pb.set_message(source.url.clone());
        let (meta, items) = match crate::feed::fetch(&client, &source.url) {
            Ok(result) => result,
            Err(e) => {
                pb.suspend(|| eprintln!("Error fetching {}: {}", source.url, e));
                pb.inc(1);
                continue;
            }
        };
        let feed_id = tx.feeds.id_of(source);
        for mut item in items {
            item.feed = feed_id.clone();
            tx.posts.upsert(item);
        }
        let mut updated = source.clone();
        updated.title = meta.title;
        updated.site_url = meta.site_url;
        updated.description = meta.description;
        tx.feeds.upsert(updated);
        pb.inc(1);
    }
    Ok(())
}
