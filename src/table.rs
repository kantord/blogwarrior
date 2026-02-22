use std::collections::HashMap;
use std::fs;
use std::io::BufRead;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::feed::FeedItem;

pub fn hash_id(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    format!("{:x}", hasher.finalize())[..14].to_string()
}

pub struct Table {
    items: HashMap<String, FeedItem>,
}

impl Table {
    pub fn new() -> Self {
        Self {
            items: HashMap::new(),
        }
    }

    pub fn load(path: &Path) -> Self {
        let mut table = Self::new();
        if let Ok(file) = fs::File::open(path) {
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

    pub fn save(&self, path: &Path) {
        let mut out = String::new();
        for item in self.items.values() {
            out.push_str(&serde_json::to_string(item).expect("failed to serialize item"));
            out.push('\n');
        }
        fs::write(path, out).expect("failed to write posts file");
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
        let mut table = Table::new();
        table.upsert(make_item("raw-id", "Post"));
        let items = table.items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, hash_id("raw-id"));
    }

    #[test]
    fn test_upsert_overwrites_existing() {
        let mut table = Table::new();
        table.upsert(make_item("same-id", "Original"));
        table.upsert(make_item("same-id", "Updated"));
        let items = table.items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Updated");
    }

    #[test]
    fn test_load_save_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("posts.jsonl");

        let mut table = Table::new();
        table.upsert(make_item("id-1", "First"));
        table.upsert(make_item("id-2", "Second"));
        table.save(&path);

        let loaded = Table::load(&path);
        assert_eq!(loaded.items().len(), 2);

        let titles: Vec<String> = loaded.items().iter().map(|i| i.title.clone()).collect();
        assert!(titles.contains(&"First".to_string()));
        assert!(titles.contains(&"Second".to_string()));
    }

    #[test]
    fn test_load_nonexistent_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.jsonl");
        let table = Table::load(&path);
        assert_eq!(table.items().len(), 0);
    }
}
