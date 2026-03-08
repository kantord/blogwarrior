pub mod index;
pub mod schema;

use schema::{BlogDataSchema, MetaEntry};
use synctato::Store;

pub(crate) type BlogData = Store<BlogDataSchema>;
pub(crate) type Transaction<'a> = schema::BlogDataSchemaTransaction<'a>;

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
