use std::io::BufReader;
use std::io::Read;

use atom_syndication::Feed;
use rss::Channel;

#[derive(Debug, PartialEq)]
struct FeedItem {
    title: String,
    date: String,
}

fn parse_rss_from_reader<R: Read>(reader: R) -> Vec<FeedItem> {
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

fn parse_atom_from_reader<R: Read>(reader: R) -> Vec<FeedItem> {
    let feed =
        Feed::read_from(BufReader::new(reader)).expect("failed to parse Atom feed");

    feed.entries()
        .iter()
        .map(|entry| FeedItem {
            title: entry.title().as_str().to_string(),
            date: entry
                .published()
                .or(Some(entry.updated()))
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "unknown".to_string()),
        })
        .collect()
}

fn fetch_rss(url: &str) -> Vec<FeedItem> {
    let response = reqwest::blocking::get(url).expect("failed to fetch feed");
    parse_rss_from_reader(response)
}

fn fetch_atom(url: &str) -> Vec<FeedItem> {
    let response = reqwest::blocking::get(url).expect("failed to fetch feed");
    parse_atom_from_reader(response)
}

fn main() {
    let feeds: Vec<(&str, Vec<FeedItem>)> = vec![
        (
            "Drew DeVault's blog (RSS)",
            fetch_rss("https://drewdevault.com/blog/index.xml"),
        ),
        (
            "Michael Stapelberg's blog (Atom)",
            fetch_atom("https://michael.stapelberg.ch/feed.xml"),
        ),
    ];

    for (name, items) in &feeds {
        println!("=== {name} ===\n");
        for item in items {
            println!("{}  {}", item.date, item.title);
        }
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rss_multiple_items() {
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

        let items = parse_rss_from_reader(xml.as_bytes());

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "First Post");
        assert_eq!(items[0].date, "Mon, 01 Jan 2024 00:00:00 +0000");
        assert_eq!(items[1].title, "Second Post");
        assert_eq!(items[1].date, "Tue, 02 Jan 2024 00:00:00 +0000");
    }

    #[test]
    fn test_parse_rss_missing_title() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rss version="2.0">
          <channel>
            <title>Test</title>
            <item>
              <pubDate>Mon, 01 Jan 2024 00:00:00 +0000</pubDate>
            </item>
          </channel>
        </rss>"#;

        let items = parse_rss_from_reader(xml.as_bytes());

        assert_eq!(items[0].title, "untitled");
    }

    #[test]
    fn test_parse_rss_missing_date() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rss version="2.0">
          <channel>
            <title>Test</title>
            <item>
              <title>No Date Post</title>
            </item>
          </channel>
        </rss>"#;

        let items = parse_rss_from_reader(xml.as_bytes());

        assert_eq!(items[0].date, "unknown");
    }

    #[test]
    fn test_parse_rss_empty_feed() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rss version="2.0">
          <channel>
            <title>Empty Blog</title>
          </channel>
        </rss>"#;

        let items = parse_rss_from_reader(xml.as_bytes());

        assert!(items.is_empty());
    }

    #[test]
    fn test_parse_atom_multiple_entries() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <feed xmlns="http://www.w3.org/2005/Atom">
          <title>Test Blog</title>
          <id>urn:test</id>
          <updated>2024-01-02T00:00:00Z</updated>
          <entry>
            <title>First Post</title>
            <id>urn:post:1</id>
            <updated>2024-01-01T00:00:00Z</updated>
            <published>2024-01-01T00:00:00Z</published>
          </entry>
          <entry>
            <title>Second Post</title>
            <id>urn:post:2</id>
            <updated>2024-01-02T00:00:00Z</updated>
            <published>2024-01-02T00:00:00Z</published>
          </entry>
        </feed>"#;

        let items = parse_atom_from_reader(xml.as_bytes());

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "First Post");
        assert_eq!(items[0].date, "2024-01-01");
        assert_eq!(items[1].title, "Second Post");
        assert_eq!(items[1].date, "2024-01-02");
    }

    #[test]
    fn test_parse_atom_falls_back_to_updated() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <feed xmlns="http://www.w3.org/2005/Atom">
          <title>Test</title>
          <id>urn:test</id>
          <updated>2024-06-15T00:00:00Z</updated>
          <entry>
            <title>No Publish Date</title>
            <id>urn:post:1</id>
            <updated>2024-06-15T00:00:00Z</updated>
          </entry>
        </feed>"#;

        let items = parse_atom_from_reader(xml.as_bytes());

        assert_eq!(items[0].date, "2024-06-15");
    }

    #[test]
    fn test_parse_atom_empty_feed() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <feed xmlns="http://www.w3.org/2005/Atom">
          <title>Empty</title>
          <id>urn:test</id>
          <updated>2024-01-01T00:00:00Z</updated>
        </feed>"#;

        let items = parse_atom_from_reader(xml.as_bytes());

        assert!(items.is_empty());
    }
}
