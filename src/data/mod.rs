pub mod index;
pub mod schema;

use std::path::Path;

use schema::{BlogDataSchema, MetaEntry};
use synctato::Store;

pub(crate) type BlogData = Store<BlogDataSchema>;
pub(crate) type Transaction<'a> = schema::BlogDataSchemaTransaction<'a>;

impl Transaction<'_> {
    /// Delete posts matching `pred` and cascade-delete their ReadMarks.
    pub(crate) fn delete_posts_where(&mut self, pred: impl Fn(&schema::FeedItem) -> bool) {
        let post_ids: Vec<String> = self
            .posts
            .items()
            .iter()
            .filter(|p| pred(p))
            .map(|p| p.raw_id.clone())
            .collect();
        self.posts.delete_where(pred);
        self.reads.delete_where(|r| post_ids.contains(&r.post_id));
    }
}

pub(crate) const SCHEMA_VERSION: u32 = 1;

/// Check that the store's schema version is compatible with this binary.
/// If the store has no version yet, write the current one.
/// If the store has a newer version, return an error.
pub(crate) fn check_schema_version(store: &mut BlogData) -> anyhow::Result<()> {
    let existing = store
        .meta()
        .items()
        .into_iter()
        .find(|e| e.key == "schema_version");

    match existing {
        Some(entry) => {
            let db_version: u32 = entry.value.parse().map_err(|_| {
                anyhow::anyhow!(
                    "Corrupted schema_version in store metadata: {:?}",
                    entry.value
                )
            })?;
            if db_version > SCHEMA_VERSION {
                anyhow::bail!(
                    "This database was written by a newer version of blogtato (schema v{db_version}). \
                     Your binary supports schema v{SCHEMA_VERSION}. Please update blogtato."
                );
            }
        }
        None => {
            store.transact("set schema version", |tx| {
                tx.meta.upsert(MetaEntry {
                    key: "schema_version".to_string(),
                    value: SCHEMA_VERSION.to_string(),
                });
                Ok(())
            })?;
        }
    }

    Ok(())
}

/// Ensure the store has a `.gitattributes` that prevents line-ending conversion
/// on JSONL data files. This avoids cross-platform dirty-worktree issues when
/// syncing between Windows and Unix.
pub(crate) fn ensure_gitattributes(store_path: &Path) -> anyhow::Result<()> {
    let path = store_path.join(".gitattributes");
    if !path.exists() {
        std::fs::write(&path, "*.jsonl -text\n")?;
    }
    Ok(())
}
