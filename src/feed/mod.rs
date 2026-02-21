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
