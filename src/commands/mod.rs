pub mod add;
pub mod clone;
pub mod feed_ls;
pub mod open;
mod pull;
pub mod remove;
pub mod show;
pub mod sync;

use std::collections::{HashMap, HashSet};

use crate::feed::FeedItem;
use crate::feed_source::FeedSource;
use crate::query::{Query, ReadFilter};
use crate::store::Store;

const HOME_ROW: [char; 9] = ['a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l'];

const POST_ALPHABET: [char; 34] = [
    'a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l', 'A', 'S', 'D', 'F', 'G', 'H', 'J', 'K', 'L', 'q',
    'w', 'e', 'r', 't', 'y', 'i', 'o', 'p', 'z', 'x', 'c', 'v', 'b', 'n', 'm',
];

fn hex_to_custom_base(hex: &str, alphabet: &[char]) -> String {
    let base = alphabet.len() as u16;
    if hex.is_empty() {
        return String::from(alphabet[0]);
    }
    let mut digits: Vec<u8> = hex
        .chars()
        .map(|c| c.to_digit(16).unwrap_or(0) as u8)
        .collect();

    let mut remainders = Vec::new();

    loop {
        let mut remainder: u16 = 0;
        let mut quotient = Vec::new();
        for &d in &digits {
            let current = remainder * 16 + d as u16;
            quotient.push((current / base) as u8);
            remainder = current % base;
        }
        remainders.push(remainder as u8);
        digits = quotient.into_iter().skip_while(|&d| d == 0).collect();
        if digits.is_empty() {
            break;
        }
    }

    remainders
        .into_iter()
        .rev()
        .map(|d| alphabet[d as usize])
        .collect()
}

fn hex_to_base9(hex: &str) -> String {
    hex_to_custom_base(hex, &HOME_ROW)
}

fn index_to_shorthand(mut n: usize) -> String {
    let base = POST_ALPHABET.len();
    if n == 0 {
        return POST_ALPHABET[0].to_string();
    }
    let mut chars = Vec::new();
    while n > 0 {
        chars.push(POST_ALPHABET[n % base]);
        n /= base;
    }
    chars.reverse();
    chars.into_iter().collect()
}

fn compute_shorthands(ids: &[String]) -> Vec<String> {
    if ids.is_empty() {
        return Vec::new();
    }

    let base9s: Vec<String> = ids.iter().map(|id| hex_to_base9(id)).collect();

    if base9s.len() == 1 {
        return vec![base9s[0].chars().next().unwrap().to_string()];
    }

    let max_len = base9s.iter().map(|s| s.len()).max().unwrap_or(1);
    for len in 1..=max_len {
        let prefixes: Vec<String> = base9s
            .iter()
            .map(|s| s.chars().take(len).collect::<String>())
            .collect();
        let unique: std::collections::HashSet<&String> = prefixes.iter().collect();
        if unique.len() == prefixes.len() {
            return prefixes;
        }
    }

    base9s
}

pub(crate) struct FeedIndex {
    pub feeds: Vec<FeedSource>,
    pub ids: Vec<String>,
    pub shorthands: Vec<String>,
}

impl FeedIndex {
    pub(crate) fn id_for_shorthand(&self, shorthand: &str) -> Option<&str> {
        self.shorthands
            .iter()
            .position(|sh| sh == shorthand)
            .map(|pos| self.ids[pos].as_str())
    }
}

pub(crate) fn feed_index(table: &crate::synctato::Table<FeedSource>) -> FeedIndex {
    let mut feeds = table.items();
    feeds.sort_by(|a, b| a.url.cmp(&b.url));
    let ids: Vec<String> = feeds.iter().map(|f| table.id_of(f)).collect();
    let shorthands = compute_shorthands(&ids);
    FeedIndex {
        feeds,
        ids,
        shorthands,
    }
}

pub(crate) struct PostIndex {
    pub items: Vec<FeedItem>,
    pub shorthands: HashMap<String, String>,
}

pub(crate) const RESERVED_COMMANDS: &[&str] = &[
    "show", "open", "read", "unread", "feed", "sync", "git", "clone",
];

pub(crate) fn post_index(table: &crate::synctato::Table<FeedItem>) -> PostIndex {
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
    feeds_table: &crate::synctato::Table<FeedSource>,
    shorthand: &str,
) -> Option<String> {
    let fi = feed_index(feeds_table);
    fi.feeds
        .iter()
        .zip(fi.shorthands.iter())
        .find(|(_, sh)| sh.as_str() == shorthand)
        .map(|(feed, _)| feed.url.clone())
}

pub(crate) fn build_feed_labels(fi: &FeedIndex) -> HashMap<String, String> {
    fi.ids
        .iter()
        .zip(fi.feeds.iter())
        .zip(fi.shorthands.iter())
        .map(|((id, feed), sh)| {
            let label = if feed.title.is_empty() {
                format!("@{} {}", sh, feed.url)
            } else {
                format!("@{} {}", sh, feed.title)
            };
            (id.clone(), label)
        })
        .collect()
}

pub(crate) struct ResolvedPosts {
    pub items: Vec<FeedItem>,
    pub shorthands: HashMap<String, String>,
    pub feed_labels: HashMap<String, String>,
}

pub(crate) fn resolve_posts(store: &Store, query: &Query) -> anyhow::Result<ResolvedPosts> {
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
        ReadFilter::Any => {}
    }

    Ok(ResolvedPosts {
        items: posts.items,
        shorthands: posts.shorthands,
        feed_labels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::zero("0", "a")]
    #[case::nine("9", "sa")]
    #[case::ff("ff", "fsf")]
    #[case::one("1", "s")]
    #[case::a("a", "ss")]
    fn test_hex_to_base9(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(hex_to_base9(input), expected);
    }

    #[test]
    fn test_compute_shorthands_unique_prefixes() {
        let ids = vec!["00".to_string(), "ff".to_string()];
        let shorthands = compute_shorthands(&ids);
        assert_eq!(shorthands.len(), 2);
        assert!(shorthands.iter().all(|s| s.len() == 1));
        assert_ne!(shorthands[0], shorthands[1]);

        let ids2 = vec!["aa".to_string(), "ab".to_string()];
        let shorthands2 = compute_shorthands(&ids2);
        assert_eq!(shorthands2.len(), 2);
        assert_ne!(shorthands2[0], shorthands2[1]);
        assert!(
            shorthands2[0].len() > 1
                || shorthands2[1].len() > 1
                || shorthands2[0] != shorthands2[1]
        );
    }

    #[test]
    fn test_compute_shorthands_single() {
        let ids = vec!["abcdef".to_string()];
        let shorthands = compute_shorthands(&ids);
        assert_eq!(shorthands.len(), 1);
        assert_eq!(shorthands[0].len(), 1);
    }

    #[test]
    fn test_compute_shorthands_empty() {
        let ids: Vec<String> = vec![];
        let shorthands = compute_shorthands(&ids);
        assert!(shorthands.is_empty());
    }

    #[rstest]
    #[case::zero(0, "a")]
    #[case::one(1, "s")]
    #[case::thirty_three(33, "m")]
    #[case::thirty_four(34, "sa")]
    fn test_index_to_shorthand(#[case] index: usize, #[case] expected: &str) {
        assert_eq!(index_to_shorthand(index), expected);
    }

    #[test]
    fn test_index_to_shorthand_uses_valid_chars() {
        for i in 0..200 {
            let sh = index_to_shorthand(i);
            assert!(sh.chars().all(|c| POST_ALPHABET.contains(&c)));
        }
    }

    #[test]
    fn test_index_to_shorthand_ordering() {
        let sh0 = index_to_shorthand(0);
        let sh33 = index_to_shorthand(33);
        let sh34 = index_to_shorthand(34);
        assert_eq!(sh0.len(), 1);
        assert_eq!(sh33.len(), 1);
        assert_eq!(sh34.len(), 2);
    }

    #[test]
    fn test_shorthand_skips_reserved_commands() {
        // Simulate the skip logic used in post_index and verify
        // that no generated shorthand collides with a reserved command.
        let mut idx = 0;
        let mut generated = Vec::new();
        for _ in 0..2000 {
            loop {
                let sh = index_to_shorthand(idx);
                idx += 1;
                if !RESERVED_COMMANDS.contains(&sh.as_str()) {
                    generated.push(sh);
                    break;
                }
            }
        }
        for sh in &generated {
            assert!(
                !RESERVED_COMMANDS.contains(&sh.as_str()),
                "shorthand {sh} collides with a reserved command"
            );
        }
    }
}
