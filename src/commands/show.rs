use std::collections::HashMap;
use std::fmt::Write;
use std::io::IsTerminal;

use anyhow::{bail, ensure};
use itertools::Itertools;

use crate::feed::FeedItem;
use crate::store::Store;

use super::{feed_index, post_index};

#[derive(Clone, Copy, Debug, PartialEq)]
enum GroupKey {
    Date,
    Feed,
}

impl GroupKey {
    fn extract(&self, item: &FeedItem, feed_labels: &HashMap<String, String>) -> String {
        match self {
            GroupKey::Date => format_date(item),
            GroupKey::Feed => feed_labels
                .get(&item.feed)
                .cloned()
                .unwrap_or_else(|| item.feed.clone()),
        }
    }

    fn compare(
        &self,
        a: &FeedItem,
        b: &FeedItem,
        feed_labels: &HashMap<String, String>,
    ) -> std::cmp::Ordering {
        match self {
            GroupKey::Date => format_date(b).cmp(&format_date(a)),
            GroupKey::Feed => self
                .extract(a, feed_labels)
                .cmp(&self.extract(b, feed_labels)),
        }
    }
}

fn format_date(item: &FeedItem) -> String {
    item.date
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn format_item(
    item: &FeedItem,
    grouped_keys: &[GroupKey],
    shorthand: &str,
    feed_labels: &HashMap<String, String>,
    color: bool,
) -> String {
    let show_date = !grouped_keys.contains(&GroupKey::Date);
    let show_feed = !grouped_keys.contains(&GroupKey::Feed);
    let feed_label = feed_labels
        .get(&item.feed)
        .map(|s| s.as_str())
        .unwrap_or(&item.feed);
    let (bold, dim, italic, date_color, reset) = if color {
        ("\x1b[1m", "\x1b[2m", "\x1b[3m", "\x1b[36m", "\x1b[0m")
    } else {
        ("", "", "", "", "")
    };
    let meta = if show_feed {
        format!(" {dim}{italic}({feed_label}){reset}")
    } else {
        String::new()
    };
    let date_part = if show_date {
        format!("{date_color}{}{reset}  ", format_date(item))
    } else {
        String::new()
    };
    format!("{date_part}{bold}{shorthand}{reset} {}{meta}", item.title)
}

fn render_grouped(
    items: &[&FeedItem],
    keys: &[GroupKey],
    shorthands: &HashMap<String, String>,
    feed_labels: &HashMap<String, String>,
    color: bool,
) -> String {
    fn recurse(
        out: &mut String,
        items: &[&FeedItem],
        remaining: &[GroupKey],
        all_keys: &[GroupKey],
        shorthands: &HashMap<String, String>,
        feed_labels: &HashMap<String, String>,
        color: bool,
    ) {
        let depth = all_keys.len() - remaining.len();
        let indent = "  ".repeat(depth);

        if remaining.is_empty() {
            for item in items {
                let sh = shorthands
                    .get(&item.raw_id)
                    .map(|s| s.as_str())
                    .unwrap_or("");
                writeln!(
                    out,
                    "{indent}{}",
                    format_item(item, all_keys, sh, feed_labels, color)
                )
                .unwrap();
            }
            return;
        }

        let key = remaining[0];
        let rest = &remaining[1..];

        let mut sorted = items.to_vec();
        sorted.sort_by(|a, b| key.compare(a, b, feed_labels));

        let (bold, reset) = if color {
            ("\x1b[1m", "\x1b[0m")
        } else {
            ("", "")
        };

        let (prefix, suffix) = if depth == 0 {
            ("=== ", " ===")
        } else {
            ("--- ", " ---")
        };

        for (group_val, group) in &sorted
            .iter()
            .chunk_by(|item| key.extract(item, feed_labels))
        {
            let group_items: Vec<&FeedItem> = group.copied().collect();
            writeln!(out, "{indent}{bold}{prefix}{group_val}{suffix}{reset}").unwrap();
            if depth == 0 {
                writeln!(out).unwrap();
            }
            recurse(
                out,
                &group_items,
                rest,
                all_keys,
                shorthands,
                feed_labels,
                color,
            );
            if depth == 0 {
                writeln!(out).unwrap();
                writeln!(out).unwrap();
            } else {
                writeln!(out).unwrap();
            }
        }
    }

    let mut out = String::new();
    recurse(&mut out, items, keys, keys, shorthands, feed_labels, color);
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

pub(crate) fn cmd_show(store: &Store, group: &str, filter: Option<&str>) -> anyhow::Result<()> {
    let keys = match parse_grouping(group) {
        Some(keys) => keys,
        None => bail!("Unknown grouping: {}. Use: d, f, df, fd", group),
    };

    let fi = feed_index(store.feeds());

    let filter_feed_id = match filter {
        Some(f) if f.starts_with('@') => {
            let shorthand = &f[1..];
            Some(
                fi.id_for_shorthand(shorthand)
                    .ok_or_else(|| anyhow::anyhow!("Unknown feed shorthand: @{}", shorthand))?
                    .to_string(),
            )
        }
        _ => None,
    };

    let feed_labels: HashMap<String, String> = fi
        .ids
        .iter()
        .zip(fi.feeds.iter())
        .zip(fi.shorthands.iter())
        .map(|((id, feed), sh)| {
            let label = if feed.title.is_empty() {
                format!("@{} {}", sh, feed.url)
            } else {
                format!("@{} {}", sh, feed.title)
            };
            (id.clone(), label)
        })
        .collect();

    let mut posts = post_index(store.posts());

    if let Some(ref feed_id) = filter_feed_id {
        posts.items.retain(|item| item.feed == *feed_id);
    }

    ensure!(!posts.items.is_empty(), "No matching posts");

    let color = std::io::stdout().is_terminal();
    let refs: Vec<&FeedItem> = posts.items.iter().collect();
    print!(
        "{}",
        render_grouped(&refs, &keys, &posts.shorthands, &feed_labels, color)
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn no_labels() -> HashMap<String, String> {
        HashMap::new()
    }

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
        assert_eq!(
            format_item(&i, &[], "abc", &no_labels(), false),
            "2024-01-15  abc Post (Alice)"
        );
    }

    #[test]
    fn test_format_item_grouped_by_date() {
        let i = item("Post", "2024-01-15", "Alice");
        assert_eq!(
            format_item(&i, &[GroupKey::Date], "abc", &no_labels(), false),
            "abc Post (Alice)"
        );
    }

    #[test]
    fn test_format_item_grouped_by_feed() {
        let i = item("Post", "2024-01-15", "Alice");
        assert_eq!(
            format_item(&i, &[GroupKey::Feed], "abc", &no_labels(), false),
            "2024-01-15  abc Post"
        );
    }

    #[test]
    fn test_format_item_grouped_by_both() {
        let i = item("Post", "2024-01-15", "Alice");
        assert_eq!(
            format_item(
                &i,
                &[GroupKey::Date, GroupKey::Feed],
                "abc",
                &no_labels(),
                false
            ),
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

        let output = render_grouped(&refs, &[], &no_labels(), &no_labels(), false);
        assert_eq!(
            output,
            "2024-01-02   Post A (Alice)\n2024-01-01   Post B (Bob)\n"
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

        let output = render_grouped(&refs, &[GroupKey::Date], &no_labels(), &no_labels(), false);
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

        let output = render_grouped(&refs, &[GroupKey::Feed], &no_labels(), &no_labels(), false);
        assert_eq!(
            output,
            "\
=== Alice ===

  2024-01-01   Post B


=== Bob ===

  2024-01-02   Post A


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

        let output = render_grouped(
            &refs,
            &[GroupKey::Date, GroupKey::Feed],
            &no_labels(),
            &no_labels(),
            false,
        );
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

        let output = render_grouped(
            &refs,
            &[GroupKey::Feed, GroupKey::Date],
            &no_labels(),
            &no_labels(),
            false,
        );
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

        assert_eq!(
            render_grouped(&refs, &[GroupKey::Date], &no_labels(), &no_labels(), false),
            ""
        );
    }

    #[test]
    fn test_date_ordering_is_descending() {
        let items = [
            item("Old", "2024-01-01", "Alice"),
            item("New", "2024-01-03", "Alice"),
            item("Mid", "2024-01-02", "Alice"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();

        let output = render_grouped(&refs, &[GroupKey::Date], &no_labels(), &no_labels(), false);
        let headers: Vec<&str> = output.lines().filter(|l| l.starts_with("===")).collect();
        assert_eq!(
            headers,
            vec![
                "=== 2024-01-03 ===",
                "=== 2024-01-02 ===",
                "=== 2024-01-01 ==="
            ]
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

        let output = render_grouped(&refs, &[GroupKey::Feed], &no_labels(), &no_labels(), false);
        let headers: Vec<&str> = output.lines().filter(|l| l.starts_with("===")).collect();
        assert_eq!(
            headers,
            vec!["=== Alice ===", "=== Bob ===", "=== Charlie ==="]
        );
    }

    #[test]
    fn test_render_grouped_with_shorthands() {
        let items = [FeedItem {
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
        }];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let mut shorthands = HashMap::new();
        shorthands.insert("id-a".to_string(), "sDf".to_string());
        let output = render_grouped(&refs, &[], &shorthands, &no_labels(), false);
        assert_eq!(output, "2024-01-02  sDf Post A (Alice)\n");
    }
}
