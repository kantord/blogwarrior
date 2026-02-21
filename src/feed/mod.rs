pub mod atom;
pub mod rss;

use chrono::{DateTime, Utc};

#[derive(Debug, PartialEq)]
pub struct FeedItem {
    pub id: String,
    pub source_id: String,
    pub title: String,
    pub date: Option<DateTime<Utc>>,
    pub author: String,
}

pub fn fetch(url: &str) -> Vec<FeedItem> {
    let response = reqwest::blocking::get(url).expect("failed to fetch feed");
    let bytes = response.bytes().expect("failed to read response body");
    let text = String::from_utf8_lossy(&bytes);

    if text.contains("<rss") {
        rss::parse(&bytes[..], url)
    } else {
        atom::parse(&bytes[..])
    }
}
