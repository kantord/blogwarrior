use anyhow::ensure;

use crate::data::BlogData;
use crate::data::schema::{FeedItem, ReadMark};
use crate::query::Query;
use crate::query::resolve::resolve_posts;

pub(crate) fn cmd_open(store: &mut BlogData, query: &Query) -> anyhow::Result<()> {
    let resolved = resolve_posts(store, query)?;
    ensure!(!resolved.items.is_empty(), "No matching posts");
    ensure!(
        resolved.items.len() == 1,
        "Expected exactly 1 post, got {}",
        resolved.items.len()
    );
    let item = &resolved.items[0];
    ensure!(!item.link.is_empty(), "Post has no link");
    match std::env::var("BROWSER") {
        Ok(browser) => {
            // Run directly so TUI browsers (w3m, elinks) inherit the terminal
            let status = std::process::Command::new(&browser)
                .arg(&item.link)
                .status()
                .map_err(|e| anyhow::anyhow!("Could not open URL: {}", e))?;
            if !status.success() {
                anyhow::bail!("{} exited with {}", browser, status);
            }
        }
        Err(_) => {
            open::that(&item.link).map_err(|e| anyhow::anyhow!("Could not open URL: {}", e))?;
        }
    }
    eprintln!("Opened in browser: {}", item.link);
    mark_read_batch(store, &resolved.items)?;
    Ok(())
}

pub(crate) fn cmd_read(store: &mut BlogData, query: &Query) -> anyhow::Result<()> {
    let resolved = resolve_posts(store, query)?;
    ensure!(!resolved.items.is_empty(), "No matching posts");
    for item in &resolved.items {
        ensure!(!item.link.is_empty(), "Post has no link");
        println!("{}", item.link);
    }
    mark_read_batch(store, &resolved.items)?;
    Ok(())
}

pub(crate) fn cmd_unread(store: &mut BlogData, query: &Query) -> anyhow::Result<()> {
    let resolved = resolve_posts(store, query)?;
    ensure!(!resolved.items.is_empty(), "No matching posts");
    mark_unread_batch(store, &resolved.items)?;
    Ok(())
}

fn mark_read_batch(store: &mut BlogData, items: &[FeedItem]) -> anyhow::Result<()> {
    let now = chrono::Utc::now();
    store.transact("mark read", |tx| {
        for item in items {
            if !tx.reads.contains_key(&item.raw_id) {
                tx.reads.upsert(ReadMark {
                    post_id: item.raw_id.clone(),
                    read_at: now,
                });
            }
        }
        Ok(())
    })
}

fn mark_unread_batch(store: &mut BlogData, items: &[FeedItem]) -> anyhow::Result<()> {
    store.transact("mark unread", |tx| {
        for item in items {
            tx.reads.delete(&item.raw_id);
        }
        Ok(())
    })
}
