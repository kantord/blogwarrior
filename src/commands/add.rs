use crate::feed_source::FeedSource;
use crate::store::Store;

pub(crate) fn cmd_add(store: &mut Store, url: &str) -> anyhow::Result<()> {
    store.feeds.upsert(FeedSource {
        url: url.to_string(),
        title: String::new(),
        site_url: String::new(),
        description: String::new(),
    });
    Ok(())
}
