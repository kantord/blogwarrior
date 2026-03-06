use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use itertools::Itertools;

use crate::data::schema::FeedItem;
use crate::query::GroupKey;

use super::RenderCtx;
use super::item::format_item;

pub(crate) fn render_grouped(
    items: &[&FeedItem],
    keys: &[GroupKey],
    shorthands: &HashMap<String, String>,
    feed_labels: &HashMap<String, String>,
    read_ids: &HashSet<String>,
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
                writeln!(out, "{indent}{}", format_item(item, content_width, ctx)).unwrap();
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
        read_ids,
        color,
        shorthand_width,
        max_width,
    };

    let mut out = String::new();
    recurse(&mut out, items, keys, &ctx);
    out
}
