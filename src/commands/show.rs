use std::collections::HashSet;
use std::io::{self, IsTerminal, Write};

use anyhow::ensure;

use crate::data::BlogData;
use crate::data::schema::FeedItem;
use crate::display::{RenderCtx, render_grouped};
use crate::query::Query;
use crate::query::resolve::resolve_posts;

pub(crate) fn cmd_show(store: &BlogData, query: &Query) -> anyhow::Result<()> {
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
    };
    let mut output = render_grouped(&refs, &ctx);
    // Trim trailing blank lines from grouped output
    let trimmed_len = output.trim_end().len();
    output.truncate(trimmed_len);
    output.push('\n');

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
