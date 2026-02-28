use crate::feed_source::FeedSource;
use crate::store::Transaction;

pub(crate) fn cmd_add(tx: &mut Transaction, url: &str) -> anyhow::Result<()> {
    tx.feeds.upsert(FeedSource {
        url: url.to_string(),
        title: String::new(),
        site_url: String::new(),
        description: String::new(),
    });
    Ok(())
}
