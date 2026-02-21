use std::io::BufReader;

use atom_syndication::Feed;
use rss::Channel;

struct FeedItem {
    title: String,
    date: String,
}

fn parse_rss(url: &str) -> Vec<FeedItem> {
    let response = reqwest::blocking::get(url).expect("failed to fetch feed");
    let reader = BufReader::new(response);
    let channel = Channel::read_from(reader).expect("failed to parse RSS feed");

    channel
        .items()
        .iter()
        .map(|item| FeedItem {
            title: item.title().unwrap_or("untitled").to_string(),
            date: item.pub_date().unwrap_or("unknown").to_string(),
        })
        .collect()
}

fn parse_atom(url: &str) -> Vec<FeedItem> {
    let response = reqwest::blocking::get(url).expect("failed to fetch feed");
    let reader = BufReader::new(response);
    let feed = Feed::read_from(reader).expect("failed to parse Atom feed");

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

fn main() {
    let feeds: Vec<(&str, Vec<FeedItem>)> = vec![
        (
            "Drew DeVault's blog (RSS)",
            parse_rss("https://drewdevault.com/blog/index.xml"),
        ),
        (
            "Michael Stapelberg's blog (Atom)",
            parse_atom("https://michael.stapelberg.ch/feed.xml"),
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
