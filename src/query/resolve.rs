use std::collections::{HashMap, HashSet};

use regex::Regex;

use crate::data::BlogData;
use crate::data::index::{FeedIndex, feed_index};
use crate::data::schema::FeedItem;
use crate::display::build_feed_labels;
use crate::shorthand::{RESERVED_COMMANDS, index_to_shorthand};

use super::{Query, ReadFilter};

pub(crate) struct PostIndex {
    pub items: Vec<FeedItem>,
    pub shorthands: HashMap<String, String>,
}

impl PostIndex {
    fn filter_hidden_items(&mut self, hidden_link_regexes: &[Regex]) {
        if hidden_link_regexes.is_empty() {
            return;
        }
        self.items
            .retain(|item| !is_hidden_item(item, hidden_link_regexes));
    }

    fn filter_by_shorthands(&mut self, shorthands: &[String]) -> anyhow::Result<()> {
        if shorthands.is_empty() {
            return Ok(());
        }
        let sh_set: HashSet<&str> = shorthands.iter().map(|s| s.as_str()).collect();
        let all_known: HashSet<&str> = self.shorthands.values().map(|s| s.as_str()).collect();
        for sh in shorthands {
            if !all_known.contains(sh.as_str()) {
                anyhow::bail!("Unknown shorthand: {sh}");
            }
        }
        self.items.retain(|item| {
            self.shorthands
                .get(&item.raw_id)
                .is_some_and(|s| sh_set.contains(s.as_str()))
        });
        Ok(())
    }

    fn filter_by_feed(&mut self, fi: &FeedIndex, shorthand: &str) -> anyhow::Result<()> {
        let feed_id = fi
            .id_for_shorthand(shorthand)
            .ok_or_else(|| anyhow::anyhow!("Unknown feed shorthand: @{shorthand}"))?;
        self.items.retain(|item| item.feed == feed_id);
        Ok(())
    }

    fn filter_by_date(&mut self, query: &Query) {
        if let Some(since) = query.date_filter.since {
            self.items
                .retain(|item| item.date.is_some_and(|d| d >= since));
        }
        if let Some(until) = query.date_filter.until {
            self.items
                .retain(|item| item.date.is_some_and(|d| d <= until));
        }
    }

    fn filter_by_read_status(&mut self, filter: ReadFilter, store: &BlogData) {
        match filter {
            ReadFilter::Read | ReadFilter::Unread => {
                let read_ids: HashSet<&str> = store
                    .reads()
                    .iter()
                    .map(|(_, r)| r.post_id.as_str())
                    .collect();
                let keep_read = matches!(filter, ReadFilter::Read);
                self.items
                    .retain(|item| read_ids.contains(item.raw_id.as_str()) == keep_read);
            }
            ReadFilter::Any | ReadFilter::All => {}
        }
    }
}

fn is_hidden_item(item: &FeedItem, hidden_link_regexes: &[Regex]) -> bool {
    !item.link.is_empty()
        && hidden_link_regexes
            .iter()
            .any(|pattern| pattern.is_match(&item.link))
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

pub(crate) struct ResolvedPosts {
    pub items: Vec<FeedItem>,
    pub shorthands: HashMap<String, String>,
    pub feed_labels: HashMap<String, String>,
}

pub(crate) fn resolve_posts(store: &BlogData, query: &Query) -> anyhow::Result<ResolvedPosts> {
    let fi = feed_index(store.feeds());
    let feed_labels = build_feed_labels(&fi);
    let mut posts = post_index(store.posts());
    let hidden_link_regexes = crate::data::hidden_link_regexes()?;

    posts.filter_by_shorthands(&query.shorthands)?;
    if let Some(ref shorthand) = query.filter {
        posts.filter_by_feed(&fi, shorthand)?;
    }
    posts.filter_by_date(query);
    posts.filter_by_read_status(query.read_filter, store);
    posts.filter_hidden_items(&hidden_link_regexes);

    Ok(ResolvedPosts {
        items: posts.items,
        shorthands: posts.shorthands,
        feed_labels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn regexes(patterns: &[&str]) -> Vec<Regex> {
        patterns.iter().map(|p| Regex::new(p).unwrap()).collect()
    }

    fn item(link: &str) -> FeedItem {
        FeedItem {
            title: "Post".to_string(),
            date: Some(
                NaiveDate::from_ymd_opt(2024, 1, 1)
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
                    .and_utc(),
            ),
            feed: "feed".to_string(),
            link: link.to_string(),
            raw_id: "id".to_string(),
        }
    }

    #[test]
    fn test_is_hidden_item_matches_shorts_url() {
        assert!(is_hidden_item(
            &item("https://youtube.com/shorts/abc"),
            &regexes(&["/shorts/"])
        ));
    }

    #[test]
    fn test_is_hidden_item_keeps_regular_watch_url() {
        assert!(!is_hidden_item(
            &item("https://youtube.com/watch?v=abc"),
            &regexes(&["/shorts/"])
        ));
    }

    #[test]
    fn test_is_hidden_item_ignores_empty_link() {
        assert!(!is_hidden_item(&item(""), &regexes(&["/shorts/"])));
    }

    #[test]
    fn test_is_hidden_item_supports_multiple_patterns() {
        assert!(is_hidden_item(
            &item("https://example.com/live/abc"),
            &regexes(&["/shorts/", "/live/"])
        ));
    }
}
