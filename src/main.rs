mod feed;

use feed::FeedItem;

fn main() {
    let feeds: Vec<(&str, Vec<FeedItem>)> = vec![
        (
            "Drew DeVault's blog (RSS)",
            feed::rss::fetch("https://drewdevault.com/blog/index.xml"),
        ),
        (
            "Michael Stapelberg's blog (Atom)",
            feed::atom::fetch("https://michael.stapelberg.ch/feed.xml"),
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
