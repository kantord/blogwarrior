use anyhow::ensure;

use crate::store::Store;

use super::feed_index;

pub(crate) fn cmd_feed_ls(store: &Store) -> anyhow::Result<()> {
    let fi = feed_index(&store.feeds);
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
