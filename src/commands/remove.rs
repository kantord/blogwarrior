use anyhow::bail;
use synctato::TableRow;

use crate::store::Store;

use super::resolve_shorthand;

pub(crate) fn cmd_remove(store: &mut Store, url: &str) -> anyhow::Result<()> {
    let url = if let Some(shorthand) = url.strip_prefix('@') {
        resolve_shorthand(&store.feeds, shorthand)
            .ok_or_else(|| anyhow::anyhow!("Unknown feed shorthand: @{}", shorthand))?
    } else {
        url.to_string()
    };

    match store.feeds.delete(&url) {
        Some(feed_id) => {
            let post_keys: Vec<String> = store
                .posts
                .items()
                .iter()
                .filter(|p| p.feed == feed_id)
                .map(|p| p.key())
                .collect();
            for key in post_keys {
                store.posts.delete(&key);
            }
        }
        None => bail!("Feed not found: {}", url),
    }

    Ok(())
}
