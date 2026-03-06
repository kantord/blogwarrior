mod group;
mod item;

use std::collections::{HashMap, HashSet};

use crate::query::GroupKey;

pub(crate) use group::render_grouped;

pub(crate) struct RenderCtx<'a> {
    pub all_keys: &'a [GroupKey],
    pub shorthands: &'a HashMap<String, String>,
    pub feed_labels: &'a HashMap<String, String>,
    pub read_ids: &'a HashSet<String>,
    pub color: bool,
    pub shorthand_width: usize,
    pub max_width: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::schema::FeedItem;
    use crate::query::DateFilter;
    use chrono::{DateTime, NaiveDate, Utc};
    use rstest::rstest;

    fn utc_date(year: i32, month: u32, day: u32) -> DateTime<Utc> {
        NaiveDate::from_ymd_opt(year, month, day)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
    }

    fn feed_item(title: &str, date: &str, feed: &str) -> FeedItem {
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

    fn feed_item_with_raw_id(title: &str, date: &str, feed: &str, raw_id: &str) -> FeedItem {
        FeedItem {
            raw_id: raw_id.to_string(),
            ..feed_item(title, date, feed)
        }
    }

    fn no_labels() -> HashMap<String, String> {
        HashMap::new()
    }

    fn no_reads() -> HashSet<String> {
        HashSet::new()
    }

    #[rstest]
    #[case::unread_no_grouping(&[], false, "* 2024-01-15  abc Post (Alice)")]
    #[case::unread_grouped_by_date(&[GroupKey::Date], false, "* abc Post (Alice)")]
    #[case::unread_grouped_by_feed(&[GroupKey::Feed], false, "* 2024-01-15  abc Post")]
    #[case::unread_grouped_by_both(&[GroupKey::Date, GroupKey::Feed], false, "* abc Post")]
    #[case::read_no_grouping(&[], true, "  2024-01-15  abc Post (Alice)")]
    #[case::read_grouped_by_date(&[GroupKey::Date], true, "  abc Post (Alice)")]
    #[case::read_grouped_by_feed(&[GroupKey::Feed], true, "  2024-01-15  abc Post")]
    #[case::read_grouped_by_both(&[GroupKey::Date, GroupKey::Feed], true, "  abc Post")]
    fn test_format_item_read_marker(
        #[case] keys: &[GroupKey],
        #[case] is_read: bool,
        #[case] expected: &str,
    ) {
        let i = feed_item("Post", "2024-01-15", "Alice");
        let mut shorthands = HashMap::new();
        shorthands.insert(i.raw_id.clone(), "abc".to_string());
        let mut read_ids = HashSet::new();
        if is_read {
            read_ids.insert(i.raw_id.clone());
        }
        let ctx = RenderCtx {
            all_keys: keys,
            shorthands: &shorthands,
            feed_labels: &no_labels(),
            read_ids: &read_ids,
            color: false,
            shorthand_width: 3,
            max_width: None,
        };
        assert_eq!(item::format_item(&i, None, &ctx), expected);
    }

    #[test]
    fn test_format_date_with_date() {
        let i = feed_item("Post", "2024-01-15", "Alice");
        assert_eq!(item::format_date(&i), "2024-01-15");
    }

    #[test]
    fn test_format_date_without_date() {
        let i = FeedItem {
            date: None,
            ..feed_item("Post", "2024-01-01", "Alice")
        };
        assert_eq!(item::format_date(&i), "unknown");
    }

    #[test]
    fn test_render_flat() {
        let items = [
            feed_item("Post A", "2024-01-02", "Alice"),
            feed_item("Post B", "2024-01-01", "Bob"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();

        let output = render_grouped(
            &refs,
            &[],
            &no_labels(),
            &no_labels(),
            &no_reads(),
            false,
            None,
        );
        assert_eq!(
            output,
            "* 2024-01-02   Post A (Alice)\n* 2024-01-01   Post B (Bob)\n"
        );
    }

    #[test]
    fn test_render_flat_with_read_marks() {
        let items = [
            feed_item_with_raw_id("Post A", "2024-01-02", "Alice", "id-a"),
            feed_item_with_raw_id("Post B", "2024-01-01", "Bob", "id-b"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let read_ids: HashSet<String> = ["id-a".to_string()].into();

        let output = render_grouped(
            &refs,
            &[],
            &no_labels(),
            &no_labels(),
            &read_ids,
            false,
            None,
        );
        assert_eq!(
            output,
            "  2024-01-02   Post A (Alice)\n* 2024-01-01   Post B (Bob)\n"
        );
    }

    #[test]
    fn test_render_grouped_with_mixed_read_status() {
        let items = [
            feed_item_with_raw_id("Post A", "2024-01-02", "Alice", "id-a"),
            feed_item_with_raw_id("Post B", "2024-01-02", "Bob", "id-b"),
            feed_item_with_raw_id("Post C", "2024-01-01", "Alice", "id-c"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let read_ids: HashSet<String> = ["id-b".to_string()].into();

        let output = render_grouped(
            &refs,
            &[GroupKey::Date],
            &no_labels(),
            &no_labels(),
            &read_ids,
            false,
            None,
        );
        assert_eq!(
            output,
            "\
=== 2024-01-02 ===

  *  Post A (Alice)
     Post B (Bob)


=== 2024-01-01 ===

  *  Post C (Alice)


"
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
            &no_reads(),
            false,
            None,
        );
        assert_eq!(
            output,
            "\
=== 2024-01-02 ===

  *  Post A (Alice)
  *  Post B (Bob)


=== 2024-01-01 ===

  *  Post C (Alice)


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
            &no_reads(),
            false,
            None,
        );
        assert_eq!(
            output,
            "\
=== Alice ===

  * 2024-01-01   Post B


=== Bob ===

  * 2024-01-02   Post A


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
            &no_reads(),
            false,
            None,
        );
        assert_eq!(
            output,
            "\
=== 2024-01-02 ===

  --- Alice ---
    *  Post B

  --- Bob ---
    *  Post A



=== 2024-01-01 ===

  --- Alice ---
    *  Post C



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
            &no_reads(),
            false,
            None,
        );
        assert_eq!(
            output,
            "\
=== Alice ===

  --- 2024-01-02 ---
    *  Post B

  --- 2024-01-01 ---
    *  Post C



=== Bob ===

  --- 2024-01-02 ---
    *  Post A



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
                &no_reads(),
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
            &no_reads(),
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
            &no_reads(),
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
        let output = render_grouped(
            &refs,
            &[],
            &shorthands,
            &no_labels(),
            &no_reads(),
            false,
            None,
        );
        assert_eq!(output, "* 2024-01-02  sDf Post A (Alice)\n");
    }

    #[test]
    fn test_cjk_characters_respect_display_width() {
        use unicode_width::UnicodeWidthStr;

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

        let max_width = 40;
        let output = render_grouped(
            &refs,
            &[],
            &shorthands,
            &labels,
            &no_reads(),
            false,
            Some(max_width),
        );

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
        use unicode_width::UnicodeWidthStr;

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
        let output = render_grouped(
            &refs,
            &[],
            &shorthands,
            &labels,
            &no_reads(),
            false,
            Some(max_width),
        );

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
        let output = render_grouped(
            &filtered,
            &[],
            &no_labels(),
            &no_labels(),
            &no_reads(),
            false,
            None,
        );
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
