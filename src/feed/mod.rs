pub mod atom;
pub mod rss;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq)]
pub struct FeedMeta {
    pub title: String,
    pub site_url: String,
    pub description: String,
}

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

impl crate::table::TableRow for FeedItem {
    fn key(&self) -> String {
        self.raw_id.clone()
    }

    const TABLE_NAME: &'static str = "posts";
    const SHARD_CHARACTERS: usize = 1;
    const EXPECTED_CAPACITY: usize = 100_000_000;
}

pub fn fetch(
    client: &reqwest::blocking::Client,
    url: &str,
) -> anyhow::Result<(FeedMeta, Vec<FeedItem>)> {
    let response = client.get(url).send()?.error_for_status()?;
    let bytes = response.bytes()?;
    let text = String::from_utf8_lossy(&bytes);

    if text.contains("<rss") {
        rss::parse(&bytes[..])
    } else {
        atom::parse(&bytes[..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn test_serde_roundtrip_with_date() {
        let item = FeedItem {
            title: "Test Post".to_string(),
            date: Some(
                NaiveDate::from_ymd_opt(2024, 1, 15)
                    .unwrap()
                    .and_hms_opt(12, 0, 0)
                    .unwrap()
                    .and_utc(),
            ),
            feed: "abc123".to_string(),
            link: String::new(),
            raw_id: String::new(),
        };

        let json = serde_json::to_string(&item).unwrap();
        let deserialized: FeedItem = serde_json::from_str(&json).unwrap();
        assert_eq!(item, deserialized);
    }

    #[test]
    fn test_serde_roundtrip_without_date() {
        let item = FeedItem {
            title: "No Date Post".to_string(),
            date: None,
            feed: "def456".to_string(),
            link: String::new(),
            raw_id: String::new(),
        };

        let json = serde_json::to_string(&item).unwrap();
        let deserialized: FeedItem = serde_json::from_str(&json).unwrap();
        assert_eq!(item, deserialized);
    }
}
