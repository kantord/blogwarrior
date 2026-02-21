mod feed;

use std::env;
use std::fmt::Write;

use itertools::Itertools;

use feed::FeedItem;

#[derive(Clone, Copy, Debug, PartialEq)]
enum GroupKey {
    Date,
    Author,
}

impl GroupKey {
    fn extract(&self, item: &FeedItem) -> String {
        match self {
            GroupKey::Date => format_date(item),
            GroupKey::Author => item.author.clone(),
        }
    }

    fn compare(&self, a: &FeedItem, b: &FeedItem) -> std::cmp::Ordering {
        match self {
            GroupKey::Date => format_date(b).cmp(&format_date(a)),
            GroupKey::Author => a.author.cmp(&b.author),
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
    let show_author = !grouped_keys.contains(&GroupKey::Author);
    match (show_date, show_author) {
        (true, true) => format!("{}  {} ({})", format_date(item), item.title, item.author),
        (true, false) => format!("{}  {}", format_date(item), item.title),
        (false, true) => format!("{} ({})", item.title, item.author),
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
            'a' => Some(GroupKey::Author),
            _ => None,
        })
        .collect()
}

fn main() {
    let grouping_arg = env::args().nth(1).unwrap_or_default();

    let keys = match parse_grouping(&grouping_arg) {
        Some(keys) => keys,
        None => {
            eprintln!("Unknown grouping: {grouping_arg}. Use: d, a, da, ad");
            return;
        }
    };

    let mut items: Vec<FeedItem> = vec![
        feed::rss::fetch("https://drewdevault.com/blog/index.xml"),
        feed::atom::fetch("https://michael.stapelberg.ch/feed.xml"),
    ]
    .into_iter()
    .flatten()
    .collect();

    items.sort_by(|a, b| b.date.cmp(&a.date));

    let refs: Vec<&FeedItem> = items.iter().collect();
    print!("{}", render_grouped(&refs, &keys));
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn item(title: &str, date: &str, author: &str) -> FeedItem {
        FeedItem {
            id: String::new(),
            source_id: String::new(),
            title: title.to_string(),
            date: Some(
                NaiveDate::parse_from_str(date, "%Y-%m-%d")
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
                    .and_utc(),
            ),
            author: author.to_string(),
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
    fn test_parse_grouping_author() {
        assert_eq!(parse_grouping("a"), Some(vec![GroupKey::Author]));
    }

    #[test]
    fn test_parse_grouping_date_author() {
        assert_eq!(
            parse_grouping("da"),
            Some(vec![GroupKey::Date, GroupKey::Author])
        );
    }

    #[test]
    fn test_parse_grouping_author_date() {
        assert_eq!(
            parse_grouping("ad"),
            Some(vec![GroupKey::Author, GroupKey::Date])
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
    fn test_format_item_grouped_by_author() {
        let i = item("Post", "2024-01-15", "Alice");
        assert_eq!(format_item(&i, &[GroupKey::Author]), "2024-01-15  Post");
    }

    #[test]
    fn test_format_item_grouped_by_both() {
        let i = item("Post", "2024-01-15", "Alice");
        assert_eq!(
            format_item(&i, &[GroupKey::Date, GroupKey::Author]),
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
            id: String::new(),
            source_id: String::new(),
            title: "Post".to_string(),
            date: None,
            author: "Alice".to_string(),
        };
        assert_eq!(format_date(&i), "unknown");
    }

    #[test]
    fn test_render_flat() {
        let items = vec![
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
        let items = vec![
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
    fn test_render_grouped_by_author() {
        let items = vec![
            item("Post A", "2024-01-02", "Bob"),
            item("Post B", "2024-01-01", "Alice"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let output = render_grouped(&refs, &[GroupKey::Author]);
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
    fn test_render_grouped_by_date_then_author() {
        let items = vec![
            item("Post A", "2024-01-02", "Bob"),
            item("Post B", "2024-01-02", "Alice"),
            item("Post C", "2024-01-01", "Alice"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let output = render_grouped(&refs, &[GroupKey::Date, GroupKey::Author]);
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
    fn test_render_grouped_by_author_then_date() {
        let items = vec![
            item("Post A", "2024-01-02", "Bob"),
            item("Post B", "2024-01-02", "Alice"),
            item("Post C", "2024-01-01", "Alice"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let output = render_grouped(&refs, &[GroupKey::Author, GroupKey::Date]);
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
        let items = vec![
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
    fn test_author_ordering_is_ascending() {
        let items = vec![
            item("Post", "2024-01-01", "Charlie"),
            item("Post", "2024-01-02", "Alice"),
            item("Post", "2024-01-03", "Bob"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let output = render_grouped(&refs, &[GroupKey::Author]);
        let headers: Vec<&str> = output.lines().filter(|l| l.starts_with("===")).collect();
        assert_eq!(
            headers,
            vec!["=== Alice ===", "=== Bob ===", "=== Charlie ==="]
        );
    }
}
