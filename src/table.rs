use std::collections::HashMap;
use std::fs;
use std::io::BufRead;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::feed::FeedItem;

pub fn hash_id(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    format!("{:x}", hasher.finalize())[..14].to_string()
}

pub struct Table {
    items: HashMap<String, FeedItem>,
    path: PathBuf,
}

impl Table {
    fn new(path: PathBuf) -> Self {
        Self {
            items: HashMap::new(),
            path,
        }
    }

    pub fn load(store: &Path, name: &str) -> Self {
        let path = store.join(name).join("items.jsonl");
        let mut table = Self::new(path);
        if let Ok(file) = fs::File::open(&table.path) {
            for line in std::io::BufReader::new(file).lines() {
                let line = line.expect("failed to read line");
                if line.trim().is_empty() {
                    continue;
                }
                let item: FeedItem =
                    serde_json::from_str(&line).expect("failed to parse post entry");
                table.items.insert(item.id.clone(), item);
            }
        }
        table
    }

    pub fn upsert(&mut self, mut item: FeedItem) {
        item.id = hash_id(&item.id);
        self.items.insert(item.id.clone(), item);
    }

    pub fn save(&self) {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).expect("failed to create table directory");
        }
        let mut sorted: Vec<&FeedItem> = self.items.values().collect();
        sorted.sort_by(|a, b| a.id.cmp(&b.id));
        let mut out = String::new();
        for item in sorted {
            out.push_str(&serde_json::to_string(item).expect("failed to serialize item"));
            out.push('\n');
        }
        fs::write(&self.path, out).expect("failed to write posts file");
    }

    pub fn items(&self) -> Vec<FeedItem> {
        self.items.values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_item(id: &str, title: &str) -> FeedItem {
        FeedItem {
            id: id.to_string(),
            source_id: "test".to_string(),
            title: title.to_string(),
            date: None,
            author: "Author".to_string(),
        }
    }

    #[test]
    fn test_upsert_hashes_id() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::load(dir.path(), "test_table");
        table.upsert(make_item("raw-id", "Post"));
        let items = table.items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, hash_id("raw-id"));
    }

    #[test]
    fn test_upsert_overwrites_existing() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::load(dir.path(), "test_table");
        table.upsert(make_item("same-id", "Original"));
        table.upsert(make_item("same-id", "Updated"));
        let items = table.items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Updated");
    }

    #[test]
    fn test_load_save_roundtrip() {
        let dir = TempDir::new().unwrap();

        let mut table = Table::load(dir.path(), "test_table");
        table.upsert(make_item("id-1", "First"));
        table.upsert(make_item("id-2", "Second"));
        table.save();

        let loaded = Table::load(dir.path(), "test_table");
        assert_eq!(loaded.items().len(), 2);

        let titles: Vec<String> = loaded.items().iter().map(|i| i.title.clone()).collect();
        assert!(titles.contains(&"First".to_string()));
        assert!(titles.contains(&"Second".to_string()));
    }

    #[test]
    fn test_load_nonexistent_file() {
        let dir = TempDir::new().unwrap();
        let table = Table::load(dir.path(), "nonexistent");
        assert_eq!(table.items().len(), 0);
    }

    fn read_lines(dir: &TempDir, name: &str) -> Vec<String> {
        let path = dir.path().join(name).join("items.jsonl");
        fs::read_to_string(path)
            .unwrap()
            .lines()
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect()
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

    #[test]
    fn test_save_sorts_items_by_id() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::load(dir.path(), "t");
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
        let mut table = Table::load(dir.path(), "t");
        table.upsert(make_item("c", "C"));
        table.upsert(make_item("a", "A"));
        table.upsert(make_item("b", "B"));
        table.save();

        let ids1 = ids_from_lines(&read_lines(&dir, "t"));

        let loaded = Table::load(dir.path(), "t");
        loaded.save();

        let ids2 = ids_from_lines(&read_lines(&dir, "t"));
        assert_eq!(ids1, ids2);
    }

    #[test]
    fn test_save_sort_order_preserved_after_upsert() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::load(dir.path(), "t");
        table.upsert(make_item("b", "B"));
        table.upsert(make_item("a", "A"));
        table.save();

        let mut table = Table::load(dir.path(), "t");
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
        let mut table = Table::load(dir.path(), "t");
        table.upsert(make_item("only", "Only"));
        table.save();

        let ids = ids_from_lines(&read_lines(&dir, "t"));
        assert_eq!(ids.len(), 1);
    }

    #[test]
    fn test_save_empty_table() {
        let dir = TempDir::new().unwrap();
        let table = Table::load(dir.path(), "t");
        table.save();

        let lines = read_lines(&dir, "t");
        assert!(lines.is_empty());
    }
}
