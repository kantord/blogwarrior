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
    match crate::data::get_config_value(store, key) {
        Some(value) => println!("{value}"),
        None => anyhow::bail!("No value set for '{key}'"),
    }
    Ok(())
}

pub(crate) fn cmd_config_unset(store: &mut BlogData, key: &str) -> anyhow::Result<()> {
    let full_key = format!("{CONFIG_PREFIX}{key}");
    let exists = store.meta().iter().any(|(_, e)| e.key == full_key);
    anyhow::ensure!(exists, "No value set for '{key}'");
    store.transact(&format!("config unset {key}"), |tx| {
        tx.meta.delete(&full_key);
        Ok(())
    })
}
