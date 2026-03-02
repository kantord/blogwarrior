use std::collections::HashMap;
use std::fmt::Write;
use std::io::IsTerminal;

use anyhow::ensure;
use itertools::Itertools;
use unicode_width::UnicodeWidthStr;

use crate::feed::FeedItem;
use crate::query::{DateFilter, GroupKey};
use crate::store::Store;

use super::{feed_index, post_index};

fn format_date(item: &FeedItem) -> String {
    item.date
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn truncate_str(s: &str, max_cols: usize) -> String {
    if s.width() <= max_cols {
        return s.to_string();
    }
    if max_cols == 0 {
        return String::new();
    }
    let budget = max_cols - 1; // reserve 1 column for '…'
    let mut used = 0;
    let mut end = 0;
    for (i, c) in s.char_indices() {
        let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if used + cw > budget {
            break;
        }
        used += cw;
        end = i + c.len_utf8();
    }
    format!("{}\u{2026}", &s[..end])
}

fn format_item(
    item: &FeedItem,
    grouped_keys: &[GroupKey],
    shorthand: &str,
    feed_labels: &HashMap<String, String>,
    color: bool,
    shorthand_width: usize,
    content_width: Option<usize>,
) -> String {
    let show_date = !grouped_keys.contains(&GroupKey::Date);
    let show_feed = !grouped_keys.contains(&GroupKey::Feed);
    let feed_label = feed_labels
        .get(&item.feed)
        .map(|s| s.as_str())
        .unwrap_or(&item.feed);

    // Compute plain-text widths for fixed parts
    let date_width = if show_date {
        format_date(item).width() + 2 // "YYYY-MM-DD  "
    } else {
        0
    };
    let fixed_width = date_width + shorthand_width + 1; // +1 for space after shorthand

    // Split feed_label "@tag Blog Name" into fixed tag and truncatable blog name.
    // Labels from cmd_show always have "@shorthand title" format, but tests may
    // pass bare names like "Alice" — treat those as blog name only.
    let (tag, blog_name) = if show_feed {
        match feed_label.split_once(' ') {
            Some((t, b)) if t.starts_with('@') => (Some(t), b),
            _ => (None, feed_label),
        }
    } else {
        (None, "")
    };

    // Fixed meta overhead: parts that never truncate
    let meta_fixed_width = if show_feed {
        match tag {
            Some(t) => 2 + t.width() + 1 + 1, // " (@tag " + ")"
            None => 2 + 1,                    // " (" + ")"
        }
    } else {
        0
    };

    let (title, blog) = match content_width {
        Some(w) if fixed_width + meta_fixed_width < w => {
            let remaining = w - fixed_width - meta_fixed_width;
            let title_len = item.title.width();
            let blog_len = blog_name.width();

            if title_len + blog_len <= remaining {
                (item.title.clone(), blog_name.to_string())
            } else if !show_feed {
                (truncate_str(&item.title, remaining), String::new())
            } else {
                // Blog name gets at most 35% of remaining, post title gets the rest
                let blog_budget = (remaining * 35 / 100).max(3).min(blog_len);
                let title_budget = remaining.saturating_sub(blog_budget);
                (
                    truncate_str(&item.title, title_budget),
                    truncate_str(blog_name, blog_budget),
                )
            }
        }
        _ => (item.title.clone(), blog_name.to_string()),
    };

    // Apply ANSI styling after truncation
    let (bold, dim, italic, date_color, reset) = if color {
        ("\x1b[1m", "\x1b[2m", "\x1b[3m", "\x1b[36m", "\x1b[0m")
    } else {
        ("", "", "", "", "")
    };

    let styled_meta = if show_feed {
        match tag {
            Some(t) => format!("{dim}{italic} ({t} {blog}){reset}"),
            None => format!("{dim}{italic} ({blog}){reset}"),
        }
    } else {
        String::new()
    };

    let date_part = if show_date {
        format!("{date_color}{}{reset}  ", format_date(item))
    } else {
        String::new()
    };

    format!(
        "{date_part}{bold}{shorthand:<sw$}{reset} {title}{styled_meta}",
        sw = shorthand_width
    )
}

struct RenderCtx<'a> {
    all_keys: &'a [GroupKey],
    shorthands: &'a HashMap<String, String>,
    feed_labels: &'a HashMap<String, String>,
    color: bool,
    shorthand_width: usize,
    max_width: Option<usize>,
}

fn render_grouped(
    items: &[&FeedItem],
    keys: &[GroupKey],
    shorthands: &HashMap<String, String>,
    feed_labels: &HashMap<String, String>,
    color: bool,
    max_width: Option<usize>,
) -> String {
    fn recurse(out: &mut String, items: &[&FeedItem], remaining: &[GroupKey], ctx: &RenderCtx) {
        let depth = ctx.all_keys.len() - remaining.len();
        let indent = "  ".repeat(depth);

        if remaining.is_empty() {
            let indent_width = depth * 2;
            let content_width = ctx.max_width.map(|w| w.saturating_sub(indent_width));
            for item in items {
                let sh = ctx
                    .shorthands
                    .get(&item.raw_id)
                    .map(|s| s.as_str())
                    .unwrap_or("");
                writeln!(
                    out,
                    "{indent}{}",
                    format_item(
                        item,
                        ctx.all_keys,
                        sh,
                        ctx.feed_labels,
                        ctx.color,
                        ctx.shorthand_width,
                        content_width
                    )
                )
                .unwrap();
            }
            return;
        }

        let key = remaining[0];
        let rest = &remaining[1..];

        let mut sorted = items.to_vec();
        sorted.sort_by(|a, b| key.compare(a, b, ctx.feed_labels));

        let (bold, reset) = if ctx.color {
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
            .chunk_by(|item| key.extract(item, ctx.feed_labels))
        {
            let group_items: Vec<&FeedItem> = group.copied().collect();
            writeln!(out, "{indent}{bold}{prefix}{group_val}{suffix}{reset}").unwrap();
            if depth == 0 {
                writeln!(out).unwrap();
            }
            recurse(out, &group_items, rest, ctx);
            if depth == 0 {
                writeln!(out).unwrap();
                writeln!(out).unwrap();
            } else {
                writeln!(out).unwrap();
            }
        }
    }

    let shorthand_width = items
        .iter()
        .filter_map(|item| shorthands.get(&item.raw_id))
        .map(|s| s.len())
        .max()
        .unwrap_or(0);

    let ctx = RenderCtx {
        all_keys: keys,
        shorthands,
        feed_labels,
        color,
        shorthand_width,
        max_width,
    };

    let mut out = String::new();
    recurse(&mut out, items, keys, &ctx);
    out
}

pub(crate) fn cmd_show(
    store: &Store,
    keys: &[GroupKey],
    filter: Option<&str>,
    date_filter: &DateFilter,
) -> anyhow::Result<()> {
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

    if let Some(since) = date_filter.since {
        posts.items.retain(|item| match item.date {
            Some(d) => d >= since,
            None => false,
        });
    }
    if let Some(until) = date_filter.until {
        posts.items.retain(|item| match item.date {
            Some(d) => d <= until,
            None => false,
        });
    }

    ensure!(!posts.items.is_empty(), "No matching posts");

    let color = std::io::stdout().is_terminal();
    let max_width = terminal_size::terminal_size().map(|(w, _)| w.0 as usize);
    let refs: Vec<&FeedItem> = posts.items.iter().collect();
    print!(
        "{}",
        render_grouped(
            &refs,
            keys,
            &posts.shorthands,
            &feed_labels,
            color,
            max_width
        )
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{feed_item, feed_item_with_raw_id, utc_date};
    use chrono::{DateTime, Utc};
    use rstest::rstest;

    fn no_labels() -> HashMap<String, String> {
        HashMap::new()
    }

    #[rstest]
    #[case::no_grouping(&[], "2024-01-15  abc Post (Alice)")]
    #[case::grouped_by_date(&[GroupKey::Date], "abc Post (Alice)")]
    #[case::grouped_by_feed(&[GroupKey::Feed], "2024-01-15  abc Post")]
    #[case::grouped_by_both(&[GroupKey::Date, GroupKey::Feed], "abc Post")]
    fn test_format_item_grouping(#[case] keys: &[GroupKey], #[case] expected: &str) {
        let i = feed_item("Post", "2024-01-15", "Alice");
        assert_eq!(
            format_item(&i, keys, "abc", &no_labels(), false, 3, None),
            expected
        );
    }

    #[test]
    fn test_format_date_with_date() {
        let i = feed_item("Post", "2024-01-15", "Alice");
        assert_eq!(format_date(&i), "2024-01-15");
    }

    #[test]
    fn test_format_date_without_date() {
        let i = FeedItem {
            date: None,
            ..feed_item("Post", "2024-01-01", "Alice")
        };
        assert_eq!(format_date(&i), "unknown");
    }

    #[test]
    fn test_render_flat() {
        let items = [
            feed_item("Post A", "2024-01-02", "Alice"),
            feed_item("Post B", "2024-01-01", "Bob"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();

        let output = render_grouped(&refs, &[], &no_labels(), &no_labels(), false, None);
        assert_eq!(
            output,
            "2024-01-02   Post A (Alice)\n2024-01-01   Post B (Bob)\n"
        );
    }

    #[test]
    fn test_render_grouped_by_date() {
        let items = [
            feed_item("Post A", "2024-01-02", "Alice"),
            feed_item("Post B", "2024-01-02", "Bob"),
            feed_item("Post C", "2024-01-01", "Alice"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();

        let output = render_grouped(
            &refs,
            &[GroupKey::Date],
            &no_labels(),
            &no_labels(),
            false,
            None,
        );
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
            feed_item("Post A", "2024-01-02", "Bob"),
            feed_item("Post B", "2024-01-01", "Alice"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();

        let output = render_grouped(
            &refs,
            &[GroupKey::Feed],
            &no_labels(),
            &no_labels(),
            false,
            None,
        );
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
            feed_item("Post A", "2024-01-02", "Bob"),
            feed_item("Post B", "2024-01-02", "Alice"),
            feed_item("Post C", "2024-01-01", "Alice"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();

        let output = render_grouped(
            &refs,
            &[GroupKey::Date, GroupKey::Feed],
            &no_labels(),
            &no_labels(),
            false,
            None,
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
            feed_item("Post A", "2024-01-02", "Bob"),
            feed_item("Post B", "2024-01-02", "Alice"),
            feed_item("Post C", "2024-01-01", "Alice"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();

        let output = render_grouped(
            &refs,
            &[GroupKey::Feed, GroupKey::Date],
            &no_labels(),
            &no_labels(),
            false,
            None,
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
            render_grouped(
                &refs,
                &[GroupKey::Date],
                &no_labels(),
                &no_labels(),
                false,
                None
            ),
            ""
        );
    }

    #[test]
    fn test_date_ordering_is_descending() {
        let items = [
            feed_item("Old", "2024-01-01", "Alice"),
            feed_item("New", "2024-01-03", "Alice"),
            feed_item("Mid", "2024-01-02", "Alice"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();

        let output = render_grouped(
            &refs,
            &[GroupKey::Date],
            &no_labels(),
            &no_labels(),
            false,
            None,
        );
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
            feed_item("Post", "2024-01-01", "Charlie"),
            feed_item("Post", "2024-01-02", "Alice"),
            feed_item("Post", "2024-01-03", "Bob"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();

        let output = render_grouped(
            &refs,
            &[GroupKey::Feed],
            &no_labels(),
            &no_labels(),
            false,
            None,
        );
        let headers: Vec<&str> = output.lines().filter(|l| l.starts_with("===")).collect();
        assert_eq!(
            headers,
            vec!["=== Alice ===", "=== Bob ===", "=== Charlie ==="]
        );
    }

    #[test]
    fn test_render_grouped_with_shorthands() {
        let items = [feed_item_with_raw_id(
            "Post A",
            "2024-01-02",
            "Alice",
            "id-a",
        )];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let mut shorthands = HashMap::new();
        shorthands.insert("id-a".to_string(), "sDf".to_string());
        let output = render_grouped(&refs, &[], &shorthands, &no_labels(), false, None);
        assert_eq!(output, "2024-01-02  sDf Post A (Alice)\n");
    }

    #[test]
    fn test_cjk_characters_respect_display_width() {
        // "你好世界测试标题很长" = 10 chars but 20 display columns.
        // Current code uses chars().count() which sees 10 and thinks it fits,
        // but the actual display width is 20, blowing past max_width.
        let cjk_title = "你好世界测试标题很长";
        let items = [feed_item_with_raw_id(
            cjk_title,
            "2024-01-15",
            "feed1",
            "id1",
        )];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let mut shorthands = HashMap::new();
        shorthands.insert("id1".to_string(), "a".to_string());
        let mut labels = HashMap::new();
        labels.insert("feed1".to_string(), "@x Blog".to_string());

        // max_width = 40.  Fixed: date(12) + shorthand(1) + space(1) = 14.
        // Meta fixed: " (@x " + ")" = 6.  remaining = 40 - 14 - 6 = 20.
        // chars().count() of title = 10, blog = 4 → total 14 ≤ 20 → no truncation.
        // But display width of title = 20, so actual line = 45 columns. Must fail.
        let max_width = 40;
        let output = render_grouped(&refs, &[], &shorthands, &labels, false, Some(max_width));

        for line in output.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let width = line.width();
            assert!(
                width <= max_width,
                "line display width ({width}) exceeds max_width ({max_width}): {line}"
            );
        }
    }

    #[test]
    fn test_long_lines_are_truncated_to_max_width() {
        let long_title =
            "An extremely long post title that should definitely be truncated to fit the width";
        let items = [feed_item_with_raw_id(
            long_title,
            "2024-01-15",
            "feed1",
            "id1",
        )];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let mut shorthands = HashMap::new();
        shorthands.insert("id1".to_string(), "a".to_string());
        let mut labels = HashMap::new();
        labels.insert(
            "feed1".to_string(),
            "@x A Fairly Long Blog Name".to_string(),
        );

        let max_width = 60;
        let output = render_grouped(&refs, &[], &shorthands, &labels, false, Some(max_width));

        for line in output.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let width = line.width();
            assert!(
                width <= max_width,
                "line exceeds {max_width} columns ({width} chars): {line}",
            );
            assert!(
                line.contains('\u{2026}'),
                "truncated line should contain \u{2026}: {line}"
            );
        }
    }

    // --- Filtering tests ---

    fn filter_items(items: &[FeedItem], date_filter: &DateFilter) -> Vec<String> {
        let filtered: Vec<&FeedItem> = items
            .iter()
            .filter(|item| {
                if let Some(since) = date_filter.since {
                    match item.date {
                        Some(d) if d < since => return false,
                        None => return false,
                        _ => {}
                    }
                }
                if let Some(until) = date_filter.until {
                    match item.date {
                        Some(d) if d > until => return false,
                        None => return false,
                        _ => {}
                    }
                }
                true
            })
            .collect();
        let output = render_grouped(&filtered, &[], &no_labels(), &no_labels(), false, None);
        output
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.to_string())
            .collect()
    }

    #[rstest]
    #[case::since_filters_old(Some(utc_date(2024, 1, 15)), None, &["Mid Post", "New Post"], &["Old Post"])]
    #[case::until_filters_new(None, Some(utc_date(2024, 1, 15)), &["Old Post", "Mid Post"], &["New Post"])]
    #[case::since_and_until(Some(utc_date(2024, 1, 10)), Some(utc_date(2024, 1, 20)), &["Mid Post"], &["Old Post", "New Post"])]
    fn test_date_filter(
        #[case] since: Option<DateTime<Utc>>,
        #[case] until: Option<DateTime<Utc>>,
        #[case] present: &[&str],
        #[case] absent: &[&str],
    ) {
        let items = [
            feed_item("Old Post", "2024-01-01", "Alice"),
            feed_item("Mid Post", "2024-01-15", "Alice"),
            feed_item("New Post", "2024-02-01", "Alice"),
        ];
        let df = DateFilter { since, until };
        let lines = filter_items(&items, &df);
        for title in present {
            assert!(
                lines.iter().any(|l| l.contains(title)),
                "{title} should be included"
            );
        }
        for title in absent {
            assert!(
                !lines.iter().any(|l| l.contains(title)),
                "{title} should be filtered out"
            );
        }
    }

    #[rstest]
    #[case::since_includes_boundary(Some(utc_date(2024, 1, 15)), None, &["Exact"], &["Before"])]
    #[case::until_includes_boundary(None, Some(utc_date(2024, 1, 15)), &["Exact"], &["After"])]
    fn test_boundary_inclusion(
        #[case] since: Option<DateTime<Utc>>,
        #[case] until: Option<DateTime<Utc>>,
        #[case] present: &[&str],
        #[case] absent: &[&str],
    ) {
        let items = [
            feed_item("Before", "2024-01-14", "Alice"),
            feed_item("Exact", "2024-01-15", "Alice"),
            feed_item("After", "2024-01-16", "Alice"),
        ];
        let df = DateFilter { since, until };
        let lines = filter_items(&items, &df);
        for title in present {
            assert!(
                lines.iter().any(|l| l.contains(title)),
                "{title} should be included"
            );
        }
        for title in absent {
            assert!(
                !lines.iter().any(|l| l.contains(title)),
                "{title} should be filtered out"
            );
        }
    }
}
