use anyhow::bail;

use crate::data::Transaction;
use crate::data::index::resolve_shorthand;

pub(crate) fn cmd_remove(tx: &mut Transaction, url: &str) -> anyhow::Result<()> {
    let url = if let Some(shorthand) = url.strip_prefix('@') {
        resolve_shorthand(tx.feeds, shorthand)
            .ok_or_else(|| anyhow::anyhow!("Unknown feed shorthand: @{}", shorthand))?
    } else {
        url.to_string()
    };

    match tx.feeds.delete(&url) {
        Some(feed_id) => {
            tx.delete_posts_where(|p| p.feed == feed_id);
        }
        None => bail!("Feed not found: {}", url),
    }

    Ok(())
}
