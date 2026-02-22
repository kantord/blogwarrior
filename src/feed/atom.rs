use std::io::{BufReader, Read};

use atom_syndication::Feed;

use super::{FeedItem, FeedMeta};

pub fn parse<R: Read>(reader: R) -> Result<(FeedMeta, Vec<FeedItem>), Box<dyn std::error::Error>> {
    let feed = Feed::read_from(BufReader::new(reader))?;

    let meta = FeedMeta {
        title: feed.title().as_str().to_string(),
        site_url: feed
            .links()
            .iter()
            .find(|l| l.rel() == "alternate")
            .or_else(|| feed.links().first())
            .map(|l| l.href().to_string())
            .unwrap_or_default(),
        description: feed
            .subtitle()
            .map(|s| s.as_str().to_string())
            .unwrap_or_default(),
    };

    let items = feed
        .entries()
        .iter()
        .map(|entry| FeedItem {

            raw_id: entry.id().to_string(),
            title: entry.title().as_str().to_string(),
            date: entry
                .published()
                .or(Some(entry.updated()))
                .map(|d| d.to_utc()),
            feed: String::new(),
            link: entry
                .links()
                .iter()
                .find(|l| l.rel() == "alternate")
                .or_else(|| entry.links().first())
                .map(|l| l.href().to_string())
                .unwrap_or_default(),

        })
        .collect();

    Ok((meta, items))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multiple_entries() {
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

        let (_, items) = parse(xml.as_bytes()).unwrap();

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "First Post");
        assert_eq!(items[0].raw_id, "urn:post:1");
        assert_eq!(
            items[0].date.unwrap().format("%Y-%m-%d").to_string(),
            "2024-01-01"
        );
        assert_eq!(items[1].title, "Second Post");
        assert_eq!(items[1].raw_id, "urn:post:2");
        assert_eq!(
            items[1].date.unwrap().format("%Y-%m-%d").to_string(),
            "2024-01-02"
        );
    }

    #[test]
    fn test_timezone_is_normalized_to_utc() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <feed xmlns="http://www.w3.org/2005/Atom">
          <title>Test</title>
          <id>urn:test</id>
          <updated>2024-01-02T04:00:00Z</updated>
          <entry>
            <title>Late Night Post</title>
            <id>urn:post:1</id>
            <updated>2024-01-01T23:00:00-05:00</updated>
            <published>2024-01-01T23:00:00-05:00</published>
          </entry>
        </feed>"#;

        let (_, items) = parse(xml.as_bytes()).unwrap();
        let date = items[0].date.unwrap();

        assert_eq!(date.format("%Y-%m-%d").to_string(), "2024-01-02");
        assert_eq!(date.format("%H:%M").to_string(), "04:00");
    }

    #[test]
    fn test_falls_back_to_updated() {
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

        let (_, items) = parse(xml.as_bytes()).unwrap();

        assert_eq!(
            items[0].date.unwrap().format("%Y-%m-%d").to_string(),
            "2024-06-15"
        );
    }

    #[test]
    fn test_empty_feed() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <feed xmlns="http://www.w3.org/2005/Atom">
          <title>Empty</title>
          <id>urn:test</id>
          <updated>2024-01-01T00:00:00Z</updated>
        </feed>"#;

        let (_, items) = parse(xml.as_bytes()).unwrap();

        assert!(items.is_empty());
    }
}
