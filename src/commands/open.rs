use std::path::Path;

use anyhow::{bail, ensure};

use crate::feed::FeedItem;

use super::{index_to_shorthand, load_sorted_posts};

fn resolve_post_shorthand(store: &Path, shorthand: &str) -> anyhow::Result<FeedItem> {
    let items = load_sorted_posts(store)?;
    let found = items
        .into_iter()
        .enumerate()
        .find(|(i, _)| index_to_shorthand(*i) == shorthand);
    match found {
        Some((_, item)) => Ok(item),
        None => bail!("Unknown shorthand: {}", shorthand),
    }
}

pub(crate) fn cmd_open(store: &Path, shorthand: &str) -> anyhow::Result<()> {
    let item = resolve_post_shorthand(store, shorthand)?;
    ensure!(!item.link.is_empty(), "Post has no link");
    open::that(&item.link).map_err(|e| anyhow::anyhow!("Could not open URL: {}", e))?;
    Ok(())
}
