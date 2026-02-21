mod feed;

use std::env;

use itertools::Itertools;

use feed::FeedItem;

fn format_date(item: &FeedItem) -> String {
    item.date
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn print_flat(items: &[FeedItem]) {
    for item in items {
        println!("{}  {} ({})", format_date(item), item.title, item.author);
    }
}

fn print_grouped_by_date(items: &[FeedItem]) {
    for (date, group) in &items.iter().chunk_by(|item| format_date(item)) {
        println!("=== {date} ===\n");
        for item in group {
            println!("  {} ({})", item.title, item.author);
        }
        println!();
    }
}

fn print_grouped_by_author(items: &[FeedItem]) {
    let sorted: Vec<&FeedItem> = items.iter().sorted_by(|a, b| a.author.cmp(&b.author)).collect();
    for (author, group) in &sorted.iter().chunk_by(|item| &item.author) {
        println!("=== {author} ===\n");
        for item in group {
            println!("  {}  {}", format_date(item), item.title);
        }
        println!();
    }
}

fn print_grouped_by_date_then_author(items: &[FeedItem]) {
    for (date, date_group) in &items.iter().chunk_by(|item| format_date(item)) {
        println!("=== {date} ===\n");
        let date_items: Vec<&FeedItem> = date_group.collect();
        let sorted: Vec<&&FeedItem> =
            date_items.iter().sorted_by(|a, b| a.author.cmp(&b.author)).collect();
        for (author, author_group) in &sorted.iter().chunk_by(|item| &item.author) {
            println!("  --- {author} ---");
            for item in author_group {
                println!("    {}", item.title);
            }
        }
        println!();
    }
}

fn print_grouped_by_author_then_date(items: &[FeedItem]) {
    let sorted: Vec<&FeedItem> = items.iter().sorted_by(|a, b| a.author.cmp(&b.author)).collect();
    for (author, author_group) in &sorted.iter().chunk_by(|item| &item.author) {
        println!("=== {author} ===\n");
        let author_items: Vec<&&FeedItem> = author_group.collect();
        for (date, date_group) in &author_items.iter().chunk_by(|item| format_date(item)) {
            println!("  --- {date} ---");
            for item in date_group {
                println!("    {}", item.title);
            }
        }
        println!();
    }
}

fn main() {
    let grouping = env::args().nth(1).unwrap_or_default();

    let mut items: Vec<FeedItem> = vec![
        feed::rss::fetch("https://drewdevault.com/blog/index.xml"),
        feed::atom::fetch("https://michael.stapelberg.ch/feed.xml"),
    ]
    .into_iter()
    .flatten()
    .collect();

    items.sort_by(|a, b| b.date.cmp(&a.date));

    match grouping.as_str() {
        "" => print_flat(&items),
        "d" => print_grouped_by_date(&items),
        "a" => print_grouped_by_author(&items),
        "da" => print_grouped_by_date_then_author(&items),
        "ad" => print_grouped_by_author_then_date(&items),
        other => eprintln!("Unknown grouping: {other}. Use: d, a, da, ad"),
    }
}
