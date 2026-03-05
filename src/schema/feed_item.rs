use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeedItem {
    pub title: String,
    pub date: Option<DateTime<Utc>>,
    pub feed: String,
    #[serde(default)]
    pub link: String,
    #[serde(default)]
    pub raw_id: String,
}

impl crate::synctato::TableRow for FeedItem {
    fn key(&self) -> String {
        self.raw_id.clone()
    }

    const TABLE_NAME: &'static str = "posts";
    const SHARD_CHARACTERS: usize = 2;
    // Conservative upper bound; drives hash ID length (16 hex chars) via the
    // birthday-problem formula in synctato. Changing this value would alter ID
    // lengths and orphan rows in existing databases, so leave it stable.
    const EXPECTED_CAPACITY: usize = 100_000_000;
}
