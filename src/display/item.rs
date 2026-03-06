use unicode_width::UnicodeWidthStr;

use crate::data::schema::FeedItem;
use crate::query::GroupKey;

use super::RenderCtx;

const READ_MARKER_WIDTH: usize = 2; // "* " or "  "

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
        match feed_label.split_once(' ') {
            Some((t, b)) if t.starts_with('@') => (Some(t), b),
            _ => (None, feed_label),
        }
    } else {
        (None, "")
    };

    let meta_fixed_width = if show_feed {
        match tag {
            Some(t) => 2 + t.width() + 1 + 1,
            None => 2 + 1,
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

    let (bold, dim, italic, date_color, reset) = if ctx.color {
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

    let read_marker = if is_read { "  " } else { "* " };

    format!(
        "{read_marker}{date_part}{bold}{shorthand:<sw$}{reset} {title}{styled_meta}",
        sw = ctx.shorthand_width
    )
}
