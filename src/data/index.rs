use std::collections::HashMap;

use crate::data::BlogData;
use crate::data::schema::{FeedItem, FeedSource};
use crate::query::{Query, ReadFilter};
use crate::shorthand::{RESERVED_COMMANDS, compute_shorthands, index_to_shorthand};
use std::collections::HashSet;

pub(crate) struct FeedEntry {
    pub feed: FeedSource,
    pub id: String,
    pub shorthand: String,
}

pub(crate) struct FeedIndex {
    pub entries: Vec<FeedEntry>,
}

impl FeedIndex {
    pub(crate) fn id_for_shorthand(&self, shorthand: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| e.shorthand == shorthand)
            .map(|e| e.id.as_str())
    }
}

pub(crate) fn feed_index(table: &synctato::Table<FeedSource>) -> FeedIndex {
    let mut feeds = table.items();
    feeds.sort_by(|a, b| a.url.cmp(&b.url));
    let ids: Vec<String> = feeds.iter().map(|f| table.id_of(f)).collect();
    let shorthands = compute_shorthands(&ids);
    let entries = feeds
        .into_iter()
        .zip(ids)
        .zip(shorthands)
        .map(|((feed, id), shorthand)| FeedEntry {
            feed,
            id,
            shorthand,
        })
        .collect();
    FeedIndex { entries }
}

pub(crate) struct PostIndex {
    pub items: Vec<FeedItem>,
    pub shorthands: HashMap<String, String>,
}

pub(crate) fn post_index(table: &synctato::Table<FeedItem>) -> PostIndex {
    let mut items = table.items();
    items.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.raw_id.cmp(&b.raw_id)));
    let mut idx = 0;
    let shorthands = items
        .iter()
        .map(|item| {
            loop {
                let sh = index_to_shorthand(idx);
                idx += 1;
                if !RESERVED_COMMANDS.contains(&sh.as_str()) {
                    return (item.raw_id.clone(), sh);
                }
            }
        })
        .collect();
    PostIndex { items, shorthands }
}

pub(crate) fn resolve_shorthand(
    feeds_table: &synctato::Table<FeedSource>,
    shorthand: &str,
) -> Option<String> {
    let fi = feed_index(feeds_table);
    fi.entries
        .iter()
        .find(|e| e.shorthand == shorthand)
        .map(|e| e.feed.url.clone())
}

pub(crate) fn build_feed_labels(fi: &FeedIndex) -> HashMap<String, String> {
    fi.entries
        .iter()
        .map(|e| {
            let label = if e.feed.title.is_empty() {
                format!("@{} {}", e.shorthand, e.feed.url)
            } else {
                format!("@{} {}", e.shorthand, e.feed.title)
            };
            (e.id.clone(), label)
        })
        .collect()
}

pub(crate) struct ResolvedPosts {
    pub items: Vec<FeedItem>,
    pub shorthands: HashMap<String, String>,
    pub feed_labels: HashMap<String, String>,
}

pub(crate) fn resolve_posts(store: &BlogData, query: &Query) -> anyhow::Result<ResolvedPosts> {
    let fi = feed_index(store.feeds());
    let feed_labels = build_feed_labels(&fi);

    let mut posts = post_index(store.posts());

    if !query.shorthands.is_empty() {
        let sh_set: HashSet<&str> = query.shorthands.iter().map(|s| s.as_str()).collect();
        let all_known: HashSet<&str> = posts.shorthands.values().map(|s| s.as_str()).collect();
        for sh in &query.shorthands {
            if !all_known.contains(sh.as_str()) {
                anyhow::bail!("Unknown shorthand: {sh}");
            }
        }
        posts.items.retain(|item| {
            posts
                .shorthands
                .get(&item.raw_id)
                .is_some_and(|s| sh_set.contains(s.as_str()))
        });
    }

    if let Some(ref shorthand) = query.filter {
        let feed_id = fi
            .id_for_shorthand(shorthand)
            .ok_or_else(|| anyhow::anyhow!("Unknown feed shorthand: @{shorthand}"))?;
        posts.items.retain(|item| item.feed == feed_id);
    }

    if let Some(since) = query.date_filter.since {
        posts
            .items
            .retain(|item| item.date.is_some_and(|d| d >= since));
    }
    if let Some(until) = query.date_filter.until {
        posts
            .items
            .retain(|item| item.date.is_some_and(|d| d <= until));
    }

    match query.read_filter {
        ReadFilter::Read => {
            posts
                .items
                .retain(|item| store.reads().contains_key(&item.raw_id));
        }
        ReadFilter::Unread => {
            posts
                .items
                .retain(|item| !store.reads().contains_key(&item.raw_id));
        }
        ReadFilter::Any | ReadFilter::All => {}
    }

    Ok(ResolvedPosts {
        items: posts.items,
        shorthands: posts.shorthands,
        feed_labels,
    })
}
