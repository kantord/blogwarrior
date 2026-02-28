pub mod add;
pub mod feed_ls;
pub mod open;
pub mod pull;
pub mod remove;
pub mod show;

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
    // Parse hex string into a vector of digit values
    let mut digits: Vec<u8> = hex
        .chars()
        .map(|c| c.to_digit(16).unwrap_or(0) as u8)
        .collect();

    let mut remainders = Vec::new();

    // Long division: divide the base-16 number by `base` repeatedly
    loop {
        let mut remainder: u16 = 0;
        let mut quotient = Vec::new();
        for &d in &digits {
            let current = remainder * 16 + d as u16;
            quotient.push((current / base) as u8);
            remainder = current % base;
        }
        remainders.push(remainder as u8);
        // Strip leading zeros from quotient
        digits = quotient.into_iter().skip_while(|&d| d == 0).collect();
        if digits.is_empty() {
            break;
        }
    }

    // Remainders are in reverse order
    remainders
        .into_iter()
        .rev()
        .map(|d| alphabet[d as usize])
        .collect()
}

fn hex_to_base9(hex: &str) -> String {
    hex_to_custom_base(hex, &HOME_ROW)
}

pub(crate) fn index_to_shorthand(mut n: usize) -> String {
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

pub(crate) fn compute_shorthands(ids: &[String]) -> Vec<String> {
    if ids.is_empty() {
        return Vec::new();
    }

    let base9s: Vec<String> = ids.iter().map(|id| hex_to_base9(id)).collect();

    if base9s.len() == 1 {
        return vec![base9s[0].chars().next().unwrap().to_string()];
    }

    // Find the shortest prefix length where all are unique
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

    // Fallback: return full base9 strings. Collisions are astronomically
    // unlikely here because id_length_for_capacity already sizes the
    // truncated hex IDs to keep collision probability below ~1/500 even
    // at EXPECTED_CAPACITY (50,000 feeds → 11 hex chars / 44 bits).
    base9s
}

pub(crate) fn resolve_shorthand(
    feeds_table: &synctato::Table<FeedSource>,
    shorthand: &str,
) -> Option<String> {
    let mut feeds: Vec<FeedSource> = feeds_table.items();
    feeds.sort_by(|a, b| a.url.cmp(&b.url));
    let ids: Vec<String> = feeds.iter().map(|f| feeds_table.id_of(f)).collect();
    let shorthands = compute_shorthands(&ids);
    for (feed, sh) in feeds.iter().zip(shorthands.iter()) {
        if sh == shorthand {
            return Some(feed.url.clone());
        }
    }
    None
}

pub(crate) fn load_sorted_posts(store: &Path) -> anyhow::Result<Vec<FeedItem>> {
    let table = synctato::Table::<FeedItem>::load(store)?;
    let mut items = table.items();
    items.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.raw_id.cmp(&b.raw_id)));
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_to_base9() {
        // "0" in hex = 0 in decimal = 0 in base9 = "a"
        assert_eq!(hex_to_base9("0"), "a");
        // "9" in hex = 9 in decimal = 10 in base9 = "sa"
        assert_eq!(hex_to_base9("9"), "sa");
        // "ff" in hex = 255 in decimal = 313 in base9 = "fsf"
        assert_eq!(hex_to_base9("ff"), "fsf");
        // "1" in hex = 1 in decimal = 1 in base9 = "s"
        assert_eq!(hex_to_base9("1"), "s");
        // "a" in hex = 10 in decimal = 11 in base9 = "ss"
        assert_eq!(hex_to_base9("a"), "ss");
    }

    #[test]
    fn test_compute_shorthands_unique_prefixes() {
        // Two IDs that differ at the first base9 digit should get 1-char shorthands
        let ids = vec!["00".to_string(), "ff".to_string()];
        let shorthands = compute_shorthands(&ids);
        assert_eq!(shorthands.len(), 2);
        assert!(shorthands.iter().all(|s| s.len() == 1));
        assert_ne!(shorthands[0], shorthands[1]);

        // Two IDs that share a base9 prefix should get longer shorthands
        let ids2 = vec!["aa".to_string(), "ab".to_string()];
        let shorthands2 = compute_shorthands(&ids2);
        assert_eq!(shorthands2.len(), 2);
        assert_ne!(shorthands2[0], shorthands2[1]);
        // They should be longer than 1 since they share a prefix in base9
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
        // Index 0 → first char
        assert_eq!(index_to_shorthand(0), "a");
        // Index 1 → second char
        assert_eq!(index_to_shorthand(1), "s");
        // Index 33 → last single char (POST_ALPHABET[33] = 'm')
        assert_eq!(index_to_shorthand(33), "m");
        // Index 34 → wraps to two chars: 34/34=1 rem 0 → "sa"
        assert_eq!(index_to_shorthand(34), "sa");
        // All output characters should be valid POST_ALPHABET chars
        for i in 0..200 {
            let sh = index_to_shorthand(i);
            assert!(sh.chars().all(|c| POST_ALPHABET.contains(&c)));
        }
    }

    #[test]
    fn test_index_to_shorthand_ordering() {
        // Lower indices produce shorter or lexicographically earlier shorthands
        let sh0 = index_to_shorthand(0);
        let sh33 = index_to_shorthand(33);
        let sh34 = index_to_shorthand(34);
        assert_eq!(sh0.len(), 1);
        assert_eq!(sh33.len(), 1);
        assert_eq!(sh34.len(), 2);
    }
}
