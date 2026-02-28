use std::collections::HashMap;
use std::fs;
use std::io::BufRead;
use std::path::{Path, PathBuf};

use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub trait TableRow: Clone + PartialEq + Serialize + DeserializeOwned {
    fn key(&self) -> String;

    const TABLE_NAME: &'static str;
    const SHARD_CHARACTERS: usize;
    const EXPECTED_CAPACITY: usize;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Row<T> {
    Tombstone {
        id: String,
        deleted_at: DateTime<Utc>,
    },
    Live {
        id: String,
        #[serde(flatten)]
        inner: T,
        #[serde(default)]
        updated_at: Option<DateTime<Utc>>,
    },
}

impl<T> Row<T> {
    pub fn id(&self) -> &str {
        match self {
            Row::Live { id, .. } | Row::Tombstone { id, .. } => id,
        }
    }
}

fn hash_id(raw: &str, id_length: usize) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    format!("{:x}", hasher.finalize())[..id_length].to_string()
}

fn id_length_for_capacity(expected_items: usize) -> usize {
    if expected_items <= 1 {
        return 4;
    }
    let k = expected_items as f64;
    let n = (500.0 * k * k).ln() / 16_f64.ln();
    (n.ceil() as usize).max(4)
}

pub struct Table<T: TableRow> {
    items: HashMap<String, Row<T>>,
    dir: PathBuf,
    shard_characters: usize,
    id_length: usize,
}

impl<T: TableRow> Table<T> {
    pub fn load(store: &Path) -> anyhow::Result<Self> {
        let dir = store.join(T::TABLE_NAME);
        let id_length = id_length_for_capacity(T::EXPECTED_CAPACITY);
        let mut table = Self {
            items: HashMap::new(),
            dir,
            shard_characters: T::SHARD_CHARACTERS,
            id_length,
        };
        if let Ok(entries) = fs::read_dir(&table.dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(fname) = path.file_name().and_then(|f| f.to_str())
                    && fname.starts_with("items_")
                    && fname.ends_with(".jsonl")
                    && let Ok(file) = fs::File::open(&path)
                {
                    for line in std::io::BufReader::new(file).lines() {
                        let line = line.context("failed to read line")?;
                        if line.trim().is_empty() {
                            continue;
                        }
                        let row: Row<T> = serde_json::from_str(&line).with_context(|| {
                            format!("failed to parse entry in {}", path.display())
                        })?;
                        table.items.insert(row.id().to_string(), row);
                    }
                }
            }
        }
        Ok(table)
    }

    pub fn upsert(&mut self, item: T) {
        let id = hash_id(&item.key(), self.id_length);

        if let Some(Row::Live {
            inner: existing, ..
        }) = self.items.get(&id)
            && item == *existing
        {
            return;
        }

        self.items.insert(
            id.clone(),
            Row::Live {
                id,
                inner: item,
                updated_at: Some(Utc::now()),
            },
        );
    }

    pub fn delete(&mut self, key: &str) -> Option<String> {
        let id = hash_id(key, self.id_length);
        if !matches!(self.items.get(&id), Some(Row::Live { .. })) {
            return None;
        }
        self.items.insert(
            id.clone(),
            Row::Tombstone {
                id: id.clone(),
                deleted_at: Utc::now(),
            },
        );
        Some(id)
    }

    pub fn id_of(&self, item: &T) -> String {
        hash_id(&item.key(), self.id_length)
    }

    fn shard_key(&self, id: &str) -> String {
        let end = self.shard_characters.min(id.len());
        id[..end].to_string()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        fs::create_dir_all(&self.dir).context("failed to create table directory")?;

        // Group items by shard key
        let mut shards: HashMap<String, Vec<&Row<T>>> = HashMap::new();
        for row in self.items.values() {
            let key = self.shard_key(row.id());
            shards.entry(key).or_default().push(row);
        }

        // Phase 1: Write new shards to temporary files.
        // If this fails, old shard files remain untouched.
        let mut tmp_paths = Vec::new();
        for (prefix, rows) in &mut shards {
            rows.sort_by(|a, b| a.id().cmp(b.id()));
            let mut out = String::new();
            for row in rows.iter() {
                out.push_str(&serde_json::to_string(row).context("failed to serialize item")?);
                out.push('\n');
            }
            let tmp_path = self.dir.join(format!("items_{}.jsonl.tmp", prefix));
            if let Err(e) = fs::write(&tmp_path, out) {
                // Clean up the failed temp file and any previously written ones
                let _ = fs::remove_file(&tmp_path);
                for (p, _) in &tmp_paths {
                    let _ = fs::remove_file(p);
                }
                return Err(e).context("failed to write shard file");
            }
            tmp_paths.push((tmp_path, format!("items_{}.jsonl", prefix)));
        }

        // Phase 2: Remove old shard files
        if let Ok(entries) = fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(fname) = path.file_name().and_then(|f| f.to_str())
                    && fname.starts_with("items_")
                    && fname.ends_with(".jsonl")
                    && !fname.ends_with(".tmp")
                {
                    fs::remove_file(&path).context("failed to remove old shard file")?;
                }
            }
        }

        // Phase 3: Rename temp files to final names
        for (tmp_path, final_name) in tmp_paths {
            let final_path = self.dir.join(final_name);
            fs::rename(&tmp_path, &final_path).context("failed to rename shard file")?;
        }

        Ok(())
    }

    pub fn items(&self) -> Vec<T> {
        self.items
            .values()
            .filter_map(|r| match r {
                Row::Live { inner, .. } => Some(inner.clone()),
                Row::Tombstone { .. } => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use tempfile::TempDir;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestItem {
        #[serde(default)]
        raw_id: String,
        title: String,
    }

    impl TableRow for TestItem {
        fn key(&self) -> String {
            self.raw_id.clone()
        }

        const TABLE_NAME: &'static str = "t";
        const SHARD_CHARACTERS: usize = 2;
        const EXPECTED_CAPACITY: usize = 1000;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct UnshardedItem {
        #[serde(default)]
        raw_id: String,
        title: String,
    }

    impl TableRow for UnshardedItem {
        fn key(&self) -> String {
            self.raw_id.clone()
        }

        const TABLE_NAME: &'static str = "t";
        const SHARD_CHARACTERS: usize = 0;
        const EXPECTED_CAPACITY: usize = 1000;
    }

    fn make_item(raw_id: &str, title: &str) -> TestItem {
        TestItem {
            raw_id: raw_id.to_string(),
            title: title.to_string(),
        }
    }

    #[test]
    fn test_upsert_hashes_id() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::<TestItem>::load(dir.path()).unwrap();
        let item = make_item("raw-id", "Post");
        table.upsert(item.clone());
        assert_eq!(
            table.id_of(&item),
            hash_id(
                "raw-id",
                id_length_for_capacity(TestItem::EXPECTED_CAPACITY)
            )
        );
        assert_eq!(table.items().len(), 1);
    }

    #[test]
    fn test_upsert_overwrites_existing() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::<TestItem>::load(dir.path()).unwrap();
        table.upsert(make_item("same-id", "Original"));
        table.upsert(make_item("same-id", "Updated"));
        let items = table.items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Updated");
    }

    #[test]
    fn test_load_save_roundtrip() {
        let dir = TempDir::new().unwrap();

        let mut table = Table::<TestItem>::load(dir.path()).unwrap();
        table.upsert(make_item("id-1", "First"));
        table.upsert(make_item("id-2", "Second"));
        table.save().unwrap();

        let loaded = Table::<TestItem>::load(dir.path()).unwrap();
        assert_eq!(loaded.items().len(), 2);

        let titles: Vec<String> = loaded.items().iter().map(|i| i.title.clone()).collect();
        assert!(titles.contains(&"First".to_string()));
        assert!(titles.contains(&"Second".to_string()));
    }

    #[test]
    fn test_load_nonexistent_file() {
        let dir = TempDir::new().unwrap();
        let table = Table::<TestItem>::load(dir.path()).unwrap();
        assert_eq!(table.items().len(), 0);
    }

    /// Read all lines from all shard files in the table directory.
    fn read_lines(dir: &TempDir, name: &str) -> Vec<String> {
        let table_dir = dir.path().join(name);
        let mut lines = Vec::new();
        if let Ok(entries) = fs::read_dir(&table_dir) {
            let mut paths: Vec<_> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|f| f.to_str())
                        .is_some_and(|f| f.starts_with("items_") && f.ends_with(".jsonl"))
                })
                .collect();
            paths.sort();
            for path in paths {
                for line in fs::read_to_string(&path)
                    .unwrap()
                    .lines()
                    .filter(|l| !l.is_empty())
                {
                    lines.push(line.to_string());
                }
            }
        }
        lines
    }

    fn ids_from_lines(lines: &[String]) -> Vec<String> {
        lines
            .iter()
            .map(|l| {
                let v: serde_json::Value = serde_json::from_str(l).unwrap();
                v["id"].as_str().unwrap().to_string()
            })
            .collect()
    }

    /// List shard file names in a table directory.
    fn shard_files(dir: &TempDir, name: &str) -> Vec<String> {
        let table_dir = dir.path().join(name);
        let mut names: Vec<String> = fs::read_dir(&table_dir)
            .unwrap()
            .flatten()
            .filter_map(|e| {
                let fname = e.file_name().to_str()?.to_string();
                if fname.starts_with("items_") && fname.ends_with(".jsonl") {
                    Some(fname)
                } else {
                    None
                }
            })
            .collect();
        names.sort();
        names
    }

    #[test]
    fn test_save_sorts_items_by_id() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::<TestItem>::load(dir.path()).unwrap();
        table.upsert(make_item("zzz", "Last"));
        table.upsert(make_item("aaa", "First"));
        table.upsert(make_item("mmm", "Middle"));
        table.save().unwrap();

        let ids = ids_from_lines(&read_lines(&dir, "t"));
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
    }

    #[test]
    fn test_save_sort_order_is_stable_across_roundtrips() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::<TestItem>::load(dir.path()).unwrap();
        table.upsert(make_item("c", "C"));
        table.upsert(make_item("a", "A"));
        table.upsert(make_item("b", "B"));
        table.save().unwrap();

        let ids1 = ids_from_lines(&read_lines(&dir, "t"));

        let loaded = Table::<TestItem>::load(dir.path()).unwrap();
        loaded.save().unwrap();

        let ids2 = ids_from_lines(&read_lines(&dir, "t"));
        assert_eq!(ids1, ids2);
    }

    #[test]
    fn test_save_sort_order_preserved_after_upsert() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::<TestItem>::load(dir.path()).unwrap();
        table.upsert(make_item("b", "B"));
        table.upsert(make_item("a", "A"));
        table.save().unwrap();

        let mut table = Table::<TestItem>::load(dir.path()).unwrap();
        table.upsert(make_item("c", "C"));
        table.save().unwrap();

        let ids = ids_from_lines(&read_lines(&dir, "t"));
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
    }

    #[test]
    fn test_save_single_item_sorted() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::<TestItem>::load(dir.path()).unwrap();
        table.upsert(make_item("only", "Only"));
        table.save().unwrap();

        let ids = ids_from_lines(&read_lines(&dir, "t"));
        assert_eq!(ids.len(), 1);
    }

    #[test]
    fn test_save_empty_table() {
        let dir = TempDir::new().unwrap();
        let table = Table::<TestItem>::load(dir.path()).unwrap();
        table.save().unwrap();

        let lines = read_lines(&dir, "t");
        assert!(lines.is_empty());
    }

    #[test]
    fn test_items_land_in_correct_shard_files() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::<TestItem>::load(dir.path()).unwrap();
        // hash_id produces 14-char hex strings; we use pre-hashed ids for predictability
        table
            .items
            .insert("aabb11".to_string(), make_row_with_id("aabb11", "Item AA"));
        table
            .items
            .insert("aabb22".to_string(), make_row_with_id("aabb22", "Item AA2"));
        table
            .items
            .insert("ccdd33".to_string(), make_row_with_id("ccdd33", "Item CC"));
        table.save().unwrap();

        let files = shard_files(&dir, "t");
        assert_eq!(files, vec!["items_aa.jsonl", "items_cc.jsonl"]);

        // Check items_aa.jsonl has 2 items
        let aa_content = fs::read_to_string(dir.path().join("t").join("items_aa.jsonl")).unwrap();
        let aa_lines: Vec<&str> = aa_content.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(aa_lines.len(), 2);

        // Check items_cc.jsonl has 1 item
        let cc_content = fs::read_to_string(dir.path().join("t").join("items_cc.jsonl")).unwrap();
        let cc_lines: Vec<&str> = cc_content.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(cc_lines.len(), 1);
    }

    #[test]
    fn test_load_reads_from_multiple_shard_files() {
        let dir = TempDir::new().unwrap();
        let table_dir = dir.path().join("t");
        fs::create_dir_all(&table_dir).unwrap();

        // Write two separate shard files
        let item1 = r#"{"id":"aa1111","title":"From AA"}"#;
        let item2 = r#"{"id":"bb2222","title":"From BB"}"#;
        fs::write(table_dir.join("items_aa.jsonl"), format!("{}\n", item1)).unwrap();
        fs::write(table_dir.join("items_bb.jsonl"), format!("{}\n", item2)).unwrap();

        let table = Table::<TestItem>::load(dir.path()).unwrap();
        assert_eq!(table.items().len(), 2);
        let titles: Vec<String> = table.items().iter().map(|i| i.title.clone()).collect();
        assert!(titles.contains(&"From AA".to_string()));
        assert!(titles.contains(&"From BB".to_string()));
    }

    #[test]
    fn test_roundtrip_with_sharding_preserves_all_items() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::<TestItem>::load(dir.path()).unwrap();
        table.upsert(make_item("alpha", "Alpha"));
        table.upsert(make_item("beta", "Beta"));
        table.upsert(make_item("gamma", "Gamma"));
        table.save().unwrap();

        let loaded = Table::<TestItem>::load(dir.path()).unwrap();
        assert_eq!(loaded.items().len(), 3);
        let titles: Vec<String> = loaded.items().iter().map(|i| i.title.clone()).collect();
        assert!(titles.contains(&"Alpha".to_string()));
        assert!(titles.contains(&"Beta".to_string()));
        assert!(titles.contains(&"Gamma".to_string()));
    }

    #[test]
    fn test_shard_characters_zero_puts_everything_in_items_empty() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::<UnshardedItem>::load(dir.path()).unwrap();
        table.items.insert(
            "aabb11".to_string(),
            Row::Live {
                id: "aabb11".to_string(),
                inner: UnshardedItem {
                    raw_id: String::new(),
                    title: "Item 1".to_string(),
                },
                updated_at: None,
            },
        );
        table.items.insert(
            "ccdd22".to_string(),
            Row::Live {
                id: "ccdd22".to_string(),
                inner: UnshardedItem {
                    raw_id: String::new(),
                    title: "Item 2".to_string(),
                },
                updated_at: None,
            },
        );
        table.save().unwrap();

        let files = shard_files(&dir, "t");
        assert_eq!(files, vec!["items_.jsonl"]);

        let content = fs::read_to_string(dir.path().join("t").join("items_.jsonl")).unwrap();
        let lines: Vec<&str> = content.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_save_cleans_up_old_shard_files() {
        let dir = TempDir::new().unwrap();
        let table_dir = dir.path().join("t");
        fs::create_dir_all(&table_dir).unwrap();

        // Create an old shard file with valid data that won't be needed after re-shard
        let old_item = r#"{"id":"zz9999","title":"Old"}"#;
        fs::write(table_dir.join("items_zz.jsonl"), format!("{}\n", old_item)).unwrap();

        // Load picks up the old item, then we replace all items with a new one
        let mut table = Table::<TestItem>::load(dir.path()).unwrap();
        table.items.clear();
        table
            .items
            .insert("aabb11".to_string(), make_row_with_id("aabb11", "Item AA"));
        table.save().unwrap();

        let files = shard_files(&dir, "t");
        assert_eq!(files, vec!["items_aa.jsonl"]);
        assert!(!table_dir.join("items_zz.jsonl").exists());
    }

    #[test]
    fn test_upsert_same_id_overwrites() {
        // Two items with the same raw ID should produce the same hash,
        // so the second upsert overwrites the first. This is correct
        // table behavior — it's the caller's job to provide distinct IDs.
        let dir = TempDir::new().unwrap();
        let mut table = Table::<TestItem>::load(dir.path()).unwrap();
        table.upsert(make_item("same", "First"));
        table.upsert(make_item("same", "Second"));
        let items = table.items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Second");
    }

    fn get_updated_at(table: &Table<TestItem>) -> Option<DateTime<Utc>> {
        match table.items.values().next().unwrap() {
            Row::Live { updated_at, .. } => *updated_at,
            Row::Tombstone { .. } => None,
        }
    }

    #[test]
    fn test_upsert_sets_updated_at_on_new_item() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::<TestItem>::load(dir.path()).unwrap();
        table.upsert(make_item("new", "New Item"));
        assert!(get_updated_at(&table).is_some());
    }

    #[test]
    fn test_upsert_preserves_updated_at_when_unchanged() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::<TestItem>::load(dir.path()).unwrap();
        table.upsert(make_item("x", "Same"));
        let ts1 = get_updated_at(&table);

        // Upsert identical content — updated_at should not change
        table.upsert(make_item("x", "Same"));
        let ts2 = get_updated_at(&table);
        assert_eq!(ts1, ts2);
    }

    #[test]
    fn test_upsert_updates_updated_at_when_content_changes() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::<TestItem>::load(dir.path()).unwrap();
        table.upsert(make_item("x", "Original"));
        let ts1 = get_updated_at(&table);

        table.upsert(make_item("x", "Changed"));
        let ts2 = get_updated_at(&table);
        assert_ne!(ts1, ts2);
        assert!(ts2 > ts1);
    }

    #[test]
    fn test_updated_at_survives_save_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::<TestItem>::load(dir.path()).unwrap();
        table.upsert(make_item("x", "Item"));
        let ts = get_updated_at(&table);
        table.save().unwrap();

        let loaded = Table::<TestItem>::load(dir.path()).unwrap();
        assert_eq!(get_updated_at(&loaded), ts);
    }

    #[test]
    fn test_upsert_unchanged_after_roundtrip() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::<TestItem>::load(dir.path()).unwrap();
        table.upsert(make_item("x", "Item"));
        table.save().unwrap();

        let mut loaded = Table::<TestItem>::load(dir.path()).unwrap();
        let ts_before = get_updated_at(&loaded);

        // Re-upsert same content after loading from disk
        loaded.upsert(make_item("x", "Item"));
        let ts_after = get_updated_at(&loaded);
        assert_eq!(ts_before, ts_after);
    }

    #[test]
    fn test_delete_removes_from_items() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::<TestItem>::load(dir.path()).unwrap();
        table.upsert(make_item("x", "Item"));
        assert_eq!(table.items().len(), 1);

        table.delete("x");
        assert_eq!(table.items().len(), 0);
    }

    #[test]
    fn test_delete_tombstone_survives_roundtrip() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::<TestItem>::load(dir.path()).unwrap();
        table.upsert(make_item("x", "Item"));
        table.delete("x");
        table.save().unwrap();

        let loaded = Table::<TestItem>::load(dir.path()).unwrap();
        assert_eq!(loaded.items().len(), 0);
    }

    #[test]
    fn test_upsert_resurrects_deleted_item() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::<TestItem>::load(dir.path()).unwrap();
        table.upsert(make_item("x", "Original"));
        table.delete("x");
        assert_eq!(table.items().len(), 0);

        table.upsert(make_item("x", "Resurrected"));
        let items = table.items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Resurrected");
    }

    #[test]
    fn test_upsert_resurrects_after_roundtrip() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::<TestItem>::load(dir.path()).unwrap();
        table.upsert(make_item("x", "Original"));
        table.delete("x");
        table.save().unwrap();

        let mut loaded = Table::<TestItem>::load(dir.path()).unwrap();
        assert_eq!(loaded.items().len(), 0);

        loaded.upsert(make_item("x", "Back"));
        let items = loaded.items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Back");
    }

    #[test]
    fn test_delete_nonexistent_key_returns_none() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::<TestItem>::load(dir.path()).unwrap();
        table.upsert(make_item("a", "Keep"));
        assert!(table.delete("never-added").is_none());
        assert_eq!(table.items().len(), 1);
    }

    #[test]
    fn test_delete_mixed_with_live() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::<TestItem>::load(dir.path()).unwrap();
        table.upsert(make_item("a", "Keep"));
        table.upsert(make_item("b", "Delete"));
        table.upsert(make_item("c", "Also Keep"));
        table.delete("b");

        let items = table.items();
        assert_eq!(items.len(), 2);
        let titles: Vec<&str> = items.iter().map(|i| i.title.as_str()).collect();
        assert!(titles.contains(&"Keep"));
        assert!(titles.contains(&"Also Keep"));
        assert!(!titles.contains(&"Delete"));
    }

    #[test]
    fn test_load_truncated_json_returns_error() {
        let dir = TempDir::new().unwrap();
        let table_dir = dir.path().join("t");
        fs::create_dir_all(&table_dir).unwrap();

        fs::write(
            table_dir.join("items_aa.jsonl"),
            "{\"id\":\"abc\",\"title\":\"tr\n",
        )
        .unwrap();

        let result = Table::<TestItem>::load(dir.path());
        assert!(result.is_err());
        let err_msg = format!("{:#}", result.err().unwrap());
        assert!(
            err_msg.contains("items_aa.jsonl"),
            "error should mention file path, got: {}",
            err_msg
        );
    }

    #[test]
    fn test_load_completely_invalid_content_returns_error() {
        let dir = TempDir::new().unwrap();
        let table_dir = dir.path().join("t");
        fs::create_dir_all(&table_dir).unwrap();

        fs::write(table_dir.join("items_aa.jsonl"), "not json at all\n").unwrap();

        let result = Table::<TestItem>::load(dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_load_empty_lines_between_valid_entries() {
        let dir = TempDir::new().unwrap();
        let table_dir = dir.path().join("t");
        fs::create_dir_all(&table_dir).unwrap();

        let content = format!(
            "{}\n\n{}\n\n",
            r#"{"id":"aa1111","title":"First"}"#,
            r#"{"id":"bb2222","title":"Second"}"#,
        );
        fs::write(table_dir.join("items_aa.jsonl"), content).unwrap();

        let table = Table::<TestItem>::load(dir.path()).unwrap();
        assert_eq!(table.items().len(), 2);
    }

    #[test]
    fn test_load_mixed_valid_and_invalid_lines_returns_error() {
        let dir = TempDir::new().unwrap();
        let table_dir = dir.path().join("t");
        fs::create_dir_all(&table_dir).unwrap();

        let content = format!(
            "{}\n{}\n",
            r#"{"id":"aa1111","title":"Valid"}"#, "corrupted line",
        );
        fs::write(table_dir.join("items_aa.jsonl"), content).unwrap();

        let result = Table::<TestItem>::load(dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_load_extra_unknown_fields_ignored() {
        let dir = TempDir::new().unwrap();
        let table_dir = dir.path().join("t");
        fs::create_dir_all(&table_dir).unwrap();

        let content =
            r#"{"id":"aa1111","title":"Post","extra_field":"should be ignored","another":42}"#;
        fs::write(
            table_dir.join("items_aa.jsonl"),
            format!("{}\n", content),
        )
        .unwrap();

        let table = Table::<TestItem>::load(dir.path()).unwrap();
        assert_eq!(table.items().len(), 1);
        assert_eq!(table.items()[0].title, "Post");
    }

    #[cfg(unix)]
    #[test]
    fn test_failed_save_preserves_previous_data() {
        let dir = TempDir::new().unwrap();

        // Save initial data
        let mut table = Table::<TestItem>::load(dir.path()).unwrap();
        table
            .items
            .insert("aabb11".to_string(), make_row_with_id("aabb11", "Original"));
        table.save().unwrap();

        // Verify initial data is saved
        let loaded = Table::<TestItem>::load(dir.path()).unwrap();
        assert_eq!(loaded.items().len(), 1);

        // Fork a child process to attempt save() with RLIMIT_FSIZE=8.
        // RLIMIT_FSIZE is process-wide, so we isolate it in a subprocess to
        // avoid interfering with other tests running in parallel.
        // The child sets a file size limit of 8 bytes — enough to create a file
        // but too small for any real JSONL row — then attempts save().
        // Deletions (unlink) are unaffected by RLIMIT_FSIZE, so save() will
        // delete old shards but fail writing new ones — simulating disk-full.
        let dir_path = dir.path().to_path_buf();
        let child_status = unsafe { libc::fork() };
        match child_status {
            -1 => panic!("fork failed"),
            0 => {
                // Child process: set RLIMIT_FSIZE and attempt save()
                unsafe {
                    libc::signal(libc::SIGXFSZ, libc::SIG_IGN);
                    let limit = libc::rlimit {
                        rlim_cur: 8,
                        rlim_max: libc::RLIM_INFINITY,
                    };
                    libc::setrlimit(libc::RLIMIT_FSIZE, &limit);
                }
                let table = Table::<TestItem>::load(&dir_path).unwrap();
                let _ = table.save();
                std::process::exit(0);
            }
            child_pid => {
                // Parent: wait for child
                let mut wstatus: libc::c_int = 0;
                unsafe {
                    libc::waitpid(child_pid, &mut wstatus, 0);
                }
            }
        }

        // Original data should still be loadable after the child's failed save
        let recovered = Table::<TestItem>::load(dir.path())
            .expect("load should not fail after a failed save");
        assert_eq!(
            recovered.items().len(),
            1,
            "original data should survive a failed save()"
        );
        assert_eq!(recovered.items()[0].title, "Original");
    }

    /// Helper to create a Row with a pre-set id (no hashing).
    fn make_row_with_id(id: &str, title: &str) -> Row<TestItem> {
        Row::Live {
            id: id.to_string(),
            inner: TestItem {
                raw_id: String::new(),
                title: title.to_string(),
            },
            updated_at: None,
        }
    }
}
