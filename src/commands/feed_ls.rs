use std::path::Path;

use anyhow::ensure;

use crate::feed_source::FeedSource;

use super::compute_shorthands;

pub(crate) fn cmd_feed_ls(store: &Path) -> anyhow::Result<()> {
    let feeds_table = synctato::Table::<FeedSource>::load(store)?;
    let mut feeds = feeds_table.items();
    ensure!(!feeds.is_empty(), "No matching feeds");
    feeds.sort_by(|a, b| a.url.cmp(&b.url));
    let ids: Vec<String> = feeds.iter().map(|f| feeds_table.id_of(f)).collect();
    let shorthands = compute_shorthands(&ids);
    for (feed, shorthand) in feeds.iter().zip(shorthands.iter()) {
        if feed.title.is_empty() {
            println!("@{} {}", shorthand, feed.url);
        } else {
            println!("@{} {} ({})", shorthand, feed.url, feed.title);
        }
    }
    Ok(())
}
