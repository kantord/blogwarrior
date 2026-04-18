use std::collections::HashSet;
use std::io::{self, IsTerminal, Write};

use anyhow::ensure;

use crate::data::BlogData;
use crate::data::schema::FeedItem;
use crate::display::{RenderCtx, Style, render_grouped};
use crate::query::{Query, ReadFilter};
use crate::query::resolve::resolve_posts;

pub(crate) struct ShowOpts {
    pub compact: bool,
}

pub(crate) fn cmd_show(store: &BlogData, query: &Query, opts: &ShowOpts) -> anyhow::Result<()> {
    let resolved = resolve_posts(store, query)?;
    ensure!(!resolved.items.is_empty(), "No matching posts");

    let read_ids: HashSet<String> = store
        .reads()
        .iter()
        .map(|(_, r)| r.post_id.clone())
        .collect();

    let color = std::io::stdout().is_terminal();
    let terminal = terminal_size::terminal_size();
    let max_width = terminal.map(|(w, _)| w.0 as usize);
    let refs: Vec<&FeedItem> = resolved.items.iter().map(|(_, item)| item).collect();
    let ctx = RenderCtx {
        all_keys: &query.keys,
        shorthands: &resolved.shorthands,
        feed_labels: &resolved.feed_labels,
        read_ids: &read_ids,
        color,
        shorthand_width: RenderCtx::shorthand_width_from(&refs, &resolved.shorthands),
        max_width,
        compact: opts.compact,
    };

    let mut output = render_grouped(&refs, &ctx);
    // Trim trailing blank lines from grouped output before appending summary
    let trimmed_len = output.trim_end().len();
    output.truncate(trimmed_len);
    output.push('\n');

    output.push_str(&format_summary(&refs, &read_ids, query, color));

    let is_tty = std::io::stdout().is_terminal();
    let term_height = terminal.map(|(_, h)| h.0 as usize).unwrap_or(usize::MAX);
    let line_count = output.lines().count();

    if is_tty && line_count > term_height {
        if output_with_pager(&output).is_err() {
            print!("{output}");
        }
    } else {
        print!("{output}");
    }

    Ok(())
}

pub(crate) fn format_summary(
    items: &[&FeedItem],
    read_ids: &HashSet<String>,
    query: &Query,
    color: bool,
) -> String {
    let count = items.len();
    let feed_count = {
        let mut feeds: Vec<&str> = items.iter().map(|i| i.feed.as_str()).collect();
        feeds.sort_unstable();
        feeds.dedup();
        feeds.len()
    };

    let status = match query.read_filter {
        ReadFilter::Unread => " unread",
        ReadFilter::Read => " read",
        // Any = default filter (.unread in default_query), count what's actually shown
        ReadFilter::Any => {
            let unread_count = items.iter().filter(|i| !read_ids.contains(&i.raw_id)).count();
            if unread_count == count {
                " unread"
            } else {
                ""
            }
        }
        ReadFilter::All => "",
    };

    let post_word = if count == 1 { "post" } else { "posts" };
    let feed_word = if feed_count == 1 { "feed" } else { "feeds" };

    let s = Style::new(color);
    format!(
        "\n{}{count}{status} {post_word} \u{00b7} {feed_count} {feed_word}{}\n",
        s.dim, s.reset
    )
}

fn output_with_pager(content: &str) -> io::Result<()> {
    let pager_env = std::env::var("PAGER").ok();
    let (bin, args) = match &pager_env {
        Some(val) => {
            let val = val.trim();
            match val.split_once(char::is_whitespace) {
                Some((b, a)) => (b.to_string(), vec![a.to_string()]),
                None => {
                    let mut args = Vec::new();
                    if val == "less" {
                        args.push("-R".to_string());
                    }
                    (val.to_string(), args)
                }
            }
        }
        None => ("less".to_string(), vec!["-R".to_string()]),
    };

    let mut child = std::process::Command::new(&bin)
        .args(&args)
        .stdin(std::process::Stdio::piped())
        .spawn()?;

    let result = child.stdin.as_mut().unwrap().write_all(content.as_bytes());

    // Ignore broken pipe — user quit the pager early, which is intentional
    if let Err(ref e) = result {
        if e.kind() != io::ErrorKind::BrokenPipe {
            result?;
        }
    }

    // Drop stdin to signal EOF
    drop(child.stdin.take());

    let _ = child.wait();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::DateFilter;

    fn make_item(title: &str, feed: &str, raw_id: &str) -> FeedItem {
        FeedItem {
            title: title.to_string(),
            date: None,
            feed: feed.to_string(),
            link: String::new(),
            raw_id: raw_id.to_string(),
        }
    }

    fn make_query(read_filter: ReadFilter) -> Query {
        Query {
            keys: vec![],
            filter: None,
            id_filter: None,
            date_filter: DateFilter {
                since: None,
                until: None,
            },
            shorthands: vec![],
            read_filter,
        }
    }

    #[test]
    fn test_format_summary_unread() {
        let items = vec![
            make_item("A", "feed1", "id-a"),
            make_item("B", "feed2", "id-b"),
            make_item("C", "feed1", "id-c"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let mut read_ids = HashSet::new();
        read_ids.insert("id-b".to_string());

        let q = make_query(ReadFilter::Unread);
        let summary = format_summary(&refs, &read_ids, &q, false);
        assert_eq!(summary, "\n3 unread posts \u{00b7} 2 feeds\n");
    }

    #[test]
    fn test_format_summary_all() {
        let items = vec![make_item("A", "feed1", "id-a")];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let read_ids = HashSet::new();

        let q = make_query(ReadFilter::All);
        let summary = format_summary(&refs, &read_ids, &q, false);
        assert_eq!(summary, "\n1 post \u{00b7} 1 feed\n");
    }

    #[test]
    fn test_format_summary_singular() {
        let items = vec![make_item("A", "feed1", "id-a")];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let read_ids = HashSet::new();

        let q = make_query(ReadFilter::Unread);
        let summary = format_summary(&refs, &read_ids, &q, false);
        assert_eq!(summary, "\n1 unread post \u{00b7} 1 feed\n");
    }

    #[test]
    fn test_format_summary_read_filter() {
        let items = vec![
            make_item("A", "feed1", "id-a"),
            make_item("B", "feed1", "id-b"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let read_ids = HashSet::new();

        let q = make_query(ReadFilter::Read);
        let summary = format_summary(&refs, &read_ids, &q, false);
        assert_eq!(summary, "\n2 read posts \u{00b7} 1 feed\n");
    }

    #[test]
    fn test_format_summary_any_all_unread() {
        // When ReadFilter::Any and all shown items are unread, say "unread"
        let items = vec![
            make_item("A", "feed1", "id-a"),
            make_item("B", "feed2", "id-b"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let read_ids = HashSet::new();

        let q = make_query(ReadFilter::Any);
        let summary = format_summary(&refs, &read_ids, &q, false);
        assert_eq!(summary, "\n2 unread posts \u{00b7} 2 feeds\n");
    }

    #[test]
    fn test_format_summary_any_mixed() {
        // When ReadFilter::Any but some items are read, don't say "unread"
        let items = vec![
            make_item("A", "feed1", "id-a"),
            make_item("B", "feed2", "id-b"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let mut read_ids = HashSet::new();
        read_ids.insert("id-a".to_string());

        let q = make_query(ReadFilter::Any);
        let summary = format_summary(&refs, &read_ids, &q, false);
        assert_eq!(summary, "\n2 posts \u{00b7} 2 feeds\n");
    }
}
