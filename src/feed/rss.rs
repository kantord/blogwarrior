use std::io::{BufReader, Read};

use rss::Channel;

use super::FeedItem;

pub fn parse<R: Read>(reader: R) -> Vec<FeedItem> {
    let channel = Channel::read_from(BufReader::new(reader)).expect("failed to parse RSS feed");

    channel
        .items()
        .iter()
        .map(|item| FeedItem {
            title: item.title().unwrap_or("untitled").to_string(),
            date: item.pub_date().unwrap_or("unknown").to_string(),
        })
        .collect()
}

pub fn fetch(url: &str) -> Vec<FeedItem> {
    let response = reqwest::blocking::get(url).expect("failed to fetch feed");
    parse(response)
}

#[cfg(test)]
mod tests {
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

        let items = parse(xml.as_bytes());

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "First Post");
        assert_eq!(items[0].date, "Mon, 01 Jan 2024 00:00:00 +0000");
        assert_eq!(items[1].title, "Second Post");
        assert_eq!(items[1].date, "Tue, 02 Jan 2024 00:00:00 +0000");
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

        let items = parse(xml.as_bytes());

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

        let items = parse(xml.as_bytes());

        assert_eq!(items[0].date, "unknown");
    }

    #[test]
    fn test_empty_feed() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rss version="2.0">
          <channel>
            <title>Empty Blog</title>
          </channel>
        </rss>"#;

        let items = parse(xml.as_bytes());

        assert!(items.is_empty());
    }
}
