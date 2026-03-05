use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::synctato::TableRow;
use crate::tables::FeedItem;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadMark {
    pub post_id: String,
    pub read_at: DateTime<Utc>,
}

impl TableRow for ReadMark {
    fn key(&self) -> String {
        self.post_id.clone()
    }

    const TABLE_NAME: &'static str = "reads";
    const SHARD_CHARACTERS: usize = 2;
    const EXPECTED_CAPACITY: usize = FeedItem::EXPECTED_CAPACITY;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synctato::Table;
    use tempfile::TempDir;

    #[test]
    fn test_roundtrip() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::<ReadMark>::load(dir.path()).unwrap();

        let mark = ReadMark {
            post_id: "some-post-id".to_string(),
            read_at: Utc::now(),
        };
        table.upsert(mark.clone());
        table.save().unwrap();

        let loaded = Table::<ReadMark>::load(dir.path()).unwrap();
        let items = loaded.items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].post_id, "some-post-id");
    }

    #[test]
    fn test_multiple_reads() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::<ReadMark>::load(dir.path()).unwrap();

        table.upsert(ReadMark {
            post_id: "post-1".to_string(),
            read_at: Utc::now(),
        });
        table.upsert(ReadMark {
            post_id: "post-2".to_string(),
            read_at: Utc::now(),
        });
        table.save().unwrap();

        let loaded = Table::<ReadMark>::load(dir.path()).unwrap();
        assert_eq!(loaded.items().len(), 2);
    }

    #[test]
    fn test_re_reading_overwrites() {
        let dir = TempDir::new().unwrap();
        let mut table = Table::<ReadMark>::load(dir.path()).unwrap();

        table.upsert(ReadMark {
            post_id: "post-1".to_string(),
            read_at: Utc::now(),
        });
        table.upsert(ReadMark {
            post_id: "post-1".to_string(),
            read_at: Utc::now(),
        });

        assert_eq!(table.items().len(), 1);
    }
}
