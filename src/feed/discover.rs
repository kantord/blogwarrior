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
    "rss",
    "feed.rss",
    "feed.atom",
];

/// Discover feed URLs from an HTML page.
///
/// Returns candidate feed URLs in priority order:
/// 1. URLs from `<link rel="alternate">` tags with feed MIME types
/// 2. Common feed paths relative to the page URL's parent directories and root
pub fn discover_feed_urls(html: &str, page_url: &url::Url) -> Vec<String> {
    let urls = find_link_tags(html, page_url);
    if !urls.is_empty() {
        return urls;
    }
    guess_common_paths(page_url)
}

fn find_link_tags(html: &str, page_url: &url::Url) -> Vec<String> {
    let lower = html.to_lowercase();
    let mut urls = Vec::new();
    let mut search_from = 0;

    while let Some(start) = lower[search_from..].find("<link").map(|i| i + search_from) {
        let tag_start = start;
        let Some(end) = lower[tag_start..].find('>').map(|i| i + tag_start + 1) else {
            break;
        };
        search_from = end;

        let tag_lower = &lower[tag_start..end];

        let rel = extract_attr(tag_lower, "rel");
        let link_type = extract_attr(tag_lower, "type");
        let href = extract_attr(tag_lower, "href");

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
    }

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
                if seen.insert(s.clone()) {
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
