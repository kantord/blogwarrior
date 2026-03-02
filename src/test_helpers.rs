use chrono::{DateTime, NaiveDate, Utc};

use crate::feed::FeedItem;

pub fn utc_date(year: i32, month: u32, day: u32) -> DateTime<Utc> {
    NaiveDate::from_ymd_opt(year, month, day)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
}

pub fn utc_rfc3339(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().to_utc()
}

pub fn feed_item(title: &str, date: &str, feed: &str) -> FeedItem {
    FeedItem {
        title: title.to_string(),
        date: Some(
            NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_utc(),
        ),
        feed: feed.to_string(),
        link: String::new(),
        raw_id: String::new(),
    }
}

pub fn feed_item_with_raw_id(title: &str, date: &str, feed: &str, raw_id: &str) -> FeedItem {
    FeedItem {
        raw_id: raw_id.to_string(),
        ..feed_item(title, date, feed)
    }
}
