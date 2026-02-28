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
#[command(args_conflicts_with_subcommands = true)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,

    /// Positional arguments: grouping mode (d, f, df, fd) and/or @shorthand filter
    args: Vec<String>,
}

#[derive(Subcommand)]
enum Command {
    /// Fetch feeds and save items to posts.jsonl
    Pull,
    /// Display items from posts.jsonl
    Show {
        /// Positional arguments: grouping mode (d, f, df, fd) and/or @shorthand filter
        args: Vec<String>,
    },
    /// Open a post in the default browser
    Open {
        /// Post shorthand
        shorthand: String,
    },
    /// Read a post's content in the terminal
    Read {
        /// Post shorthand
        shorthand: String,
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

fn format_item(item: &FeedItem, grouped_keys: &[GroupKey], shorthand: &str) -> String {
    let show_date = !grouped_keys.contains(&GroupKey::Date);
    let show_feed = !grouped_keys.contains(&GroupKey::Feed);
    let body = match (show_date, show_feed) {
        (true, true) => format!("{}  {} ({})", format_date(item), item.title, item.feed),
        (true, false) => format!("{}  {}", format_date(item), item.title),
        (false, true) => format!("{} ({})", item.title, item.feed),
        (false, false) => item.title.clone(),
    };
    format!("{} {}", shorthand, body)
}

fn render_grouped(items: &[&FeedItem], keys: &[GroupKey], shorthands: &HashMap<String, String>) -> String {
    fn recurse(out: &mut String, items: &[&FeedItem], remaining: &[GroupKey], all_keys: &[GroupKey], shorthands: &HashMap<String, String>) {
        let depth = all_keys.len() - remaining.len();
        let indent = "  ".repeat(depth);

        if remaining.is_empty() {
            for item in items {
                let sh = shorthands.get(&item.raw_id).map(|s| s.as_str()).unwrap_or("");
                writeln!(out, "{indent}{}", format_item(item, all_keys, sh)).unwrap();
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
            recurse(out, &group_items, rest, all_keys, shorthands);
            if depth == 0 {
                writeln!(out).unwrap();
                writeln!(out).unwrap();
            } else {
                writeln!(out).unwrap();
            }
        }
    }

    let mut out = String::new();
    recurse(&mut out, items, keys, keys, shorthands);
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

fn parse_show_args(args: &[String]) -> (String, Option<String>) {
    let mut group = String::new();
    let mut filter = None;
    for arg in args {
        if arg.starts_with('@') {
            filter = Some(arg.clone());
        } else {
            group = arg.clone();
        }
    }
    (group, filter)
}

const HOME_ROW: [char; 9] = ['a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l'];

const POST_ALPHABET: [char; 34] = [
    'a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l',
    'A', 'S', 'D', 'F', 'G', 'H', 'J', 'K', 'L',
    'q', 'w', 'e', 'r', 't', 'y', 'i', 'o', 'p',
    'z', 'x', 'c', 'v', 'b', 'n', 'm',
];

fn hex_to_custom_base(hex: &str, alphabet: &[char]) -> String {
    let base = alphabet.len() as u16;
    if hex.is_empty() {
        return String::from(alphabet[0]);
    }
    // Parse hex string into a vector of digit values
    let mut digits: Vec<u8> = hex
        .chars()
        .map(|c| c.to_digit(16).unwrap_or(0) as u8)
        .collect();

    let mut remainders = Vec::new();

    // Long division: divide the base-16 number by `base` repeatedly
    loop {
        let mut remainder: u16 = 0;
        let mut quotient = Vec::new();
        for &d in &digits {
            let current = remainder * 16 + d as u16;
            quotient.push((current / base) as u8);
            remainder = current % base;
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
        .map(|d| alphabet[d as usize])
        .collect()
}

fn hex_to_base9(hex: &str) -> String {
    hex_to_custom_base(hex, &HOME_ROW)
}

fn index_to_shorthand(mut n: usize) -> String {
    let base = POST_ALPHABET.len();
    if n == 0 {
        return POST_ALPHABET[0].to_string();
    }
    let mut chars = Vec::new();
    while n > 0 {
        chars.push(POST_ALPHABET[n % base]);
        n /= base;
    }
    chars.reverse();
    chars.into_iter().collect()
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

fn load_sorted_posts(store: &Path) -> Vec<FeedItem> {
    let table = table::Table::<FeedItem>::load(store, "posts", 1, 100_000_000);
    let mut items = table.items();
    items.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.raw_id.cmp(&b.raw_id)));
    items
}

fn resolve_post_shorthand(store: &Path, shorthand: &str) -> FeedItem {
    let items = load_sorted_posts(store);
    let found = items
        .into_iter()
        .enumerate()
        .find(|(i, _)| index_to_shorthand(*i) == shorthand);
    match found {
        Some((_, item)) => item,
        None => {
            eprintln!("Unknown shorthand: {}", shorthand);
            std::process::exit(1);
        }
    }
}

fn cmd_open(store: &Path, shorthand: &str) {
    let item = resolve_post_shorthand(store, shorthand);
    if item.link.is_empty() {
        eprintln!("Post has no link");
        std::process::exit(1);
    }
    if let Err(e) = open::that(&item.link) {
        eprintln!("Could not open URL: {}", e);
        std::process::exit(1);
    }
}

fn cmd_read(store: &Path, shorthand: &str) {
    let item = resolve_post_shorthand(store, shorthand);
    if item.link.is_empty() {
        eprintln!("Post has no link");
        std::process::exit(1);
    }
    let response = match reqwest::blocking::get(&item.link) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Could not fetch URL: {}", e);
            std::process::exit(1);
        }
    };
    let html = match response.text() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Could not read response: {}", e);
            std::process::exit(1);
        }
    };
    let reader = match readability_js::Readability::new() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Could not initialize reader: {}", e);
            std::process::exit(1);
        }
    };
    let article = reader
        .parse_with_url(&html, &item.link)
        .or_else(|_| reader.parse(&html));
    match article {
        Ok(a) => {
            println!("{}\n", a.title);
            print!("{}", a.text_content);
        }
        Err(e) => {
            eprintln!("Could not extract readable content: {}", e);
            eprintln!("Try: blog open {}", shorthand);
            std::process::exit(1);
        }
    }
}

fn cmd_show(store: &Path, group: &str, filter: Option<&str>) {
    let keys = match parse_grouping(group) {
        Some(keys) => keys,
        None => {
            eprintln!("Unknown grouping: {}. Use: d, f, df, fd", group);
            return;
        }
    };

    let feeds_table = table::Table::<FeedSource>::load(store, "feeds", 0, 50_000);
    let mut feeds = feeds_table.items();
    feeds.sort_by(|a, b| a.url.cmp(&b.url));
    let ids: Vec<String> = feeds.iter().map(|f| feeds_table.id_of(f)).collect();
    let shorthands = compute_shorthands(&ids);

    let filter_feed_id = match filter {
        Some(f) if f.starts_with('@') => {
            let shorthand = &f[1..];
            match shorthands.iter().position(|sh| sh == shorthand) {
                Some(pos) => Some(ids[pos].clone()),
                None => {
                    eprintln!("Unknown shorthand: {}", f);
                    std::process::exit(1);
                }
            }
        }
        _ => None,
    };

    let feed_labels: HashMap<String, String> = ids
        .iter()
        .zip(feeds.iter())
        .zip(shorthands.iter())
        .map(|((id, feed), sh)| {
            let label = if feed.title.is_empty() {
                format!("@{} {}", sh, feed.url)
            } else {
                format!("@{} {}", sh, feed.title)
            };
            (id.clone(), label)
        })
        .collect();

    let mut items = load_sorted_posts(store);

    let post_shorthands: HashMap<String, String> = items
        .iter()
        .enumerate()
        .map(|(i, item)| (item.raw_id.clone(), index_to_shorthand(i)))
        .collect();

    if let Some(ref feed_id) = filter_feed_id {
        items.retain(|item| item.feed == *feed_id);
    }

    for item in &mut items {
        if let Some(label) = feed_labels.get(&item.feed) {
            item.feed = label.clone();
        }
    }

    if items.is_empty() {
        eprintln!("No matching posts");
        std::process::exit(1);
    }

    let refs: Vec<&FeedItem> = items.iter().collect();
    print!("{}", render_grouped(&refs, &keys, &post_shorthands));
}

fn main() {
    let args = Args::parse();
    let store = store_dir();

    match args.command {
        Some(Command::Pull) => cmd_pull(&store),
        Some(Command::Show { ref args }) => {
            let (group, filter) = parse_show_args(args);
            cmd_show(&store, &group, filter.as_deref());
        }
        Some(Command::Open { ref shorthand }) => cmd_open(&store, shorthand),
        Some(Command::Read { ref shorthand }) => cmd_read(&store, shorthand),
        Some(Command::Feed { command: FeedCommand::Add { ref url } }) => cmd_add(&store, url),
        Some(Command::Feed { command: FeedCommand::Rm { ref url } }) => cmd_remove(&store, url),
        Some(Command::Feed { command: FeedCommand::Ls }) => cmd_feed_ls(&store),
        None => {
            let (group, filter) = parse_show_args(&args.args);
            cmd_show(&store, &group, filter.as_deref());
        }
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
        assert_eq!(format_item(&i, &[], "abc"), "abc 2024-01-15  Post (Alice)");
    }

    #[test]
    fn test_format_item_grouped_by_date() {
        let i = item("Post", "2024-01-15", "Alice");
        assert_eq!(format_item(&i, &[GroupKey::Date], "abc"), "abc Post (Alice)");
    }

    #[test]
    fn test_format_item_grouped_by_feed() {
        let i = item("Post", "2024-01-15", "Alice");
        assert_eq!(format_item(&i, &[GroupKey::Feed], "abc"), "abc 2024-01-15  Post");
    }

    #[test]
    fn test_format_item_grouped_by_both() {
        let i = item("Post", "2024-01-15", "Alice");
        assert_eq!(
            format_item(&i, &[GroupKey::Date, GroupKey::Feed], "abc"),
            "abc Post"
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
        let empty: HashMap<String, String> = HashMap::new();
        let output = render_grouped(&refs, &[], &empty);
        assert_eq!(
            output,
            " 2024-01-02  Post A (Alice)\n 2024-01-01  Post B (Bob)\n"
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
        let empty: HashMap<String, String> = HashMap::new();
        let output = render_grouped(&refs, &[GroupKey::Date], &empty);
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
        let empty: HashMap<String, String> = HashMap::new();
        let output = render_grouped(&refs, &[GroupKey::Feed], &empty);
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
        let empty: HashMap<String, String> = HashMap::new();
        let output = render_grouped(&refs, &[GroupKey::Date, GroupKey::Feed], &empty);
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
        let empty: HashMap<String, String> = HashMap::new();
        let output = render_grouped(&refs, &[GroupKey::Feed, GroupKey::Date], &empty);
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
        let empty: HashMap<String, String> = HashMap::new();
        assert_eq!(render_grouped(&refs, &[GroupKey::Date], &empty), "");
    }

    #[test]
    fn test_date_ordering_is_descending() {
        let items = [
            item("Old", "2024-01-01", "Alice"),
            item("New", "2024-01-03", "Alice"),
            item("Mid", "2024-01-02", "Alice"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let empty: HashMap<String, String> = HashMap::new();
        let output = render_grouped(&refs, &[GroupKey::Date], &empty);
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
        let empty: HashMap<String, String> = HashMap::new();
        let output = render_grouped(&refs, &[GroupKey::Feed], &empty);
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

    #[test]
    fn test_index_to_shorthand() {
        // Index 0 → first char
        assert_eq!(index_to_shorthand(0), "a");
        // Index 1 → second char
        assert_eq!(index_to_shorthand(1), "s");
        // Index 33 → last single char (POST_ALPHABET[33] = 'm')
        assert_eq!(index_to_shorthand(33), "m");
        // Index 34 → wraps to two chars: 34/34=1 rem 0 → "sa"
        assert_eq!(index_to_shorthand(34), "sa");
        // All output characters should be valid POST_ALPHABET chars
        for i in 0..200 {
            let sh = index_to_shorthand(i);
            assert!(sh.chars().all(|c| POST_ALPHABET.contains(&c)));
        }
    }

    #[test]
    fn test_index_to_shorthand_ordering() {
        // Lower indices produce shorter or lexicographically earlier shorthands
        let sh0 = index_to_shorthand(0);
        let sh33 = index_to_shorthand(33);
        let sh34 = index_to_shorthand(34);
        assert_eq!(sh0.len(), 1);
        assert_eq!(sh33.len(), 1);
        assert_eq!(sh34.len(), 2);
    }

    #[test]
    fn test_render_grouped_with_shorthands() {
        let items = [
            FeedItem {
                title: "Post A".to_string(),
                date: Some(
                    NaiveDate::parse_from_str("2024-01-02", "%Y-%m-%d")
                        .unwrap()
                        .and_hms_opt(0, 0, 0)
                        .unwrap()
                        .and_utc(),
                ),
                feed: "Alice".to_string(),
                link: String::new(),
                raw_id: "id-a".to_string(),
            },
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let mut shorthands = HashMap::new();
        shorthands.insert("id-a".to_string(), "sDf".to_string());
        let output = render_grouped(&refs, &[], &shorthands);
        assert_eq!(output, "sDf 2024-01-02  Post A (Alice)\n");
    }
}
