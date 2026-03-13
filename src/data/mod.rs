pub mod index;
pub mod schema;

use std::path::PathBuf;

use regex::Regex;
use schema::{BlogDataSchema, MetaEntry};
use serde::Deserialize;
use synctato::Store;

pub(crate) type BlogData = Store<BlogDataSchema>;
pub(crate) type Transaction<'a> = schema::BlogDataSchemaTransaction<'a>;

pub(crate) const SCHEMA_VERSION: u32 = 1;
const CONFIG_FILE_NAME: &str = "config.toml";

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
            let db_version: u32 = entry.value.parse().unwrap_or(0);
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

#[derive(Debug, Deserialize)]
struct ConfigFile {
    #[serde(default)]
    filters: FiltersSection,
}

#[derive(Debug, Default, Deserialize)]
struct FiltersSection {
    #[serde(default)]
    hide_link_regex: Vec<String>,
}

pub(crate) fn hidden_link_regexes() -> anyhow::Result<Vec<Regex>> {
    let path = config_file_path()?;
    hidden_link_regexes_from_path(&path)
}

fn config_file_path() -> anyhow::Result<PathBuf> {
    dirs::config_dir()
        .map(|d| d.join("blogtato").join(CONFIG_FILE_NAME))
        .ok_or_else(|| anyhow::anyhow!("could not determine config directory; set XDG_CONFIG_HOME"))
}

fn hidden_link_regexes_from_path(path: &std::path::Path) -> anyhow::Result<Vec<Regex>> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        if path.exists() {
            anyhow::bail!("Failed to read {}", path.display());
        }
        return Ok(Vec::new());
    };

    let config: ConfigFile =
        toml::from_str(&raw).map_err(|e| anyhow::anyhow!("Invalid {}: {}", path.display(), e))?;

    config
        .filters
        .hide_link_regex
        .into_iter()
        .map(|pattern| {
            Regex::new(&pattern).map_err(|e| {
                anyhow::anyhow!("Invalid regex in {}: {} ({})", path.display(), pattern, e)
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_hidden_link_regexes_missing_file_returns_empty() {
        let dir = TempDir::new().unwrap();
        let rules = hidden_link_regexes_from_path(&dir.path().join(CONFIG_FILE_NAME)).unwrap();
        assert!(rules.is_empty());
    }

    #[test]
    fn test_hidden_link_regexes_reads_config_toml() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(CONFIG_FILE_NAME);
        std::fs::write(
            &path,
            r#"[filters]
hide_link_regex = ["/shorts/", "youtube\\.com/live/"]"#,
        )
        .unwrap();

        let rules = hidden_link_regexes_from_path(&path).unwrap();
        assert_eq!(rules.len(), 2);
        assert!(rules[0].is_match("https://youtube.com/shorts/abc"));
        assert!(rules[1].is_match("https://youtube.com/live/abc"));
    }

    #[test]
    fn test_hidden_link_regexes_rejects_invalid_toml() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(CONFIG_FILE_NAME);
        std::fs::write(&path, "not = [valid").unwrap();

        let err = hidden_link_regexes_from_path(&path)
            .unwrap_err()
            .to_string();
        assert!(err.contains(CONFIG_FILE_NAME), "got: {err}");
    }

    #[test]
    fn test_hidden_link_regexes_rejects_invalid_regex() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(CONFIG_FILE_NAME);
        std::fs::write(
            &path,
            r#"[filters]
hide_link_regex = ["["]"#,
        )
        .unwrap();

        let err = hidden_link_regexes_from_path(&path)
            .unwrap_err()
            .to_string();
        assert!(err.contains("Invalid regex"), "got: {err}");
    }
}
