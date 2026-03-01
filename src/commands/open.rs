use anyhow::ensure;

use crate::feed::FeedItem;
use crate::store::Store;

use super::{PostIndex, post_index};

fn resolve_post_shorthand(store: &Store, shorthand: &str) -> anyhow::Result<FeedItem> {
    let PostIndex { items, shorthands } = post_index(store.posts());
    items
        .into_iter()
        .find(|item| shorthands.get(&item.raw_id).is_some_and(|s| s == shorthand))
        .ok_or_else(|| anyhow::anyhow!("Unknown shorthand: {}", shorthand))
}

pub(crate) fn cmd_open(store: &Store, shorthand: &str) -> anyhow::Result<()> {
    let item = resolve_post_shorthand(store, shorthand)?;
    ensure!(!item.link.is_empty(), "Post has no link");
    match std::env::var("BROWSER") {
        Ok(browser) => open::with(&item.link, &browser),
        Err(_) => open::that(&item.link),
    }
    .map_err(|e| anyhow::anyhow!("Could not open URL: {}", e))?;
    Ok(())
}
