use std::io::BufReader;

use atom_syndication::Feed;
use rss::Channel;

fn main() {
    println!("=== Drew DeVault's blog (RSS) ===\n");

    let response = reqwest::blocking::get("https://drewdevault.com/blog/index.xml")
        .expect("failed to fetch Drew DeVault's feed");
    let reader = BufReader::new(response);
    let channel = Channel::read_from(reader).expect("failed to parse RSS feed");

    for item in channel.items() {
        let title = item.title().unwrap_or("untitled");
        let date = item.pub_date().unwrap_or("unknown");
        println!("{date}  {title}");
    }

    println!("\n=== Michael Stapelberg's blog (Atom) ===\n");

    let response = reqwest::blocking::get("https://michael.stapelberg.ch/feed.xml")
        .expect("failed to fetch Stapelberg's feed");
    let reader = BufReader::new(response);
    let feed = Feed::read_from(reader).expect("failed to parse Atom feed");

    for entry in feed.entries() {
        let title = entry.title().as_str();
        let date = entry
            .published()
            .or(Some(entry.updated()))
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "unknown".to_string());

        println!("{date}  {title}");
    }
}
