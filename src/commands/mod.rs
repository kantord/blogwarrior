pub mod add;
pub mod feed_ls;
pub mod open;
pub mod pull;
pub mod remove;
pub mod show;

use std::collections::HashMap;
use std::path::Path;

use crate::feed::FeedItem;
use crate::feed_source::FeedSource;

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

pub(crate) fn feed_index(table: &synctato::Table<FeedSource>) -> FeedIndex {
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

pub(crate) fn post_index(store: &Path) -> anyhow::Result<PostIndex> {
    let table = synctato::Table::<FeedItem>::load(store)?;
    let mut items = table.items();
    items.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.raw_id.cmp(&b.raw_id)));
    let shorthands = items
        .iter()
        .enumerate()
        .map(|(i, item)| (item.raw_id.clone(), index_to_shorthand(i)))
        .collect();
    Ok(PostIndex { items, shorthands })
}

pub(crate) fn resolve_shorthand(
    feeds_table: &synctato::Table<FeedSource>,
    shorthand: &str,
) -> Option<String> {
    let fi = feed_index(feeds_table);
    fi.feeds
        .iter()
        .zip(fi.shorthands.iter())
        .find(|(_, sh)| sh.as_str() == shorthand)
        .map(|(feed, _)| feed.url.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_to_base9() {
        assert_eq!(hex_to_base9("0"), "a");
        assert_eq!(hex_to_base9("9"), "sa");
        assert_eq!(hex_to_base9("ff"), "fsf");
        assert_eq!(hex_to_base9("1"), "s");
        assert_eq!(hex_to_base9("a"), "ss");
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

    #[test]
    fn test_index_to_shorthand() {
        assert_eq!(index_to_shorthand(0), "a");
        assert_eq!(index_to_shorthand(1), "s");
        assert_eq!(index_to_shorthand(33), "m");
        assert_eq!(index_to_shorthand(34), "sa");
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
}
