use std::path::Path;

use anyhow::bail;
use synctato::TableRow;

use crate::feed::FeedItem;
use crate::feed_source::FeedSource;

use super::resolve_shorthand;

pub(crate) fn cmd_remove(store: &Path, url: &str) -> anyhow::Result<()> {
    let mut feeds_table = synctato::Table::<FeedSource>::load(store)?;
    let mut posts_table = synctato::Table::<FeedItem>::load(store)?;

    let resolved_url;
    let url = if let Some(shorthand) = url.strip_prefix('@') {
        match resolve_shorthand(&feeds_table, shorthand) {
            Some(u) => {
                resolved_url = u;
                &resolved_url
            }
            None => bail!("Unknown shorthand: @{}", shorthand),
        }
    } else {
        url
    };

    match feeds_table.delete(url) {
        Some(feed_id) => {
            let post_keys: Vec<String> = posts_table
                .items()
                .iter()
                .filter(|p| p.feed == feed_id)
                .map(|p| p.key())
                .collect();
            for key in post_keys {
                posts_table.delete(&key);
            }
        }
        None => bail!("Feed not found: {}", url),
    }

    feeds_table.save()?;
    posts_table.save()?;
    Ok(())
}
