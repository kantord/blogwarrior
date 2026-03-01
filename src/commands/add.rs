use std::time::Duration;

use anyhow::bail;
use indicatif::{ProgressBar, ProgressStyle};

use crate::feed_source::FeedSource;
use crate::store::Transaction;

fn spinner(msg: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}

pub(crate) fn resolve_feed_url(url: &str) -> anyhow::Result<String> {
    let client = crate::http::http_client()?;

    let sp = spinner(&format!("Fetching {url}..."));
    let response = client.get(url).send()?.error_for_status()?;
    let bytes = response.bytes()?;

    // Try parsing as RSS/Atom — if it works, the URL is already a feed
    if crate::feed::rss::parse(&bytes[..]).is_ok() || crate::feed::atom::parse(&bytes[..]).is_ok() {
        sp.finish_and_clear();
        return Ok(url.to_string());
    }

    // Not a feed — try HTML feed discovery
    sp.set_message(format!("Looking for feeds on {url}..."));
    let html = String::from_utf8_lossy(&bytes);
    let base_url = url::Url::parse(url)?;
    let candidates = feedfinder::detect_feeds(&base_url, &html)
        .map_err(|e| anyhow::anyhow!("feed discovery failed: {e:?}"))?;

    // Validate candidates by fetching and parsing each one, dedup by URL
    let mut seen = std::collections::HashSet::new();
    let feeds: Vec<_> = candidates
        .iter()
        .filter(|f| seen.insert(f.url().to_string()))
        .filter(|f| {
            sp.set_message(format!("Checking {}...", f.url()));
            is_valid_feed(&client, f.url().as_str())
        })
        .collect();

    sp.finish_and_clear();

    match feeds.len() {
        0 => bail!("no feeds found at {url}"),
        1 => {
            let feed_url = feeds[0].url().to_string();
            Ok(feed_url)
        }
        _ => {
            eprintln!("Multiple feeds found at {url}:");
            for feed in &feeds {
                let title = feed.title().unwrap_or("(untitled)");
                eprintln!("  {} — {title}", feed.url());
            }
            bail!(
                "multiple feeds found; run `blog feed add <feed-url>` with a specific URL from the list above"
            );
        }
    }
}

fn is_valid_feed(client: &reqwest::blocking::Client, url: &str) -> bool {
    let Ok(resp) = client.get(url).send().and_then(|r| r.error_for_status()) else {
        return false;
    };
    let Ok(bytes) = resp.bytes() else {
        return false;
    };
    crate::feed::rss::parse(&bytes[..]).is_ok() || crate::feed::atom::parse(&bytes[..]).is_ok()
}

pub(crate) fn cmd_add(tx: &mut Transaction, url: &str) -> anyhow::Result<()> {
    tx.feeds.upsert(FeedSource {
        url: url.to_string(),
        title: String::new(),
        site_url: String::new(),
        description: String::new(),
    });
    Ok(())
}
