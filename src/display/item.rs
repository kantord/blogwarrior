use unicode_width::UnicodeWidthStr;

use crate::data::schema::FeedItem;
use crate::query::GroupKey;

use super::{RenderCtx, Style};

const READ_MARKER_WIDTH: usize = 2; // "* " or "  "
const META_PAREN_WIDTH: usize = 3; // " (" + ")"
const META_TAG_SPACE: usize = 1; // space between tag and blog name

pub(crate) fn format_date(item: &FeedItem) -> String {
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

/// Split a feed label like "@tag Blog Name" into (Some("@tag"), "Blog Name").
/// Labels without an @-tag return (None, full_label).
fn resolve_feed_label(feed_label: &str) -> (Option<&str>, &str) {
    match feed_label.split_once(' ') {
        Some((t, b)) if t.starts_with('@') => (Some(t), b),
        _ => (None, feed_label),
    }
}

/// Fixed-width overhead from the meta suffix " (@tag BlogName)" or " (BlogName)".
fn meta_fixed_width(tag: Option<&str>) -> usize {
    match tag {
        Some(t) => META_PAREN_WIDTH + t.width() + META_TAG_SPACE,
        None => META_PAREN_WIDTH,
    }
}

/// Decide how much space title and blog name each get, truncating if needed.
fn budget_title_and_blog(
    title: &str,
    blog_name: &str,
    show_feed: bool,
    content_width: Option<usize>,
    fixed_width: usize,
    meta_width: usize,
) -> (String, String) {
    match content_width {
        Some(w) if fixed_width + meta_width < w => {
            let remaining = w - fixed_width - meta_width;
            let title_len = title.width();
            let blog_len = blog_name.width();

            if title_len + blog_len <= remaining {
                (title.to_string(), blog_name.to_string())
            } else if !show_feed {
                (truncate_str(title, remaining), String::new())
            } else {
                let blog_budget = (remaining * 35 / 100).max(3).min(blog_len);
                let title_budget = remaining.saturating_sub(blog_budget);
                (
                    truncate_str(title, title_budget),
                    truncate_str(blog_name, blog_budget),
                )
            }
        }
        _ => (title.to_string(), blog_name.to_string()),
    }
}

pub(super) fn format_item(
    item: &FeedItem,
    content_width: Option<usize>,
    ctx: &RenderCtx,
) -> String {
    let shorthand = ctx
        .shorthands
        .get(&item.raw_id)
        .map(|s| s.as_str())
        .unwrap_or("");
    let is_read = ctx.read_ids.contains(&item.raw_id);
    let show_date = !ctx.all_keys.contains(&GroupKey::Date);
    let show_feed = !ctx.all_keys.contains(&GroupKey::Feed);
    let feed_label = ctx
        .feed_labels
        .get(&item.feed)
        .map(|s| s.as_str())
        .unwrap_or(&item.feed);

    let date_width = if show_date {
        format_date(item).width() + 2
    } else {
        0
    };
    let fixed_width = READ_MARKER_WIDTH + date_width + ctx.shorthand_width + 1;

    let (tag, blog_name) = if show_feed {
        resolve_feed_label(feed_label)
    } else {
        (None, "")
    };

    let meta_width = if show_feed { meta_fixed_width(tag) } else { 0 };

    let (title, blog) = budget_title_and_blog(
        &item.title,
        blog_name,
        show_feed,
        content_width,
        fixed_width,
        meta_width,
    );

    let s = Style::new(ctx.color);

    let styled_meta = if show_feed {
        match tag {
            Some(t) => format!("{}{} ({t} {blog}){}", s.dim, s.italic, s.reset),
            None => format!("{}{} ({blog}){}", s.dim, s.italic, s.reset),
        }
    } else {
        String::new()
    };

    let date_part = if show_date {
        format!("{}{}{}  ", s.date_color, format_date(item), s.reset)
    } else {
        String::new()
    };

    let read_marker = if is_read { "  " } else { "* " };

    format!(
        "{read_marker}{date_part}{}{shorthand:<sw$}{} {title}{styled_meta}",
        s.bold,
        s.reset,
        sw = ctx.shorthand_width
    )
}
