use std::path::Path;

use crate::feed::FeedItem;
use crate::feed_source::FeedSource;

fn http_client() -> anyhow::Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .user_agent(format!("blogtato/{}", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| anyhow::anyhow!("failed to build HTTP client: {}", e))
}

pub(crate) fn cmd_pull(store: &Path) -> anyhow::Result<()> {
    let client = http_client()?;
    let mut feeds_table = synctato::Table::<FeedSource>::load(store)?;
    let sources = feeds_table.items();
    let mut table = synctato::Table::<FeedItem>::load(store)?;
    for source in &sources {
        let (meta, items) = match crate::feed::fetch(&client, &source.url) {
            Ok(result) => result,
            Err(e) => {
                eprintln!("Error fetching {}: {}", source.url, e);
                continue;
            }
        };
        let feed_id = feeds_table.id_of(source);
        for mut item in items {
            item.feed = feed_id.clone();
            table.upsert(item);
        }
        let mut updated = source.clone();
        updated.title = meta.title;
        updated.site_url = meta.site_url;
        updated.description = meta.description;
        feeds_table.upsert(updated);
    }
    table.save()?;
    feeds_table.save()?;
    Ok(())
}
