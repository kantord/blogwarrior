const FEED_TYPES: &[&str] = &[
    "application/rss+xml",
    "application/atom+xml",
    "application/feed+json",
];

const COMMON_FEED_FILENAMES: &[&str] = &[
    "feed.xml",
    "rss.xml",
    "index.xml",
    "feed",
    "feed/",
    "atom.xml",
    "atom",
    "atom/",
    "rss",
    "feed.rss",
    "feed.atom",
];

/// Feed-like path segments used to identify feed URLs in `<a>` tags.
const FEED_PATH_KEYWORDS: &[&str] = &["feed", "rss", "atom"];

/// Discover feed URLs from an HTML page.
///
/// Returns candidate feed URLs in priority order:
/// 1. URLs from `<link rel="alternate">` tags with feed MIME types
/// 2. URLs from `<a>` tags whose href contains a feed-like path segment
/// 3. Common feed paths relative to the page URL's parent directories and root
pub fn discover_feed_urls(html: &str, page_url: &url::Url) -> Vec<String> {
    let urls = find_link_tags(html, page_url);
    if !urls.is_empty() {
        return urls;
    }
    let mut urls = find_anchor_feed_links(html, page_url);
    let mut seen: std::collections::HashSet<String> = urls
        .iter()
        .map(|u| u.trim_end_matches('/').to_string())
        .collect();
    for u in guess_common_paths(page_url) {
        let key = u.trim_end_matches('/').to_string();
        if seen.insert(key) {
            urls.push(u);
        }
    }
    urls
}

/// Scan lowercased HTML for opening tags with the given name, calling `f` for each.
///
/// The tag name must be lowercase. For short tag names (e.g. `"a"`), a word-boundary
/// check ensures `<a` doesn't match `<aside>`.
fn for_each_tag(html: &str, tag_name: &str, mut f: impl FnMut(&str)) {
    let lower = html.to_lowercase();
    let needle = format!("<{tag_name}");
    let needle_len = needle.len();
    let mut search_from = 0;

    while let Some(start) = lower[search_from..].find(&needle).map(|i| i + search_from) {
        let after = start + needle_len;
        // Ensure the match is a real tag boundary (whitespace or '>'), not a prefix like <aside>
        if after < lower.len() {
            let next = lower.as_bytes()[after];
            if !next.is_ascii_whitespace() && next != b'>' {
                search_from = after;
                continue;
            }
        }
        let Some(end) = lower[start..].find('>').map(|i| i + start + 1) else {
            break;
        };
        search_from = end;
        f(&lower[start..end]);
    }
}

fn find_link_tags(html: &str, page_url: &url::Url) -> Vec<String> {
    let mut urls = Vec::new();

    for_each_tag(html, "link", |tag| {
        let rel = extract_attr(tag, "rel");
        let link_type = extract_attr(tag, "type");
        let href = extract_attr(tag, "href");

        let is_alternate = rel
            .as_deref()
            .is_some_and(|r| r.eq_ignore_ascii_case("alternate"));
        let is_feed_type = link_type.as_deref().is_some_and(|t| {
            FEED_TYPES
                .iter()
                .any(|&ft| t.trim().eq_ignore_ascii_case(ft))
        });

        if is_alternate
            && is_feed_type
            && let Some(href) = href
        {
            let href = href.trim();
            if let Ok(absolute) = page_url.join(href) {
                urls.push(absolute.to_string());
            }
        }
    });

    urls
}

/// Find feed-like URLs from `<a>` tags whose href path contains a feed keyword.
fn find_anchor_feed_links(html: &str, page_url: &url::Url) -> Vec<String> {
    let mut urls = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for_each_tag(html, "a", |tag| {
        let Some(href) = extract_attr(tag, "href") else {
            return;
        };
        let href = href.trim();

        // Strip query string and fragment before matching path segments
        let path_part = href.split(['?', '#']).next().unwrap_or(href);

        let is_feed_like = path_part.split('/').any(|seg| {
            FEED_PATH_KEYWORDS
                .iter()
                .any(|&kw| seg.eq_ignore_ascii_case(kw))
        });

        if is_feed_like && let Ok(absolute) = page_url.join(href) {
            let s = absolute.to_string();
            if seen.insert(s.clone()) {
                urls.push(s);
            }
        }
    });

    urls
}

/// Extract an attribute value from a lowercased HTML tag.
fn extract_attr(tag_lower: &str, attr_name: &str) -> Option<String> {
    // Find attr_name= preceded by whitespace to avoid matching data-type= when looking for type=
    let needle = format!("{attr_name}=");
    let mut search_from = 0;
    let pos = loop {
        let pos = tag_lower[search_from..]
            .find(&needle)
            .map(|i| i + search_from)?;
        if pos == 0 || tag_lower.as_bytes()[pos - 1].is_ascii_whitespace() {
            break pos;
        }
        search_from = pos + 1;
    };
    let after_eq = pos + needle.len();
    let rest = &tag_lower[after_eq..];

    let quote = rest.as_bytes().first()?;
    if *quote != b'"' && *quote != b'\'' {
        return None;
    }
    let quote_char = *quote as char;
    let value_start = 1;
    let value_end = rest[value_start..].find(quote_char)? + value_start;
    Some(rest[value_start..value_end].to_string())
}

fn guess_common_paths(page_url: &url::Url) -> Vec<String> {
    let path = page_url.path();
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    let mut urls = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // From root to deepest parent (root feeds are most common)
    for depth in 0..=segments.len() {
        let parent = if depth == 0 {
            "/".to_string()
        } else {
            format!("/{}/", segments[..depth].join("/"))
        };

        for filename in COMMON_FEED_FILENAMES {
            let candidate_path = format!("{parent}{filename}");
            if let Ok(candidate) = page_url.join(&candidate_path) {
                let s = candidate.to_string();
                let key = s.trim_end_matches('/').to_string();
                if seen.insert(key) {
                    urls.push(s);
                }
            }
        }
    }

    urls
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn parse_url(s: &str) -> url::Url {
        url::Url::parse(s).unwrap()
    }

    // === <link rel="alternate"> tag parsing ===

    #[test]
    fn test_finds_rss_link_tag() {
        let html = r#"<html><head>
            <link rel="alternate" type="application/rss+xml" href="/feed.xml">
        </head></html>"#;
        let url = parse_url("https://example.com/blog/post");
        let result = discover_feed_urls(html, &url);
        assert_eq!(result, vec!["https://example.com/feed.xml"]);
    }

    #[test]
    fn test_finds_atom_link_tag() {
        let html = r#"<html><head>
            <link rel="alternate" type="application/atom+xml" href="/atom.xml">
        </head></html>"#;
        let url = parse_url("https://example.com/");
        let result = discover_feed_urls(html, &url);
        assert_eq!(result, vec!["https://example.com/atom.xml"]);
    }

    #[test]
    fn test_finds_json_feed_link_tag() {
        let html = r#"<html><head>
            <link rel="alternate" type="application/feed+json" href="/feed.json">
        </head></html>"#;
        let url = parse_url("https://example.com/");
        let result = discover_feed_urls(html, &url);
        assert_eq!(result, vec!["https://example.com/feed.json"]);
    }

    #[test]
    fn test_finds_multiple_link_tags() {
        let html = r#"<html><head>
            <link rel="alternate" type="application/rss+xml" href="/rss.xml">
            <link rel="alternate" type="application/atom+xml" href="/atom.xml">
        </head></html>"#;
        let url = parse_url("https://example.com/");
        let result = discover_feed_urls(html, &url);
        assert_eq!(
            result,
            vec![
                "https://example.com/rss.xml",
                "https://example.com/atom.xml"
            ]
        );
    }

    #[test]
    fn test_resolves_relative_href() {
        let html = r#"<link rel="alternate" type="application/rss+xml" href="feed.xml">"#;
        let url = parse_url("https://example.com/blog/");
        let result = discover_feed_urls(html, &url);
        assert_eq!(result, vec!["https://example.com/blog/feed.xml"]);
    }

    #[test]
    fn test_resolves_absolute_href() {
        let html = r#"<link rel="alternate" type="application/rss+xml" href="https://example.com/feed.xml">"#;
        let url = parse_url("https://example.com/blog/post");
        let result = discover_feed_urls(html, &url);
        assert_eq!(result, vec!["https://example.com/feed.xml"]);
    }

    #[test]
    fn test_ignores_non_feed_type() {
        let html = r#"<html><head>
            <link rel="alternate" type="text/html" href="/page">
            <link rel="alternate" type="application/rss+xml" href="/feed.xml">
        </head></html>"#;
        let url = parse_url("https://example.com/");
        let result = discover_feed_urls(html, &url);
        assert_eq!(result, vec!["https://example.com/feed.xml"]);
    }

    #[test]
    fn test_ignores_non_alternate_rel() {
        let html = r#"<html><head>
            <link rel="stylesheet" type="application/rss+xml" href="/style.css">
        </head></html>"#;
        let url = parse_url("https://example.com/");
        let result = discover_feed_urls(html, &url);
        assert!(
            !result.contains(&"https://example.com/style.css".to_string()),
            "should not include non-alternate link"
        );
    }

    #[test]
    fn test_ignores_link_without_href() {
        let html = r#"<link rel="alternate" type="application/rss+xml">"#;
        let url = parse_url("https://example.com/");
        let result = discover_feed_urls(html, &url);
        // No <link> matched, so only fallback candidates should appear
        assert!(
            result
                .iter()
                .all(|u| COMMON_FEED_FILENAMES.iter().any(|f| u.ends_with(f))),
            "should only contain fallback candidates"
        );
    }

    #[test]
    fn test_ignores_link_without_type() {
        let html = r#"<link rel="alternate" href="/feed.xml">"#;
        let url = parse_url("https://example.com/");
        let result = discover_feed_urls(html, &url);
        // The <link> lacks a type, so it should not be picked up as a feed link.
        // Result should be fallback candidates only (which may include /feed.xml
        // by coincidence, but via fallback not via the <link> tag).
        // Verify by checking that other fallback paths are also present.
        assert!(
            result.contains(&"https://example.com/rss.xml".to_string()),
            "should fall back to common paths when <link> has no type"
        );
    }

    #[test]
    fn test_case_insensitive_attributes() {
        let html = r#"<LINK REL="alternate" TYPE="Application/RSS+XML" HREF="/feed.xml">"#;
        let url = parse_url("https://example.com/");
        let result = discover_feed_urls(html, &url);
        assert_eq!(result, vec!["https://example.com/feed.xml"]);
    }

    #[test]
    fn test_attributes_in_any_order() {
        let html = r#"<link href="/feed.xml" type="application/rss+xml" rel="alternate">"#;
        let url = parse_url("https://example.com/");
        let result = discover_feed_urls(html, &url);
        assert_eq!(result, vec!["https://example.com/feed.xml"]);
    }

    #[test]
    fn test_single_quoted_attributes() {
        let html = r#"<link rel='alternate' type='application/rss+xml' href='/feed.xml'>"#;
        let url = parse_url("https://example.com/");
        let result = discover_feed_urls(html, &url);
        assert_eq!(result, vec!["https://example.com/feed.xml"]);
    }

    #[test]
    fn test_data_type_attribute_does_not_confuse_type_match() {
        let html =
            r#"<link data-type="foo" rel="alternate" type="application/rss+xml" href="/feed.xml">"#;
        let url = parse_url("https://example.com/");
        let result = discover_feed_urls(html, &url);
        assert_eq!(result, vec!["https://example.com/feed.xml"]);
    }

    // === Common path fallback ===

    #[test]
    fn test_fallback_when_no_link_tags() {
        let html = r#"<html><head><title>My Blog</title></head></html>"#;
        let url = parse_url("https://example.com/");
        let result = discover_feed_urls(html, &url);
        assert!(!result.is_empty(), "should generate common path candidates");
        assert!(result.contains(&"https://example.com/feed.xml".to_string()));
        assert!(result.contains(&"https://example.com/rss.xml".to_string()));
        assert!(result.contains(&"https://example.com/atom.xml".to_string()));
    }

    #[test]
    fn test_no_fallback_when_link_tags_found() {
        let html = r#"<link rel="alternate" type="application/rss+xml" href="/my-feed.xml">"#;
        let url = parse_url("https://example.com/");
        let result = discover_feed_urls(html, &url);
        assert_eq!(result, vec!["https://example.com/my-feed.xml"]);
        assert!(
            !result.contains(&"https://example.com/feed.xml".to_string()),
            "should not include common path fallback when <link> tags found"
        );
    }

    #[test]
    fn test_fallback_includes_parent_paths() {
        let html = "<html></html>";
        let url = parse_url("https://example.com/blog/2024/some-post");
        let result = discover_feed_urls(html, &url);
        // Should try paths relative to /blog/2024/, /blog/, and /
        assert!(result.contains(&"https://example.com/blog/2024/feed.xml".to_string()));
        assert!(result.contains(&"https://example.com/blog/feed.xml".to_string()));
        assert!(result.contains(&"https://example.com/feed.xml".to_string()));
    }

    #[test]
    fn test_fallback_tries_root_first() {
        let html = "<html></html>";
        let url = parse_url("https://example.com/blog/post");
        let result = discover_feed_urls(html, &url);
        let root_feed = result
            .iter()
            .position(|u| u == "https://example.com/feed.xml")
            .expect("should contain /feed.xml");
        let blog_feed = result
            .iter()
            .position(|u| u == "https://example.com/blog/feed.xml")
            .expect("should contain /blog/feed.xml");
        assert!(
            root_feed < blog_feed,
            "/feed.xml should come before /blog/feed.xml"
        );
    }

    #[test]
    fn test_fallback_no_duplicate_urls() {
        let html = "<html></html>";
        let url = parse_url("https://example.com/blog/post");
        let result = discover_feed_urls(html, &url);
        let mut seen = std::collections::HashSet::new();
        for u in &result {
            assert!(seen.insert(u), "duplicate candidate: {u}");
        }
    }

    #[rstest]
    #[case::root("https://example.com/")]
    #[case::one_deep("https://example.com/blog/")]
    #[case::two_deep("https://example.com/blog/post")]
    #[case::three_deep("https://example.com/blog/2024/post")]
    fn test_fallback_always_includes_root_paths(#[case] page_url: &str) {
        let html = "<html></html>";
        let url = parse_url(page_url);
        let result = discover_feed_urls(html, &url);
        assert!(
            result.contains(&"https://example.com/feed.xml".to_string()),
            "should always include root /feed.xml for {page_url}"
        );
    }

    // === <a> tag feed link discovery ===

    #[test]
    fn test_finds_feed_from_anchor_tag() {
        let html = r#"<html><body><a href="atom/"><img src="/img/rss.png"></a></body></html>"#;
        let url = parse_url("https://example.com/blog/");
        let result = discover_feed_urls(html, &url);
        assert!(
            result.contains(&"https://example.com/blog/atom/".to_string()),
            "should find feed URL from <a> tag with feed-like href, got: {result:?}"
        );
    }

    #[test]
    fn test_finds_feed_from_anchor_with_rss_href() {
        let html = r#"<html><body><a href="/blog/rss/">RSS Feed</a></body></html>"#;
        let url = parse_url("https://example.com/");
        let result = discover_feed_urls(html, &url);
        assert!(
            result.contains(&"https://example.com/blog/rss/".to_string()),
            "should find feed URL from <a> tag with rss in href, got: {result:?}"
        );
    }

    #[test]
    fn test_finds_feed_from_anchor_with_feed_href() {
        let html = r#"<html><body><a href="/blog/feed/">Subscribe</a></body></html>"#;
        let url = parse_url("https://example.com/");
        let result = discover_feed_urls(html, &url);
        assert!(
            result.contains(&"https://example.com/blog/feed/".to_string()),
            "should find feed URL from <a> tag with feed in href, got: {result:?}"
        );
    }

    #[test]
    fn test_anchor_feed_links_prioritized_before_guesses() {
        let html = r#"<html><body><a href="atom/">Feed</a></body></html>"#;
        let url = parse_url("https://example.com/blog/");
        let result = discover_feed_urls(html, &url);
        let anchor_pos = result
            .iter()
            .position(|u| u == "https://example.com/blog/atom/")
            .expect("should contain anchor feed URL");
        let guess_pos = result
            .iter()
            .position(|u| u == "https://example.com/feed.xml")
            .expect("should contain guessed feed URL");
        assert!(
            anchor_pos < guess_pos,
            "anchor-discovered feeds should come before guessed paths"
        );
    }

    #[test]
    fn test_no_duplicate_trailing_slash_variants() {
        let html = "<html></html>";
        let url = parse_url("https://example.com/blog/post");
        let result = discover_feed_urls(html, &url);
        // "atom" and "atom/" at the same path should not both appear
        let root_atom_count = result
            .iter()
            .filter(|u| {
                let trimmed = u.trim_end_matches('/');
                trimmed == "https://example.com/atom"
            })
            .count();
        assert!(
            root_atom_count <= 1,
            "should not have both /atom and /atom/ as separate candidates, got: {result:?}"
        );
        // Same for feed/feed/
        let root_feed_count = result
            .iter()
            .filter(|u| {
                let trimmed = u.trim_end_matches('/');
                trimmed == "https://example.com/feed"
            })
            .count();
        assert!(
            root_feed_count <= 1,
            "should not have both /feed and /feed/ as separate candidates, got: {result:?}"
        );
    }

    #[test]
    fn test_anchor_tags_skipped_when_link_tags_present() {
        let html = r#"<html><head>
            <link rel="alternate" type="application/rss+xml" href="/my-feed.xml">
        </head><body><a href="/atom/">Atom Feed</a></body></html>"#;
        let url = parse_url("https://example.com/");
        let result = discover_feed_urls(html, &url);
        assert_eq!(result, vec!["https://example.com/my-feed.xml"]);
    }

    #[test]
    fn test_anchor_feed_link_with_query_string() {
        let html = r#"<html><body><a href="/feed?format=rss">RSS</a></body></html>"#;
        let url = parse_url("https://example.com/");
        let result = discover_feed_urls(html, &url);
        assert!(
            result.contains(&"https://example.com/feed?format=rss".to_string()),
            "should match feed URL with query string, got: {result:?}"
        );
    }

    #[test]
    fn test_anchor_feed_link_with_fragment() {
        let html = r#"<html><body><a href="/blog/atom#latest">Feed</a></body></html>"#;
        let url = parse_url("https://example.com/");
        let result = discover_feed_urls(html, &url);
        assert!(
            result.contains(&"https://example.com/blog/atom#latest".to_string()),
            "should match feed URL with fragment, got: {result:?}"
        );
    }

    // === Empty / edge cases ===

    #[test]
    fn test_empty_html() {
        let url = parse_url("https://example.com/");
        let result = discover_feed_urls("", &url);
        assert!(
            !result.is_empty(),
            "empty HTML should still produce common path fallback"
        );
    }

    #[test]
    fn test_html_with_no_head() {
        let html = "<html><body><p>Hello</p></body></html>";
        let url = parse_url("https://example.com/");
        let result = discover_feed_urls(html, &url);
        assert!(
            !result.is_empty(),
            "HTML without <head> should fall back to common paths"
        );
    }

    #[test]
    fn test_self_closing_link_tag() {
        let html = r#"<link rel="alternate" type="application/rss+xml" href="/feed.xml" />"#;
        let url = parse_url("https://example.com/");
        let result = discover_feed_urls(html, &url);
        assert_eq!(result, vec!["https://example.com/feed.xml"]);
    }

    #[test]
    fn test_link_tag_with_extra_attributes() {
        let html = r#"<link rel="alternate" type="application/rss+xml" title="My Blog Feed" href="/feed.xml">"#;
        let url = parse_url("https://example.com/");
        let result = discover_feed_urls(html, &url);
        assert_eq!(result, vec!["https://example.com/feed.xml"]);
    }

    #[test]
    fn test_unicode_before_link_tag() {
        // İ (U+0130) lowercases to i + combining dot (2 bytes → 3 bytes),
        // shifting byte indices between html and html.to_lowercase()
        let html = r#"<title>İstanbul Blog</title><link rel="alternate" type="application/rss+xml" href="/feed.xml">"#;
        let url = parse_url("https://example.com/");
        let result = discover_feed_urls(html, &url);
        assert_eq!(result, vec!["https://example.com/feed.xml"]);
    }

    #[test]
    fn test_link_tag_multiline() {
        let html = r#"<link
            rel="alternate"
            type="application/rss+xml"
            href="/feed.xml"
        >"#;
        let url = parse_url("https://example.com/");
        let result = discover_feed_urls(html, &url);
        assert_eq!(result, vec!["https://example.com/feed.xml"]);
    }
}
