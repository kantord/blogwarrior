//! Shorthands are short, keyboard-friendly identifiers for feeds and posts.
//!
//! **Feed shorthands** use the 9-key home row (`asdfghjkl`). Each feed's hex ID
//! is converted to base-9 using these characters, then the shortest unique prefix
//! is chosen so every feed gets a distinct shorthand (e.g. `a`, `sf`, `dk`).
//! Because they derive from the stable hex ID, feed shorthands tend to remain the
//! same across runs — users can rely on them in muscle memory and scripts.
//!
//! **Post shorthands** use a larger 34-character alphabet (home row + shifted +
//! remaining letters) to keep shorthands short even with many posts. Posts are
//! assigned sequentially by date order, skipping any that collide with reserved
//! command names. These are ephemeral — they only need to stay stable between
//! `blog sync` runs, since new posts shift the numbering.

/// Alphabet for feed shorthands: the 9 home-row keys (base-9 encoding).
const HOME_ROW: [char; 9] = ['a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l'];

/// Alphabet for post shorthands: home row, shifted home row, then remaining
/// letter keys. Gives base-34 encoding to keep shorthands short.
const POST_ALPHABET: [char; 34] = [
    'a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l', 'A', 'S', 'D', 'F', 'G', 'H', 'J', 'K', 'L', 'q',
    'w', 'e', 'r', 't', 'y', 'i', 'o', 'p', 'z', 'x', 'c', 'v', 'b', 'n', 'm',
];

pub(crate) const RESERVED_COMMANDS: &[&str] = &[
    "show", "open", "read", "unread", "feed", "sync", "git", "clone", "export", "config",
];

/// Convert a hex string to a custom base using the given alphabet.
/// Performs base conversion from base-16 digits to base-N where N = alphabet.len().
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

/// Convert a hex ID to a home-row string for use as a feed shorthand.
fn hex_to_home_row(hex: &str) -> String {
    hex_to_custom_base(hex, &HOME_ROW)
}

/// Convert a sequential index to a post shorthand using the post alphabet.
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

/// Compute the shortest unique home-row prefix for each hex ID.
pub(crate) fn compute_shorthands(ids: &[String]) -> Vec<String> {
    if ids.is_empty() {
        return Vec::new();
    }

    let encoded: Vec<String> = ids.iter().map(|id| hex_to_home_row(id)).collect();

    let max_len = encoded.iter().map(|s| s.len()).max().unwrap_or(1);
    for len in 1..=max_len {
        let prefixes: Vec<String> = encoded
            .iter()
            .map(|s| s.chars().take(len).collect::<String>())
            .collect();
        let unique: std::collections::HashSet<&String> = prefixes.iter().collect();
        if unique.len() == prefixes.len() {
            return prefixes;
        }
    }

    encoded
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
    fn test_hex_to_home_row(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(hex_to_home_row(input), expected);
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
            shorthands2.iter().all(|s| s.len() > 1),
            "shared hex prefix should force longer shorthands"
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
