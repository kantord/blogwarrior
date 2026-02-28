use std::path::Path;

use anyhow::ensure;

use crate::feed_source::FeedSource;

use super::feed_index;

pub(crate) fn cmd_feed_ls(store: &Path) -> anyhow::Result<()> {
    let feeds_table = synctato::Table::<FeedSource>::load(store)?;
    let fi = feed_index(&feeds_table);
    ensure!(!fi.feeds.is_empty(), "No matching feeds");
    for (feed, shorthand) in fi.feeds.iter().zip(fi.shorthands.iter()) {
        if feed.title.is_empty() {
            println!("@{} {}", shorthand, feed.url);
        } else {
            println!("@{} {} ({})", shorthand, feed.url, feed.title);
        }
    }
    Ok(())
}
