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
    /// Unsubscribe from a feed by URL or @shorthand
    Rm {
        /// The feed URL or @shorthand to unsubscribe from
        url: String,
    },
    /// List subscribed feeds
    Ls,
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

const HOME_ROW: [char; 9] = ['a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l'];

fn hex_to_base9(hex: &str) -> String {
    if hex.is_empty() {
        return String::from("a");
    }
    // Parse hex string into a vector of digit values
    let mut digits: Vec<u8> = hex
        .chars()
        .map(|c| c.to_digit(16).unwrap_or(0) as u8)
        .collect();

    let mut remainders = Vec::new();

    // Long division: divide the base-16 number by 9 repeatedly
    loop {
        let mut remainder: u16 = 0;
        let mut quotient = Vec::new();
        for &d in &digits {
            let current = remainder * 16 + d as u16;
            quotient.push((current / 9) as u8);
            remainder = current % 9;
        }
        remainders.push(remainder as u8);
        // Strip leading zeros from quotient
        digits = quotient.into_iter().skip_while(|&d| d == 0).collect();
        if digits.is_empty() {
            break;
        }
    }

    // Remainders are in reverse order
    remainders
        .into_iter()
        .rev()
        .map(|d| HOME_ROW[d as usize])
        .collect()
}

fn compute_shorthands(ids: &[String]) -> Vec<String> {
    if ids.is_empty() {
        return Vec::new();
    }

    let base9s: Vec<String> = ids.iter().map(|id| hex_to_base9(id)).collect();

    if base9s.len() == 1 {
        return vec![base9s[0].chars().next().unwrap().to_string()];
    }

    // Find the shortest prefix length where all are unique
    let max_len = base9s.iter().map(|s| s.len()).max().unwrap_or(1);
    for len in 1..=max_len {
        let prefixes: Vec<String> = base9s
            .iter()
            .map(|s| s.chars().take(len).collect::<String>())
            .collect();
        let unique: std::collections::HashSet<&String> = prefixes.iter().collect();
        if unique.len() == prefixes.len() {
            return prefixes;
        }
    }

    // Fallback: return full strings
    base9s
}

fn resolve_shorthand(feeds_table: &table::Table<FeedSource>, shorthand: &str) -> Option<String> {
    let mut feeds = feeds_table.items();
    feeds.sort_by(|a, b| a.url.cmp(&b.url));
    let ids: Vec<String> = feeds.iter().map(|f| feeds_table.id_of(f)).collect();
    let shorthands = compute_shorthands(&ids);
    for (feed, sh) in feeds.iter().zip(shorthands.iter()) {
        if sh == shorthand {
            return Some(feed.url.clone());
        }
    }
    None
}

fn store_dir() -> PathBuf {
    std::env::var("RSS_STORE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::data_dir()
                .expect("could not determine data directory")
                .join("blogtato")
        })
}

fn cmd_remove(store: &Path, url: &str) {
    let mut feeds_table = table::Table::<FeedSource>::load(store, "feeds", 0, 50_000);
    let mut posts_table = table::Table::<FeedItem>::load(store, "posts", 1, 100_000_000);

    let resolved_url;
    let url = if let Some(shorthand) = url.strip_prefix('@') {
        match resolve_shorthand(&feeds_table, shorthand) {
            Some(u) => {
                resolved_url = u;
                &resolved_url
            }
            None => {
                eprintln!("Unknown shorthand: @{}", shorthand);
                return;
            }
        }
    } else {
        url
    };

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

fn cmd_feed_ls(store: &Path) {
    let feeds_table = table::Table::<FeedSource>::load(store, "feeds", 0, 50_000);
    let mut feeds = feeds_table.items();
    if feeds.is_empty() {
        eprintln!("No matching feeds");
        std::process::exit(1);
    }
    feeds.sort_by(|a, b| a.url.cmp(&b.url));
    let ids: Vec<String> = feeds.iter().map(|f| feeds_table.id_of(f)).collect();
    let shorthands = compute_shorthands(&ids);
    for (feed, shorthand) in feeds.iter().zip(shorthands.iter()) {
        if feed.title.is_empty() {
            println!("@{} {}", shorthand, feed.url);
        } else {
            println!("@{} {} ({})", shorthand, feed.url, feed.title);
        }
    }
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

    if items.is_empty() {
        eprintln!("No matching posts");
        std::process::exit(1);
    }

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
        Some(Command::Feed { command: FeedCommand::Rm { ref url } }) => cmd_remove(&store, url),
        Some(Command::Feed { command: FeedCommand::Ls }) => cmd_feed_ls(&store),
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

    #[test]
    fn test_hex_to_base9() {
        // "0" in hex = 0 in decimal = 0 in base9 = "a"
        assert_eq!(hex_to_base9("0"), "a");
        // "9" in hex = 9 in decimal = 10 in base9 = "sa"
        assert_eq!(hex_to_base9("9"), "sa");
        // "ff" in hex = 255 in decimal = 313 in base9 = "fsf"
        assert_eq!(hex_to_base9("ff"), "fsf");
        // "1" in hex = 1 in decimal = 1 in base9 = "s"
        assert_eq!(hex_to_base9("1"), "s");
        // "a" in hex = 10 in decimal = 11 in base9 = "ss"
        assert_eq!(hex_to_base9("a"), "ss");
    }

    #[test]
    fn test_compute_shorthands_unique_prefixes() {
        // Two IDs that differ at the first base9 digit should get 1-char shorthands
        let ids = vec!["00".to_string(), "ff".to_string()];
        let shorthands = compute_shorthands(&ids);
        assert_eq!(shorthands.len(), 2);
        assert!(shorthands.iter().all(|s| s.len() == 1));
        assert_ne!(shorthands[0], shorthands[1]);

        // Two IDs that share a base9 prefix should get longer shorthands
        let ids2 = vec!["aa".to_string(), "ab".to_string()];
        let shorthands2 = compute_shorthands(&ids2);
        assert_eq!(shorthands2.len(), 2);
        assert_ne!(shorthands2[0], shorthands2[1]);
        // They should be longer than 1 since they share a prefix in base9
        assert!(shorthands2[0].len() > 1 || shorthands2[1].len() > 1 || shorthands2[0] != shorthands2[1]);
    }

    #[test]
    fn test_compute_shorthands_single() {
        let ids = vec!["abcdef".to_string()];
        let shorthands = compute_shorthands(&ids);
        assert_eq!(shorthands.len(), 1);
        assert_eq!(shorthands[0].len(), 1);
    }

    #[test]
    fn test_compute_shorthands_empty() {
        let ids: Vec<String> = vec![];
        let shorthands = compute_shorthands(&ids);
        assert!(shorthands.is_empty());
    }
}
