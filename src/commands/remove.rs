use crate::synctato::TableRow;
use anyhow::bail;

use crate::store::Transaction;

use super::resolve_shorthand;

pub(crate) fn cmd_remove(tx: &mut Transaction, url: &str) -> anyhow::Result<()> {
    let url = if let Some(shorthand) = url.strip_prefix('@') {
        resolve_shorthand(tx.feeds, shorthand)
            .ok_or_else(|| anyhow::anyhow!("Unknown feed shorthand: @{}", shorthand))?
    } else {
        url.to_string()
    };

    match tx.feeds.delete(&url) {
        Some(feed_id) => {
            let post_keys: Vec<String> = tx
                .posts
                .items()
                .iter()
                .filter(|p| p.feed == feed_id)
                .map(|p| p.key())
                .collect();
            for key in post_keys {
                tx.posts.delete(&key);
            }
        }
        None => bail!("Feed not found: {}", url),
    }

    Ok(())
}
