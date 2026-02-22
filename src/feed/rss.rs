use std::io::{BufReader, Read};

use chrono::{DateTime, FixedOffset};
use rss::Channel;
use url::Url;

use super::FeedItem;

fn normalize_url(raw: &str) -> String {
    match Url::parse(raw) {
        Ok(url) => url.to_string(),
        Err(_) => raw.to_string(),
    }
}

pub fn parse<R: Read>(reader: R, source_id: &str) -> Vec<FeedItem> {
    let channel = Channel::read_from(BufReader::new(reader)).expect("failed to parse RSS feed");
    let author = channel.title().to_string();
    let source_id = source_id.to_string();

    channel
        .items()
        .iter()
        .map(|item| FeedItem {
            id: super::hash_id(
                &item
                    .guid()
                    .map(|g| g.value().to_string())
                    .or_else(|| item.link().map(|l| normalize_url(l)))
                    .unwrap_or_default(),
            ),
            source_id: source_id.clone(),
            title: item.title().unwrap_or("untitled").to_string(),
            date: item
                .pub_date()
                .and_then(|d| DateTime::<FixedOffset>::parse_from_rfc2822(d).ok())
                .map(|d| d.to_utc()),
            author: author.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::hash_id;
    use super::*;

    #[test]
    fn test_multiple_items() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rss version="2.0">
          <channel>
            <title>Test Blog</title>
            <item>
              <title>First Post</title>
              <pubDate>Mon, 01 Jan 2024 00:00:00 +0000</pubDate>
            </item>
            <item>
              <title>Second Post</title>
              <pubDate>Tue, 02 Jan 2024 00:00:00 +0000</pubDate>
            </item>
          </channel>
        </rss>"#;

        let items = parse(xml.as_bytes(), "https://example.com/feed.xml");

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "First Post");
        assert_eq!(items[0].id, hash_id(""));
        assert_eq!(
            items[0].date.unwrap().format("%Y-%m-%d").to_string(),
            "2024-01-01"
        );
        assert_eq!(items[0].author, "Test Blog");
        assert_eq!(items[0].source_id, "https://example.com/feed.xml");
        assert_eq!(items[1].title, "Second Post");
        assert_eq!(
            items[1].date.unwrap().format("%Y-%m-%d").to_string(),
            "2024-01-02"
        );
        assert_eq!(items[1].author, "Test Blog");
    }

    #[test]
    fn test_timezone_is_normalized_to_utc() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rss version="2.0">
          <channel>
            <title>Test</title>
            <item>
              <title>Late Night Post</title>
              <pubDate>Mon, 01 Jan 2024 23:00:00 -0500</pubDate>
            </item>
          </channel>
        </rss>"#;

        let items = parse(xml.as_bytes(), "https://example.com/feed.xml");
        let date = items[0].date.unwrap();

        assert_eq!(date.format("%Y-%m-%d").to_string(), "2024-01-02");
        assert_eq!(date.format("%H:%M").to_string(), "04:00");
    }

    #[test]
    fn test_missing_title() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rss version="2.0">
          <channel>
            <title>Test</title>
            <item>
              <pubDate>Mon, 01 Jan 2024 00:00:00 +0000</pubDate>
            </item>
          </channel>
        </rss>"#;

        let items = parse(xml.as_bytes(), "https://example.com/feed.xml");

        assert_eq!(items[0].title, "untitled");
    }

    #[test]
    fn test_missing_date() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rss version="2.0">
          <channel>
            <title>Test</title>
            <item>
              <title>No Date Post</title>
            </item>
          </channel>
        </rss>"#;

        let items = parse(xml.as_bytes(), "https://example.com/feed.xml");

        assert_eq!(items[0].date, None);
    }

    #[test]
    fn test_empty_feed() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rss version="2.0">
          <channel>
            <title>Empty Blog</title>
          </channel>
        </rss>"#;

        let items = parse(xml.as_bytes(), "https://example.com/feed.xml");

        assert!(items.is_empty());
    }

    #[test]
    fn test_id_from_guid() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rss version="2.0">
          <channel>
            <title>Test</title>
            <item>
              <title>Post</title>
              <guid>https://example.com/post/1</guid>
            </item>
          </channel>
        </rss>"#;

        let items = parse(xml.as_bytes(), "https://example.com/feed.xml");

        assert_eq!(items[0].id, hash_id("https://example.com/post/1"));
    }

    #[test]
    fn test_id_falls_back_to_link() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rss version="2.0">
          <channel>
            <title>Test</title>
            <item>
              <title>Post</title>
              <link>https://example.com/post/1</link>
            </item>
          </channel>
        </rss>"#;

        let items = parse(xml.as_bytes(), "https://example.com/feed.xml");

        assert_eq!(items[0].id, hash_id("https://example.com/post/1"));
    }

    #[test]
    fn test_id_link_is_normalized() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rss version="2.0">
          <channel>
            <title>Test</title>
            <item>
              <title>Post</title>
              <link>HTTPS://EXAMPLE.COM/post/1</link>
            </item>
          </channel>
        </rss>"#;

        let items = parse(xml.as_bytes(), "https://example.com/feed.xml");

        assert_eq!(items[0].id, hash_id("https://example.com/post/1"));
    }

    #[test]
    fn test_id_empty_when_no_guid_or_link() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rss version="2.0">
          <channel>
            <title>Test</title>
            <item>
              <title>Post</title>
            </item>
          </channel>
        </rss>"#;

        let items = parse(xml.as_bytes(), "https://example.com/feed.xml");

        assert_eq!(items[0].id, hash_id(""));
    }

    #[test]
    fn test_id_prefers_guid_over_link() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rss version="2.0">
          <channel>
            <title>Test</title>
            <item>
              <title>Post</title>
              <guid>urn:uuid:123</guid>
              <link>https://example.com/post/1</link>
            </item>
          </channel>
        </rss>"#;

        let items = parse(xml.as_bytes(), "https://example.com/feed.xml");

        assert_eq!(items[0].id, hash_id("urn:uuid:123"));
    }
}
