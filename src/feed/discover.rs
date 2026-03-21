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

/// Collects unique URLs, treating trailing-slash variants as duplicates.
struct UrlDedup {
    urls: Vec<String>,
    seen: std::collections::HashSet<String>,
}

impl UrlDedup {
    fn new() -> Self {
        Self {
            urls: Vec::new(),
            seen: std::collections::HashSet::new(),
        }
    }

    fn try_insert(&mut self, url: String) -> bool {
        let key = url.trim_end_matches('/').to_string();
        if self.seen.insert(key) {
            self.urls.push(url);
            true
        } else {
            false
        }
    }

    fn into_urls(self) -> Vec<String> {
        self.urls
    }
}

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
    let mut dedup = UrlDedup::new();
    for u in find_anchor_feed_links(html, page_url) {
        dedup.try_insert(u);
    }
    for u in guess_common_paths(page_url) {
        dedup.try_insert(u);
    }
    dedup.into_urls()
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
    let mut dedup = UrlDedup::new();

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
            dedup.try_insert(absolute.to_string());
        }
    });

    dedup.into_urls()
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

    let mut dedup = UrlDedup::new();

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
                dedup.try_insert(candidate.to_string());
            }
        }
    }

    dedup.into_urls()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn parse_url(s: &str) -> url::Url {
        url::Url::parse(s).unwrap()
    }

    // === <link rel="alternate"> tag parsing ===

    #[rstest]
    #[case::rss("application/rss+xml", "/feed.xml", "https://example.com/feed.xml")]
    #[case::atom("application/atom+xml", "/atom.xml", "https://example.com/atom.xml")]
    #[case::json("application/feed+json", "/feed.json", "https://example.com/feed.json")]
    fn test_finds_link_tag_by_type(#[case] mime: &str, #[case] href: &str, #[case] expected: &str) {
        let html = format!(r#"<link rel="alternate" type="{mime}" href="{href}">"#);
        let url = parse_url("https://example.com/");
        let result = discover_feed_urls(&html, &url);
        assert_eq!(result, vec![expected]);
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

    #[rstest]
    #[case::relative(
        "https://example.com/blog/",
        "feed.xml",
        "https://example.com/blog/feed.xml"
    )]
    #[case::absolute(
        "https://example.com/blog/post",
        "https://example.com/feed.xml",
        "https://example.com/feed.xml"
    )]
    #[case::root_relative(
        "https://example.com/blog/post",
        "/feed.xml",
        "https://example.com/feed.xml"
    )]
    fn test_resolves_href(#[case] page_url: &str, #[case] href: &str, #[case] expected: &str) {
        let html = format!(r#"<link rel="alternate" type="application/rss+xml" href="{href}">"#);
        let url = parse_url(page_url);
        let result = discover_feed_urls(&html, &url);
        assert_eq!(result, vec![expected]);
    }

    #[rstest]
    #[case::non_feed_type(r#"<link rel="alternate" type="text/html" href="/page">"#, "/page")]
    #[case::non_alternate_rel(
        r#"<link rel="stylesheet" type="application/rss+xml" href="/style.css">"#,
        "/style.css"
    )]
    fn test_ignores_invalid_link_tags(#[case] html: &str, #[case] excluded_path: &str) {
        let url = parse_url("https://example.com/");
        let result = discover_feed_urls(html, &url);
        let excluded = format!("https://example.com{excluded_path}");
        assert!(
            !result.contains(&excluded),
            "should not include {excluded_path}"
        );
    }

    #[rstest]
    #[case::no_href(r#"<link rel="alternate" type="application/rss+xml">"#)]
    #[case::no_type(r#"<link rel="alternate" href="/feed.xml">"#)]
    fn test_incomplete_link_falls_back(#[case] html: &str) {
        let url = parse_url("https://example.com/");
        let result = discover_feed_urls(html, &url);
        assert!(
            result.contains(&"https://example.com/rss.xml".to_string()),
            "should fall back to common paths, got: {result:?}"
        );
    }

    #[rstest]
    #[case::uppercase(r#"<LINK REL="alternate" TYPE="Application/RSS+XML" HREF="/feed.xml">"#)]
    #[case::reordered(r#"<link href="/feed.xml" type="application/rss+xml" rel="alternate">"#)]
    #[case::single_quoted(r#"<link rel='alternate' type='application/rss+xml' href='/feed.xml'>"#)]
    #[case::data_type_attr(
        r#"<link data-type="foo" rel="alternate" type="application/rss+xml" href="/feed.xml">"#
    )]
    #[case::self_closing(r#"<link rel="alternate" type="application/rss+xml" href="/feed.xml" />"#)]
    #[case::extra_attrs(r#"<link rel="alternate" type="application/rss+xml" title="My Blog Feed" href="/feed.xml">"#)]
    #[case::multiline(
        "<link\n            rel=\"alternate\"\n            type=\"application/rss+xml\"\n            href=\"/feed.xml\"\n        >"
    )]
    fn test_link_tag_variations(#[case] html: &str) {
        let url = parse_url("https://example.com/");
        let result = discover_feed_urls(html, &url);
        assert_eq!(result, vec!["https://example.com/feed.xml"]);
    }

    // === Common path fallback ===

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

    #[rstest]
    #[case::atom_relative(
        "https://example.com/blog/",
        r#"<a href="atom/"><img src="/img/rss.png"></a>"#,
        "https://example.com/blog/atom/"
    )]
    #[case::rss_absolute(
        "https://example.com/",
        r#"<a href="/blog/rss/">RSS Feed</a>"#,
        "https://example.com/blog/rss/"
    )]
    #[case::feed_absolute(
        "https://example.com/",
        r#"<a href="/blog/feed/">Subscribe</a>"#,
        "https://example.com/blog/feed/"
    )]
    #[case::query_string(
        "https://example.com/",
        r#"<a href="/feed?format=rss">RSS</a>"#,
        "https://example.com/feed?format=rss"
    )]
    #[case::fragment(
        "https://example.com/",
        r#"<a href="/blog/atom#latest">Feed</a>"#,
        "https://example.com/blog/atom#latest"
    )]
    fn test_finds_feed_from_anchor(
        #[case] page_url: &str,
        #[case] anchor: &str,
        #[case] expected: &str,
    ) {
        let html = format!("<html><body>{anchor}</body></html>");
        let url = parse_url(page_url);
        let result = discover_feed_urls(&html, &url);
        assert!(
            result.contains(&expected.to_string()),
            "should find {expected} in results, got: {result:?}"
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

    #[rstest]
    #[case::atom("https://example.com/atom")]
    #[case::feed("https://example.com/feed")]
    fn test_no_duplicate_trailing_slash_variants(#[case] base_url: &str) {
        let html = "<html></html>";
        let url = parse_url("https://example.com/blog/post");
        let result = discover_feed_urls(html, &url);
        let count = result
            .iter()
            .filter(|u| u.trim_end_matches('/') == base_url)
            .count();
        assert!(
            count <= 1,
            "should not have both {base_url} and {base_url}/ as separate candidates, got: {result:?}"
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

    // === Empty / edge cases ===

    #[rstest]
    #[case::empty("")]
    #[case::no_head("<html><body><p>Hello</p></body></html>")]
    fn test_fallback_on_minimal_html(#[case] html: &str) {
        let url = parse_url("https://example.com/");
        let result = discover_feed_urls(html, &url);
        assert!(
            !result.is_empty(),
            "should produce common path fallback for: {html:?}"
        );
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
}
