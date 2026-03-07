use anyhow::ensure;

use crate::data::BlogData;
use crate::data::index::feed_index;

pub(crate) fn cmd_feed_ls(store: &BlogData) -> anyhow::Result<()> {
    let fi = feed_index(store.feeds());
    ensure!(!fi.entries.is_empty(), "No matching feeds");
    for e in &fi.entries {
        if e.feed.title.is_empty() {
            println!("@{} {}", e.shorthand, e.feed.url);
        } else {
            println!("@{} {} ({})", e.shorthand, e.feed.url, e.feed.title);
        }
    }
    Ok(())
}
