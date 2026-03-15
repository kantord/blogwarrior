pub mod index;
pub mod schema;

use regex::Regex;
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

pub(crate) fn hidden_link_regexes(store: &BlogData) -> anyhow::Result<Vec<Regex>> {
    let Some(raw) = get_config_value(store, "hide_link_regex") else {
        return Ok(Vec::new());
    };

    let patterns: Vec<String> = serde_json::from_str(&raw).map_err(|e| {
        anyhow::anyhow!("Invalid hide_link_regex config (expected JSON array): {e}")
    })?;

    patterns
        .into_iter()
        .map(|pattern| {
            Regex::new(&pattern)
                .map_err(|e| anyhow::anyhow!("Invalid regex in hide_link_regex: {pattern} ({e})"))
        })
        .collect()
}

pub(crate) fn get_config_value(store: &BlogData, key: &str) -> Option<String> {
    let full_key = format!("config.{key}");
    store
        .meta()
        .items()
        .into_iter()
        .find(|e| e.key == full_key)
        .map(|e| e.value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_store(dir: &std::path::Path) -> BlogData {
        BlogData::open(dir).unwrap()
    }

    #[test]
    fn test_hidden_link_regexes_no_config_returns_empty() {
        let dir = TempDir::new().unwrap();
        let store = test_store(dir.path());
        let rules = hidden_link_regexes(&store).unwrap();
        assert!(rules.is_empty());
    }

    #[test]
    fn test_hidden_link_regexes_reads_from_store() {
        let dir = TempDir::new().unwrap();
        let mut store = test_store(dir.path());
        store
            .transact("set config", |tx| {
                tx.meta.upsert(MetaEntry {
                    key: "config.hide_link_regex".to_string(),
                    value: r#"["/shorts/", "youtube\\.com/live/"]"#.to_string(),
                });
                Ok(())
            })
            .unwrap();

        let rules = hidden_link_regexes(&store).unwrap();
        assert_eq!(rules.len(), 2);
        assert!(rules[0].is_match("https://youtube.com/shorts/abc"));
        assert!(rules[1].is_match("https://youtube.com/live/abc"));
    }

    #[test]
    fn test_hidden_link_regexes_rejects_invalid_json() {
        let dir = TempDir::new().unwrap();
        let mut store = test_store(dir.path());
        store
            .transact("set config", |tx| {
                tx.meta.upsert(MetaEntry {
                    key: "config.hide_link_regex".to_string(),
                    value: "not json".to_string(),
                });
                Ok(())
            })
            .unwrap();

        let err = hidden_link_regexes(&store).unwrap_err().to_string();
        assert!(err.contains("hide_link_regex"), "got: {err}");
    }

    #[test]
    fn test_hidden_link_regexes_rejects_invalid_regex() {
        let dir = TempDir::new().unwrap();
        let mut store = test_store(dir.path());
        store
            .transact("set config", |tx| {
                tx.meta.upsert(MetaEntry {
                    key: "config.hide_link_regex".to_string(),
                    value: r#"["["]"#.to_string(),
                });
                Ok(())
            })
            .unwrap();

        let err = hidden_link_regexes(&store).unwrap_err().to_string();
        assert!(err.contains("Invalid regex"), "got: {err}");
    }
}
