use std::collections::HashMap;
use std::fs;
use std::io::BufRead;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;
use sha2::{Digest, Sha256};

pub trait TableRow: Clone + Serialize + DeserializeOwned {
    fn id(&self) -> &str;
    fn set_id(&mut self, id: String);
}

pub fn hash_id(raw: &str, id_length: usize) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    format!("{:x}", hasher.finalize())[..id_length].to_string()
}

pub fn id_length_for_capacity(expected_items: usize) -> usize {
    if expected_items <= 1 {
        return 4;
    }
    let k = expected_items as f64;
    let n = (500.0 * k * k).ln() / 16_f64.ln();
    (n.ceil() as usize).max(4)
}

pub struct Table<T: TableRow> {
    items: HashMap<String, T>,
    dir: PathBuf,
    shard_characters: usize,
    id_length: usize,
}

impl<T: TableRow> Table<T> {
    pub fn load(store: &Path, name: &str, shard_characters: usize, expected_items: usize) -> Self {
        let dir = store.join(name);
        let id_length = id_length_for_capacity(expected_items);
        let mut table = Self {
            items: HashMap::new(),
            dir,
            shard_characters,
            id_length,
        };
        if let Ok(entries) = fs::read_dir(&table.dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(fname) = path.file_name().and_then(|f| f.to_str()) {
                    if fname.starts_with("items_") && fname.ends_with(".jsonl") {
                        if let Ok(file) = fs::File::open(&path) {
                            for line in std::io::BufReader::new(file).lines() {
                                let line = line.expect("failed to read line");
                                if line.trim().is_empty() {
                                    continue;
                                }
                                let item: T = serde_json::from_str(&line)
                                    .expect("failed to parse entry");
                                table.items.insert(item.id().to_string(), item);
                            }
                        }
                    }
                }
            }
        }
        table
    }

    pub fn upsert(&mut self, mut item: T) {
        item.set_id(hash_id(item.id(), self.id_length));
        self.items.insert(item.id().to_string(), item);
    }

    pub fn update(&mut self, item: T) {
        self.items.insert(item.id().to_string(), item);
    }

    fn shard_key(&self, id: &str) -> String {
        let end = self.shard_characters.min(id.len());
        id[..end].to_string()
    }

    pub fn save(&self) {
        fs::create_dir_all(&self.dir).expect("failed to create table directory");

        // Remove all existing shard files
        if let Ok(entries) = fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(fname) = path.file_name().and_then(|f| f.to_str()) {
                    if fname.starts_with("items_") && fname.ends_with(".jsonl") {
                        fs::remove_file(&path).expect("failed to remove old shard file");
                    }
                }
            }
        }

        // Group items by shard key
        let mut shards: HashMap<String, Vec<&T>> = HashMap::new();
        for item in self.items.values() {
            let key = self.shard_key(item.id());
            shards.entry(key).or_default().push(item);
        }

        // Write each shard
        for (prefix, mut items) in shards {
            items.sort_by(|a, b| a.id().cmp(b.id()));
            let mut out = String::new();
            for item in items {
                out.push_str(&serde_json::to_string(item).expect("failed to serialize item"));
                out.push('\n');
            }
            let path = self.dir.join(format!("items_{}.jsonl", prefix));
            fs::write(&path, out).expect("failed to write shard file");
        }
    }

    pub fn items(&self) -> Vec<T> {
        self.items.values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use tempfile::TempDir;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestItem {
        id: String,
        title: String,
    }

    impl TableRow for TestItem {
        fn id(&self) -> &str {
            &self.id
        }
        fn set_id(&mut self, id: String) {
            self.id = id;
        }
    }

    fn make_item(id: &str, title: &str) -> TestItem {
        TestItem {
            id: id.to_string(),
            title: title.to_string(),
        }
    }

    #[test]
    fn test_upsert_hashes_id() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::<TestItem>::load(dir.path(), "test_table", 2, 1000);
        table.upsert(make_item("raw-id", "Post"));
        let items = table.items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, hash_id("raw-id", id_length_for_capacity(1000)));
    }

    #[test]
    fn test_upsert_overwrites_existing() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::<TestItem>::load(dir.path(), "test_table", 2, 1000);
        table.upsert(make_item("same-id", "Original"));
        table.upsert(make_item("same-id", "Updated"));
        let items = table.items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Updated");
    }

    #[test]
    fn test_load_save_roundtrip() {
        let dir = TempDir::new().unwrap();

        let mut table = Table::<TestItem>::load(dir.path(), "test_table", 2, 1000);
        table.upsert(make_item("id-1", "First"));
        table.upsert(make_item("id-2", "Second"));
        table.save();

        let loaded = Table::<TestItem>::load(dir.path(), "test_table", 2, 1000);
        assert_eq!(loaded.items().len(), 2);

        let titles: Vec<String> = loaded.items().iter().map(|i| i.title.clone()).collect();
        assert!(titles.contains(&"First".to_string()));
        assert!(titles.contains(&"Second".to_string()));
    }

    #[test]
    fn test_load_nonexistent_file() {
        let dir = TempDir::new().unwrap();
        let table = Table::<TestItem>::load(dir.path(), "nonexistent", 2, 1000);
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
        let mut table = Table::<TestItem>::load(dir.path(), "t", 2, 1000);
        table.upsert(make_item("zzz", "Last"));
        table.upsert(make_item("aaa", "First"));
        table.upsert(make_item("mmm", "Middle"));
        table.save();

        let ids = ids_from_lines(&read_lines(&dir, "t"));
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
    }

    #[test]
    fn test_save_sort_order_is_stable_across_roundtrips() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::<TestItem>::load(dir.path(), "t", 2, 1000);
        table.upsert(make_item("c", "C"));
        table.upsert(make_item("a", "A"));
        table.upsert(make_item("b", "B"));
        table.save();

        let ids1 = ids_from_lines(&read_lines(&dir, "t"));

        let loaded = Table::<TestItem>::load(dir.path(), "t", 2, 1000);
        loaded.save();

        let ids2 = ids_from_lines(&read_lines(&dir, "t"));
        assert_eq!(ids1, ids2);
    }

    #[test]
    fn test_save_sort_order_preserved_after_upsert() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::<TestItem>::load(dir.path(), "t", 2, 1000);
        table.upsert(make_item("b", "B"));
        table.upsert(make_item("a", "A"));
        table.save();

        let mut table = Table::<TestItem>::load(dir.path(), "t", 2, 1000);
        table.upsert(make_item("c", "C"));
        table.save();

        let ids = ids_from_lines(&read_lines(&dir, "t"));
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
    }

    #[test]
    fn test_save_single_item_sorted() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::<TestItem>::load(dir.path(), "t", 2, 1000);
        table.upsert(make_item("only", "Only"));
        table.save();

        let ids = ids_from_lines(&read_lines(&dir, "t"));
        assert_eq!(ids.len(), 1);
    }

    #[test]
    fn test_save_empty_table() {
        let dir = TempDir::new().unwrap();
        let table = Table::<TestItem>::load(dir.path(), "t", 2, 1000);
        table.save();

        let lines = read_lines(&dir, "t");
        assert!(lines.is_empty());
    }

    #[test]
    fn test_items_land_in_correct_shard_files() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::<TestItem>::load(dir.path(), "t", 2, 1000);
        // hash_id produces 14-char hex strings; we use pre-hashed ids for predictability
        table.items.insert(
            "aabb11".to_string(),
            make_item_with_id("aabb11", "Item AA"),
        );
        table.items.insert(
            "aabb22".to_string(),
            make_item_with_id("aabb22", "Item AA2"),
        );
        table.items.insert(
            "ccdd33".to_string(),
            make_item_with_id("ccdd33", "Item CC"),
        );
        table.save();

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

        let table = Table::<TestItem>::load(dir.path(), "t", 2, 1000);
        assert_eq!(table.items().len(), 2);
        let titles: Vec<String> = table.items().iter().map(|i| i.title.clone()).collect();
        assert!(titles.contains(&"From AA".to_string()));
        assert!(titles.contains(&"From BB".to_string()));
    }

    #[test]
    fn test_roundtrip_with_sharding_preserves_all_items() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::<TestItem>::load(dir.path(), "t", 2, 1000);
        table.upsert(make_item("alpha", "Alpha"));
        table.upsert(make_item("beta", "Beta"));
        table.upsert(make_item("gamma", "Gamma"));
        table.save();

        let loaded = Table::<TestItem>::load(dir.path(), "t", 2, 1000);
        assert_eq!(loaded.items().len(), 3);
        let titles: Vec<String> = loaded.items().iter().map(|i| i.title.clone()).collect();
        assert!(titles.contains(&"Alpha".to_string()));
        assert!(titles.contains(&"Beta".to_string()));
        assert!(titles.contains(&"Gamma".to_string()));
    }

    #[test]
    fn test_shard_characters_zero_puts_everything_in_items_empty() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::<TestItem>::load(dir.path(), "t", 0, 1000);
        table.items.insert(
            "aabb11".to_string(),
            make_item_with_id("aabb11", "Item 1"),
        );
        table.items.insert(
            "ccdd22".to_string(),
            make_item_with_id("ccdd22", "Item 2"),
        );
        table.save();

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
        let mut table = Table::<TestItem>::load(dir.path(), "t", 2, 1000);
        table.items.clear();
        table.items.insert(
            "aabb11".to_string(),
            make_item_with_id("aabb11", "Item AA"),
        );
        table.save();

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
        let mut table = Table::<TestItem>::load(dir.path(), "t", 2, 1000);
        table.upsert(make_item("same", "First"));
        table.upsert(make_item("same", "Second"));
        let items = table.items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Second");
    }

    /// Helper to create a TestItem with a pre-set id (no hashing).
    fn make_item_with_id(id: &str, title: &str) -> TestItem {
        TestItem {
            id: id.to_string(),
            title: title.to_string(),
        }
    }
}
