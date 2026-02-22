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
    pub id: String,
    pub source_id: String,
    pub title: String,
    pub date: Option<DateTime<Utc>>,
    pub author: String,
}

impl crate::table::TableRow for FeedItem {
    fn id(&self) -> &str {
        &self.id
    }
    fn set_id(&mut self, id: String) {
        self.id = id;
    }
}

pub fn fetch(url: &str) -> (FeedMeta, Vec<FeedItem>) {
    let response = reqwest::blocking::get(url).expect("failed to fetch feed");
    let bytes = response.bytes().expect("failed to read response body");
    let text = String::from_utf8_lossy(&bytes);

    if text.contains("<rss") {
        rss::parse(&bytes[..], url)
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
            id: "https://example.com/post/1".to_string(),
            source_id: "https://example.com/feed.xml".to_string(),
            title: "Test Post".to_string(),
            date: Some(
                NaiveDate::from_ymd_opt(2024, 1, 15)
                    .unwrap()
                    .and_hms_opt(12, 0, 0)
                    .unwrap()
                    .and_utc(),
            ),
            author: "Alice".to_string(),
        };

        let json = serde_json::to_string(&item).unwrap();
        let deserialized: FeedItem = serde_json::from_str(&json).unwrap();
        assert_eq!(item, deserialized);
    }

    #[test]
    fn test_serde_roundtrip_without_date() {
        let item = FeedItem {
            id: "urn:post:2".to_string(),
            source_id: "urn:test".to_string(),
            title: "No Date Post".to_string(),
            date: None,
            author: "Bob".to_string(),
        };

        let json = serde_json::to_string(&item).unwrap();
        let deserialized: FeedItem = serde_json::from_str(&json).unwrap();
        assert_eq!(item, deserialized);
    }
}
