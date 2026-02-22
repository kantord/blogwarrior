mod feed;
mod feed_source;
mod table;

use std::collections::HashMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use itertools::Itertools;

use feed::FeedItem;
use feed_source::FeedSource;
use table::TableRow;

/// A simple RSS/Atom feed reader
#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Fetch feeds and save items to posts.jsonl
    Pull,
    /// Display items from posts.jsonl
    Show {
        /// Grouping mode: d (date), f (feed), or combinations like df, fd
        #[arg(short, long, default_value = "")]
        group: String,
    },
    /// Manage feed subscriptions
    Feed {
        #[command(subcommand)]
        command: FeedCommand,
    },
}

#[derive(Subcommand)]
enum FeedCommand {
    /// Subscribe to a feed by URL
    Add {
        /// The feed URL to subscribe to
        url: String,
    },
    /// Unsubscribe from a feed by URL
    Remove {
        /// The feed URL to unsubscribe from
        url: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum GroupKey {
    Date,
    Feed,
}

impl GroupKey {
    fn extract(&self, item: &FeedItem) -> String {
        match self {
            GroupKey::Date => format_date(item),
            GroupKey::Feed => item.feed.clone(),
        }
    }

    fn compare(&self, a: &FeedItem, b: &FeedItem) -> std::cmp::Ordering {
        match self {
            GroupKey::Date => format_date(b).cmp(&format_date(a)),
            GroupKey::Feed => a.feed.cmp(&b.feed),
        }
    }
}

fn format_date(item: &FeedItem) -> String {
    item.date
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn format_item(item: &FeedItem, grouped_keys: &[GroupKey]) -> String {
    let show_date = !grouped_keys.contains(&GroupKey::Date);
    let show_feed = !grouped_keys.contains(&GroupKey::Feed);
    match (show_date, show_feed) {
        (true, true) => format!("{}  {} ({})", format_date(item), item.title, item.feed),
        (true, false) => format!("{}  {}", format_date(item), item.title),
        (false, true) => format!("{} ({})", item.title, item.feed),
        (false, false) => item.title.clone(),
    }
}

fn render_grouped(items: &[&FeedItem], keys: &[GroupKey]) -> String {
    fn recurse(out: &mut String, items: &[&FeedItem], remaining: &[GroupKey], all_keys: &[GroupKey]) {
        let depth = all_keys.len() - remaining.len();
        let indent = "  ".repeat(depth);

        if remaining.is_empty() {
            for item in items {
                writeln!(out, "{indent}{}", format_item(item, all_keys)).unwrap();
            }
            return;
        }

        let key = remaining[0];
        let rest = &remaining[1..];

        let mut sorted = items.to_vec();
        sorted.sort_by(|a, b| key.compare(a, b));

        let (prefix, suffix) = if depth == 0 {
            ("=== ", " ===")
        } else {
            ("--- ", " ---")
        };

        for (group_val, group) in &sorted.iter().chunk_by(|item| key.extract(item)) {
            let group_items: Vec<&FeedItem> = group.copied().collect();
            writeln!(out, "{indent}{prefix}{group_val}{suffix}").unwrap();
            if depth == 0 {
                writeln!(out).unwrap();
            }
            recurse(out, &group_items, rest, all_keys);
            if depth == 0 {
                writeln!(out).unwrap();
                writeln!(out).unwrap();
            } else {
                writeln!(out).unwrap();
            }
        }
    }

    let mut out = String::new();
    recurse(&mut out, items, keys, keys);
    out
}

fn parse_grouping(arg: &str) -> Option<Vec<GroupKey>> {
    arg.chars()
        .map(|c| match c {
            'd' => Some(GroupKey::Date),
            'f' => Some(GroupKey::Feed),
            _ => None,
        })
        .collect()
}

fn store_dir() -> PathBuf {
    std::env::var("RSS_STORE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn cmd_remove(store: &Path, url: &str) {
    let mut feeds_table = table::Table::<FeedSource>::load(store, "feeds", 0, 50_000);
    let mut posts_table = table::Table::<FeedItem>::load(store, "posts", 1, 100_000_000);

    feeds_table.on_delete(url, |feed_id| {
        let post_keys: Vec<String> = posts_table
            .items()
            .iter()
            .filter(|p| p.feed == feed_id)
            .map(|p| p.key())
            .collect();
        for key in post_keys {
            posts_table.delete(&key);
        }
    });

    feeds_table.save();
    posts_table.save();
}

fn cmd_add(store: &Path, url: &str) {
    let mut table = table::Table::<FeedSource>::load(store, "feeds", 0, 50_000);
    table.upsert(FeedSource {
        url: url.to_string(),
        title: String::new(),
        site_url: String::new(),
        description: String::new(),
    });
    table.save();
}

fn cmd_pull(store: &Path) {
    let mut feeds_table = table::Table::<FeedSource>::load(store, "feeds", 0, 50_000);
    let sources = feeds_table.items();
    let mut table = table::Table::<FeedItem>::load(store, "posts", 1, 100_000_000);
    for source in &sources {
        let (meta, items) = match feed::fetch(&source.url) {
            Ok(result) => result,
            Err(e) => {
                eprintln!("Error fetching {}: {}", source.url, e);
                continue;
            }
        };
        let feed_id = feeds_table.id_of(source);
        for mut item in items {
            item.feed = feed_id.clone();
            table.upsert(item);
        }
        let mut updated = source.clone();
        updated.title = meta.title;
        updated.site_url = meta.site_url;
        updated.description = meta.description;
        feeds_table.upsert(updated);
    }
    table.save();
    feeds_table.save();
}

fn cmd_show(store: &Path, group: &str) {
    let keys = match parse_grouping(group) {
        Some(keys) => keys,
        None => {
            eprintln!("Unknown grouping: {}. Use: d, f, df, fd", group);
            return;
        }
    };

    let feeds_table = table::Table::<FeedSource>::load(store, "feeds", 0, 50_000);
    let feeds = feeds_table.items();
    let feed_titles: HashMap<String, String> = feeds
        .iter()
        .map(|f| (feeds_table.id_of(f), f.title.clone()))
        .collect();

    let table = table::Table::<FeedItem>::load(store, "posts", 1, 100_000_000);
    let mut items = table.items();

    for item in &mut items {
        if let Some(title) = feed_titles.get(&item.feed)
            && !title.is_empty()
        {
            item.feed = title.clone();
        }
    }

    items.sort_by(|a, b| b.date.cmp(&a.date));

    let refs: Vec<&FeedItem> = items.iter().collect();
    print!("{}", render_grouped(&refs, &keys));
}

fn main() {
    let args = Args::parse();
    let store = store_dir();

    match args.command {
        Some(Command::Pull) => cmd_pull(&store),
        Some(Command::Show { ref group }) => cmd_show(&store, group),
        Some(Command::Feed { command: FeedCommand::Add { ref url } }) => cmd_add(&store, url),
        Some(Command::Feed { command: FeedCommand::Remove { ref url } }) => cmd_remove(&store, url),
        None => cmd_show(&store, ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn item(title: &str, date: &str, feed: &str) -> FeedItem {
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

    #[test]
    fn test_parse_grouping_empty() {
        assert_eq!(parse_grouping(""), Some(vec![]));
    }

    #[test]
    fn test_parse_grouping_date() {
        assert_eq!(parse_grouping("d"), Some(vec![GroupKey::Date]));
    }

    #[test]
    fn test_parse_grouping_feed() {
        assert_eq!(parse_grouping("f"), Some(vec![GroupKey::Feed]));
    }

    #[test]
    fn test_parse_grouping_date_feed() {
        assert_eq!(
            parse_grouping("df"),
            Some(vec![GroupKey::Date, GroupKey::Feed])
        );
    }

    #[test]
    fn test_parse_grouping_feed_date() {
        assert_eq!(
            parse_grouping("fd"),
            Some(vec![GroupKey::Feed, GroupKey::Date])
        );
    }

    #[test]
    fn test_parse_grouping_invalid() {
        assert_eq!(parse_grouping("x"), None);
    }

    #[test]
    fn test_parse_grouping_partially_invalid() {
        assert_eq!(parse_grouping("dx"), None);
    }

    #[test]
    fn test_format_item_no_grouping() {
        let i = item("Post", "2024-01-15", "Alice");
        assert_eq!(format_item(&i, &[]), "2024-01-15  Post (Alice)");
    }

    #[test]
    fn test_format_item_grouped_by_date() {
        let i = item("Post", "2024-01-15", "Alice");
        assert_eq!(format_item(&i, &[GroupKey::Date]), "Post (Alice)");
    }

    #[test]
    fn test_format_item_grouped_by_feed() {
        let i = item("Post", "2024-01-15", "Alice");
        assert_eq!(format_item(&i, &[GroupKey::Feed]), "2024-01-15  Post");
    }

    #[test]
    fn test_format_item_grouped_by_both() {
        let i = item("Post", "2024-01-15", "Alice");
        assert_eq!(
            format_item(&i, &[GroupKey::Date, GroupKey::Feed]),
            "Post"
        );
    }

    #[test]
    fn test_format_date_with_date() {
        let i = item("Post", "2024-01-15", "Alice");
        assert_eq!(format_date(&i), "2024-01-15");
    }

    #[test]
    fn test_format_date_without_date() {
        let i = FeedItem {
            title: "Post".to_string(),
            date: None,
            feed: "Alice".to_string(),
            link: String::new(),
            raw_id: String::new(),

        };
        assert_eq!(format_date(&i), "unknown");
    }

    #[test]
    fn test_render_flat() {
        let items = [
            item("Post A", "2024-01-02", "Alice"),
            item("Post B", "2024-01-01", "Bob"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let output = render_grouped(&refs, &[]);
        assert_eq!(
            output,
            "2024-01-02  Post A (Alice)\n2024-01-01  Post B (Bob)\n"
        );
    }

    #[test]
    fn test_render_grouped_by_date() {
        let items = [
            item("Post A", "2024-01-02", "Alice"),
            item("Post B", "2024-01-02", "Bob"),
            item("Post C", "2024-01-01", "Alice"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let output = render_grouped(&refs, &[GroupKey::Date]);
        assert_eq!(
            output,
            "\
=== 2024-01-02 ===

  Post A (Alice)
  Post B (Bob)


=== 2024-01-01 ===

  Post C (Alice)


"
        );
    }

    #[test]
    fn test_render_grouped_by_feed() {
        let items = [
            item("Post A", "2024-01-02", "Bob"),
            item("Post B", "2024-01-01", "Alice"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let output = render_grouped(&refs, &[GroupKey::Feed]);
        assert_eq!(
            output,
            "\
=== Alice ===

  2024-01-01  Post B


=== Bob ===

  2024-01-02  Post A


"
        );
    }

    #[test]
    fn test_render_grouped_by_date_then_feed() {
        let items = [
            item("Post A", "2024-01-02", "Bob"),
            item("Post B", "2024-01-02", "Alice"),
            item("Post C", "2024-01-01", "Alice"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let output = render_grouped(&refs, &[GroupKey::Date, GroupKey::Feed]);
        assert_eq!(
            output,
            "\
=== 2024-01-02 ===

  --- Alice ---
    Post B

  --- Bob ---
    Post A



=== 2024-01-01 ===

  --- Alice ---
    Post C



"
        );
    }

    #[test]
    fn test_render_grouped_by_feed_then_date() {
        let items = [
            item("Post A", "2024-01-02", "Bob"),
            item("Post B", "2024-01-02", "Alice"),
            item("Post C", "2024-01-01", "Alice"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let output = render_grouped(&refs, &[GroupKey::Feed, GroupKey::Date]);
        assert_eq!(
            output,
            "\
=== Alice ===

  --- 2024-01-02 ---
    Post B

  --- 2024-01-01 ---
    Post C



=== Bob ===

  --- 2024-01-02 ---
    Post A



"
        );
    }

    #[test]
    fn test_render_empty_items() {
        let refs: Vec<&FeedItem> = vec![];
        assert_eq!(render_grouped(&refs, &[GroupKey::Date]), "");
    }

    #[test]
    fn test_date_ordering_is_descending() {
        let items = [
            item("Old", "2024-01-01", "Alice"),
            item("New", "2024-01-03", "Alice"),
            item("Mid", "2024-01-02", "Alice"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let output = render_grouped(&refs, &[GroupKey::Date]);
        let headers: Vec<&str> = output.lines().filter(|l| l.starts_with("===")).collect();
        assert_eq!(
            headers,
            vec!["=== 2024-01-03 ===", "=== 2024-01-02 ===", "=== 2024-01-01 ==="]
        );
    }

    #[test]
    fn test_feed_ordering_is_ascending() {
        let items = [
            item("Post", "2024-01-01", "Charlie"),
            item("Post", "2024-01-02", "Alice"),
            item("Post", "2024-01-03", "Bob"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let output = render_grouped(&refs, &[GroupKey::Feed]);
        let headers: Vec<&str> = output.lines().filter(|l| l.starts_with("===")).collect();
        assert_eq!(
            headers,
            vec!["=== Alice ===", "=== Bob ===", "=== Charlie ==="]
        );
    }
}
