use crate::data::BlogData;
use crate::data::schema::MetaEntry;

const CONFIG_PREFIX: &str = "config.";

pub(crate) fn cmd_config_set(store: &mut BlogData, key: &str, value: &str) -> anyhow::Result<()> {
    store.transact(&format!("config set {key}"), |tx| {
        tx.meta.upsert(MetaEntry {
            key: format!("{CONFIG_PREFIX}{key}"),
            value: value.to_string(),
        });
        Ok(())
    })
}

pub(crate) fn cmd_config_get(store: &BlogData, key: &str) -> anyhow::Result<()> {
    let full_key = format!("{CONFIG_PREFIX}{key}");
    match store.meta().items().into_iter().find(|e| e.key == full_key) {
        Some(entry) => println!("{}", entry.value),
        None => anyhow::bail!("No value set for '{key}'"),
    }
    Ok(())
}

pub(crate) fn cmd_config_unset(store: &mut BlogData, key: &str) -> anyhow::Result<()> {
    let full_key = format!("{CONFIG_PREFIX}{key}");
    let exists = store.meta().items().into_iter().any(|e| e.key == full_key);
    anyhow::ensure!(exists, "No value set for '{key}'");
    store.transact(&format!("config unset {key}"), |tx| {
        tx.meta.delete(&full_key);
        Ok(())
    })
}

pub(crate) fn get_config_value(store: &BlogData, key: &str) -> Option<String> {
    let full_key = format!("{CONFIG_PREFIX}{key}");
    store
        .meta()
        .items()
        .into_iter()
        .find(|e| e.key == full_key)
        .map(|e| e.value)
}
