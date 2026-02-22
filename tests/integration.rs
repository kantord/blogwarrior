use std::fs;
use std::io::BufRead;
use std::path::Path;

use assert_cmd::Command;
use httpmock::prelude::*;
use tempfile::TempDir;

fn read_table(dir: &Path) -> Vec<serde_json::Value> {
    let mut items = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(fname) = path.file_name().and_then(|f| f.to_str())
                && fname.starts_with("items_") && fname.ends_with(".jsonl")
            {
                let file = fs::File::open(&path).unwrap();
                for line in std::io::BufReader::new(file).lines() {
                    let line = line.unwrap();
                    if !line.trim().is_empty() {
                        let value: serde_json::Value = serde_json::from_str(&line).unwrap();
                        if value.get("deleted_at").is_none() {
                            items.push(value);
                        }
                    }
                }
            }
        }
    }
    items
}

struct TestContext {
    dir: TempDir,
    server: MockServer,
}

impl TestContext {
    fn new() -> Self {
        Self {
            dir: TempDir::new().unwrap(),
            server: MockServer::start(),
        }
    }

    fn write_feeds(&self, urls: &[&str]) {
        let feeds_dir = self.dir.path().join("feeds");
        if feeds_dir.exists() {
            fs::remove_dir_all(&feeds_dir).unwrap();
        }
        for url in urls {
            self.run(&["feed", "add", url]).success();
        }
    }

    fn read_posts(&self) -> Vec<serde_json::Value> {
        read_table(&self.dir.path().join("posts"))
    }

    fn read_feeds(&self) -> Vec<serde_json::Value> {
        read_table(&self.dir.path().join("feeds"))
    }

    fn run(&self, args: &[&str]) -> assert_cmd::assert::Assert {
        #[allow(deprecated)]
        Command::cargo_bin("blog")
            .unwrap()
            .args(args)
            .env("RSS_STORE", self.dir.path())
            .assert()
    }

    fn mock_rss_feed(&self, path: &str, xml: &str) {
        self.server.mock(|when, then| {
            when.method(GET).path(path);
            then.status(200)
                .header("Content-Type", "application/rss+xml")
                .body(xml);
        });
    }

    fn mock_atom_feed(&self, path: &str, xml: &str) {
        self.server.mock(|when, then| {
            when.method(GET).path(path);
            then.status(200)
                .header("Content-Type", "application/atom+xml")
                .body(xml);
        });
    }
}

fn rss_xml_with_guids(title: &str, items: &[(&str, &str, &str)]) -> String {
    let items_xml: String = items
        .iter()
        .map(|(item_title, date, guid)| {
            format!(
                "<item><title>{}</title><pubDate>{}</pubDate><guid>{}</guid></item>",
                item_title, date, guid
            )
        })
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>{}</title>
    {}
  </channel>
</rss>"#,
        title, items_xml
    )
}

fn rss_xml(title: &str, items: &[(&str, &str)]) -> String {
    let items_xml: String = items
        .iter()
        .enumerate()
        .map(|(i, (item_title, date))| {
            format!(
                "<item><title>{}</title><pubDate>{}</pubDate><guid>urn:item:{}</guid></item>",
                item_title, date, i
            )
        })
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>{}</title>
    {}
  </channel>
</rss>"#,
        title, items_xml
    )
}

fn atom_xml(title: &str, feed_id: &str, entries: &[(&str, &str, &str)]) -> String {
    let entries_xml: String = entries
        .iter()
        .map(|(entry_title, id, date)| {
            format!(
                "<entry><title>{}</title><id>{}</id><updated>{}</updated><published>{}</published></entry>",
                entry_title, id, date, date
            )
        })
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>{}</title>
  <id>{}</id>
  <updated>2024-01-01T00:00:00Z</updated>
  {}
</feed>"#,
        title, feed_id, entries_xml
    )
}

#[test]
fn test_pull_creates_posts_file() {
    let ctx = TestContext::new();
    let xml = rss_xml(
        "Test Blog",
        &[
            ("First Post", "Mon, 01 Jan 2024 00:00:00 +0000"),
            ("Second Post", "Tue, 02 Jan 2024 00:00:00 +0000"),
        ],
    );
    ctx.mock_rss_feed("/feed.xml", &xml);

    let url = ctx.server.url("/feed.xml");
    ctx.write_feeds(&[&url]);

    ctx.run(&["pull"]).success();

    let posts = ctx.read_posts();
    assert_eq!(posts.len(), 2);
    let titles: Vec<&str> = posts.iter().map(|p| p["title"].as_str().unwrap()).collect();
    assert!(titles.contains(&"First Post"));
    assert!(titles.contains(&"Second Post"));
    // feed field should contain the feed's table ID, same for all posts from this feed
    let feed_ids: Vec<&str> = posts.iter().map(|p| p["feed"].as_str().unwrap()).collect();
    assert!(feed_ids.iter().all(|f| !f.is_empty()));
    assert!(feed_ids.iter().all(|f| f == &feed_ids[0]));
}

#[test]
fn test_pull_multiple_feeds() {
    let ctx = TestContext::new();

    let rss = rss_xml(
        "RSS Blog",
        &[("RSS Post", "Mon, 01 Jan 2024 00:00:00 +0000")],
    );
    ctx.mock_rss_feed("/rss.xml", &rss);

    let atom = atom_xml(
        "Atom Blog",
        "urn:atom-blog",
        &[("Atom Post", "urn:atom:1", "2024-01-02T00:00:00Z")],
    );
    ctx.mock_atom_feed("/atom.xml", &atom);

    let rss_url = ctx.server.url("/rss.xml");
    let atom_url = ctx.server.url("/atom.xml");
    ctx.write_feeds(&[&rss_url, &atom_url]);

    ctx.run(&["pull"]).success();

    let posts = ctx.read_posts();
    assert_eq!(posts.len(), 2);

    let titles: Vec<&str> = posts.iter().map(|p| p["title"].as_str().unwrap()).collect();
    assert!(titles.contains(&"RSS Post"));
    assert!(titles.contains(&"Atom Post"));
}

#[test]
fn test_show_displays_posts() {
    let ctx = TestContext::new();

    let posts = r#"{"id":"1","title":"Hello World","date":"2024-01-15T00:00:00Z","feed":"Alice"}
{"id":"2","title":"Second Post","date":"2024-01-14T00:00:00Z","feed":"Bob"}"#;
    fs::create_dir_all(ctx.dir.path().join("posts")).unwrap();
    fs::write(ctx.dir.path().join("posts").join("items_.jsonl"), posts).unwrap();

    let output = ctx.run(&["show"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    assert!(stdout.contains("Hello World"));
    assert!(stdout.contains("Second Post"));
    assert!(stdout.contains("Alice"));
    assert!(stdout.contains("Bob"));
}

#[test]
fn test_show_with_grouping() {
    let ctx = TestContext::new();

    let posts = r#"{"id":"1","title":"Post A","date":"2024-01-15T00:00:00Z","feed":"Alice"}
{"id":"2","title":"Post B","date":"2024-01-15T00:00:00Z","feed":"Bob"}
{"id":"3","title":"Post C","date":"2024-01-14T00:00:00Z","feed":"Alice"}"#;
    fs::create_dir_all(ctx.dir.path().join("posts")).unwrap();
    fs::write(ctx.dir.path().join("posts").join("items_.jsonl"), posts).unwrap();

    let output = ctx.run(&["show", "d"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    assert!(stdout.contains("=== 2024-01-15 ==="));
    assert!(stdout.contains("=== 2024-01-14 ==="));
    assert!(stdout.contains("Post A"));
    assert!(stdout.contains("Post B"));
    assert!(stdout.contains("Post C"));
}

#[test]
fn test_show_default_no_subcommand() {
    let ctx = TestContext::new();

    let posts =
        r#"{"id":"1","title":"Default Show","date":"2024-01-15T00:00:00Z","feed":"Alice"}"#;
    fs::create_dir_all(ctx.dir.path().join("posts")).unwrap();
    fs::write(ctx.dir.path().join("posts").join("items_.jsonl"), posts).unwrap();

    let output = ctx.run(&[]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    assert!(stdout.contains("Default Show"));
    assert!(stdout.contains("Alice"));
}

#[test]
fn test_pull_then_show() {
    let ctx = TestContext::new();
    let xml = rss_xml(
        "Roundtrip Blog",
        &[("Roundtrip Post", "Wed, 03 Jan 2024 00:00:00 +0000")],
    );
    ctx.mock_rss_feed("/feed.xml", &xml);

    let url = ctx.server.url("/feed.xml");
    ctx.write_feeds(&[&url]);

    ctx.run(&["pull"]).success();

    let output = ctx.run(&["show"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    assert!(stdout.contains("Roundtrip Post"));
    assert!(stdout.contains("Roundtrip Blog"));
}

#[test]
fn test_serde_roundtrip() {
    let ctx = TestContext::new();
    let xml = rss_xml(
        "Serde Blog",
        &[
            ("Post One", "Mon, 01 Jan 2024 12:00:00 +0000"),
            ("Post Two", "Tue, 02 Jan 2024 12:00:00 +0000"),
        ],
    );
    ctx.mock_rss_feed("/feed.xml", &xml);

    let url = ctx.server.url("/feed.xml");
    ctx.write_feeds(&[&url]);

    ctx.run(&["pull"]).success();

    let posts = ctx.read_posts();
    assert_eq!(posts.len(), 2);

    // Write back and re-read to verify roundtrip
    let mut out = String::new();
    for post in &posts {
        out.push_str(&serde_json::to_string(post).unwrap());
        out.push('\n');
    }
    // Remove existing shard files and write all to a single shard
    if let Ok(entries) = fs::read_dir(ctx.dir.path().join("posts")) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(fname) = path.file_name().and_then(|f| f.to_str())
                && fname.starts_with("items_") && fname.ends_with(".jsonl")
            {
                fs::remove_file(&path).unwrap();
            }
        }
    }
    fs::write(ctx.dir.path().join("posts").join("items_.jsonl"), &out).unwrap();

    let posts2 = ctx.read_posts();
    assert_eq!(posts, posts2);
}

#[test]
fn test_pull_twice_no_duplicates() {
    let ctx = TestContext::new();

    let xml1 = rss_xml_with_guids(
        "Blog",
        &[
            ("Post A", "Mon, 01 Jan 2024 00:00:00 +0000", "guid-a"),
            ("Post B", "Tue, 02 Jan 2024 00:00:00 +0000", "guid-b"),
        ],
    );
    ctx.mock_rss_feed("/feed.xml", &xml1);

    let url = ctx.server.url("/feed.xml");
    ctx.write_feeds(&[&url]);

    ctx.run(&["pull"]).success();
    let posts1 = ctx.read_posts();
    assert_eq!(posts1.len(), 2);

    // Second pull with overlapping + new item
    let xml2 = rss_xml_with_guids(
        "Blog",
        &[
            ("Post B Updated", "Tue, 02 Jan 2024 00:00:00 +0000", "guid-b"),
            ("Post C", "Wed, 03 Jan 2024 00:00:00 +0000", "guid-c"),
        ],
    );
    ctx.mock_rss_feed("/feed2.xml", &xml2);

    let url2 = ctx.server.url("/feed2.xml");
    ctx.write_feeds(&[&url2]);

    ctx.run(&["pull"]).success();
    let posts2 = ctx.read_posts();

    // Should have 3 items: A (from first pull, preserved), B (updated), C (new)
    assert_eq!(posts2.len(), 3);

    let titles: Vec<&str> = posts2.iter().map(|p| p["title"].as_str().unwrap()).collect();
    assert!(titles.contains(&"Post A"));
    assert!(titles.contains(&"Post B Updated"));
    assert!(titles.contains(&"Post C"));
    // Original "Post B" should be overwritten
    assert!(!titles.contains(&"Post B"));
}

#[test]
fn test_add_creates_feed() {
    let ctx = TestContext::new();

    ctx.run(&["feed", "add", "https://example.com/feed.xml"]).success();

    let feeds = ctx.read_feeds();
    assert_eq!(feeds.len(), 1);
    assert_eq!(feeds[0]["url"].as_str().unwrap(), "https://example.com/feed.xml");
}

#[test]
fn test_add_then_pull() {
    let ctx = TestContext::new();
    let xml = rss_xml(
        "Added Blog",
        &[("Added Post", "Mon, 01 Jan 2024 00:00:00 +0000")],
    );
    ctx.mock_rss_feed("/added.xml", &xml);

    let url = ctx.server.url("/added.xml");
    ctx.run(&["feed", "add", &url]).success();
    ctx.run(&["pull"]).success();

    let posts = ctx.read_posts();
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0]["title"].as_str().unwrap(), "Added Post");
}

#[test]
fn test_pull_continues_after_feed_failure() {
    let ctx = TestContext::new();

    // One feed returns a 500 error
    ctx.server.mock(|when, then| {
        when.method(GET).path("/broken.xml");
        then.status(500).body("Internal Server Error");
    });

    // The other feed works fine
    let xml = rss_xml(
        "Good Blog",
        &[("Good Post", "Mon, 01 Jan 2024 00:00:00 +0000")],
    );
    ctx.mock_rss_feed("/good.xml", &xml);

    let broken_url = ctx.server.url("/broken.xml");
    let good_url = ctx.server.url("/good.xml");
    ctx.write_feeds(&[&broken_url, &good_url]);

    ctx.run(&["pull"]).success();

    let posts = ctx.read_posts();
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0]["title"].as_str().unwrap(), "Good Post");
}

#[test]
fn test_remove_feed() {
    let ctx = TestContext::new();
    let xml = rss_xml(
        "Blog to Remove",
        &[("Post", "Mon, 01 Jan 2024 00:00:00 +0000")],
    );
    ctx.mock_rss_feed("/removable.xml", &xml);

    let url = ctx.server.url("/removable.xml");
    ctx.run(&["feed", "add", &url]).success();
    ctx.run(&["pull"]).success();

    let output = ctx.run(&["show"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("Blog to Remove"));

    ctx.run(&["feed", "rm", &url]).success();

    // Pull should no longer fetch the removed feed
    ctx.run(&["pull"]).success();

    // Feed and its posts should be gone — show should report no posts
    let output = ctx.run(&["show"]).failure();
    let stderr = String::from_utf8(output.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("No matching posts"));
}

#[test]
fn test_remove_feed_deletes_its_posts() {
    let ctx = TestContext::new();

    let xml1 = rss_xml_with_guids(
        "Keep Blog",
        &[("Keep Post", "Mon, 01 Jan 2024 00:00:00 +0000", "guid-keep")],
    );
    ctx.mock_rss_feed("/keep.xml", &xml1);

    let xml2 = rss_xml_with_guids(
        "Remove Blog",
        &[("Remove Post", "Tue, 02 Jan 2024 00:00:00 +0000", "guid-remove")],
    );
    ctx.mock_rss_feed("/remove.xml", &xml2);

    let keep_url = ctx.server.url("/keep.xml");
    let remove_url = ctx.server.url("/remove.xml");
    ctx.write_feeds(&[&keep_url, &remove_url]);
    ctx.run(&["pull"]).success();

    let posts = ctx.read_posts();
    assert_eq!(posts.len(), 2);

    ctx.run(&["feed", "rm", &remove_url]).success();

    let posts = ctx.read_posts();
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0]["title"].as_str().unwrap(), "Keep Post");
}

#[test]
fn test_feed_ls() {
    let ctx = TestContext::new();

    ctx.run(&["feed", "add", "https://example.com/feed1.xml"]).success();
    ctx.run(&["feed", "add", "https://example.com/feed2.xml"]).success();

    let output = ctx.run(&["feed", "ls"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    assert!(stdout.contains("https://example.com/feed1.xml"));
    assert!(stdout.contains("https://example.com/feed2.xml"));

    // Each line should start with a shorthand consisting only of home-row characters
    let home_row_chars: &[char] = &['a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l'];
    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let first_word: String = line.chars().take_while(|c| *c != ' ').collect();
        assert!(first_word.starts_with('@'), "line should start with @shorthand: {}", line);
        let shorthand = &first_word[1..];
        assert!(!shorthand.is_empty(), "shorthand should not be empty: {}", line);
        assert!(
            shorthand.chars().all(|c| home_row_chars.contains(&c)),
            "shorthand '{}' contains non-home-row characters in line: {}",
            shorthand,
            line,
        );
    }
}

#[test]
fn test_feed_ls_no_feeds_prints_error() {
    let ctx = TestContext::new();

    let output = ctx.run(&["feed", "ls"]).failure();
    let stderr = String::from_utf8(output.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("No matching feeds"), "expected 'No matching feeds' on stderr, got: {}", stderr);
}

#[test]
fn test_feed_remove_by_shorthand() {
    let ctx = TestContext::new();

    let xml1 = rss_xml_with_guids(
        "Keep Blog",
        &[("Keep Post", "Mon, 01 Jan 2024 00:00:00 +0000", "guid-keep")],
    );
    ctx.mock_rss_feed("/keep.xml", &xml1);

    let xml2 = rss_xml_with_guids(
        "Remove Blog",
        &[("Remove Post", "Tue, 02 Jan 2024 00:00:00 +0000", "guid-remove")],
    );
    ctx.mock_rss_feed("/remove.xml", &xml2);

    let keep_url = ctx.server.url("/keep.xml");
    let remove_url = ctx.server.url("/remove.xml");
    ctx.write_feeds(&[&keep_url, &remove_url]);
    ctx.run(&["pull"]).success();

    // Run feed ls and parse the shorthand for the remove_url
    let output = ctx.run(&["feed", "ls"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    let shorthand = stdout
        .lines()
        .find(|line| line.contains(&remove_url))
        .map(|line| {
            let first_word: String = line.chars().take_while(|c| *c != ' ').collect();
            first_word // includes the '@' prefix
        })
        .expect("should find remove_url in feed ls output");

    // Remove using the shorthand
    ctx.run(&["feed", "rm", &shorthand]).success();

    let feeds = ctx.read_feeds();
    assert_eq!(feeds.len(), 1);
    assert_eq!(feeds[0]["url"].as_str().unwrap(), keep_url);

    let posts = ctx.read_posts();
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0]["title"].as_str().unwrap(), "Keep Post");
}

#[test]
fn test_show_no_posts_prints_error() {
    let ctx = TestContext::new();

    let output = ctx.run(&["show"]).failure();
    let stderr = String::from_utf8(output.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("No matching posts"), "expected 'No matching posts' on stderr, got: {}", stderr);
}

#[test]
fn test_show_filter_by_shorthand() {
    let ctx = TestContext::new();

    let xml1 = rss_xml_with_guids(
        "Alpha Blog",
        &[
            ("Alpha Post 1", "Mon, 01 Jan 2024 00:00:00 +0000", "guid-a1"),
            ("Alpha Post 2", "Tue, 02 Jan 2024 00:00:00 +0000", "guid-a2"),
        ],
    );
    ctx.mock_rss_feed("/alpha.xml", &xml1);

    let xml2 = rss_xml_with_guids(
        "Beta Blog",
        &[
            ("Beta Post 1", "Wed, 03 Jan 2024 00:00:00 +0000", "guid-b1"),
        ],
    );
    ctx.mock_rss_feed("/beta.xml", &xml2);

    let alpha_url = ctx.server.url("/alpha.xml");
    let beta_url = ctx.server.url("/beta.xml");
    ctx.write_feeds(&[&alpha_url, &beta_url]);
    ctx.run(&["pull"]).success();

    // Get shorthand for alpha feed
    let output = ctx.run(&["feed", "ls"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    let alpha_shorthand = stdout
        .lines()
        .find(|line| line.contains(&alpha_url))
        .map(|line| {
            let first_word: String = line.chars().take_while(|c| *c != ' ').collect();
            first_word
        })
        .expect("should find alpha_url in feed ls output");

    // Filter with `show @shorthand` — should only show alpha posts
    let output = ctx.run(&["show", &alpha_shorthand]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    assert!(stdout.contains("Alpha Post 1"), "should contain Alpha Post 1");
    assert!(stdout.contains("Alpha Post 2"), "should contain Alpha Post 2");
    assert!(!stdout.contains("Beta Post 1"), "should NOT contain Beta Post 1");

    // Also test with no subcommand: `blog @shorthand`
    let output = ctx.run(&[&alpha_shorthand]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    assert!(stdout.contains("Alpha Post 1"), "no-subcommand: should contain Alpha Post 1");
    assert!(stdout.contains("Alpha Post 2"), "no-subcommand: should contain Alpha Post 2");
    assert!(!stdout.contains("Beta Post 1"), "no-subcommand: should NOT contain Beta Post 1");
}

#[test]
fn test_show_filter_unknown_shorthand() {
    let ctx = TestContext::new();

    ctx.run(&["feed", "add", "https://example.com/feed.xml"]).success();

    let output = ctx.run(&["show", "@zzz"]).failure();
    let stderr = String::from_utf8(output.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("Unknown shorthand"), "expected unknown shorthand error, got: {}", stderr);
}

#[test]
fn test_remove_then_readd_feed() {
    let ctx = TestContext::new();
    let xml = rss_xml(
        "Returning Blog",
        &[("Old Post", "Mon, 01 Jan 2024 00:00:00 +0000")],
    );
    ctx.mock_rss_feed("/returning.xml", &xml);

    let url = ctx.server.url("/returning.xml");
    ctx.run(&["feed", "add", &url]).success();
    ctx.run(&["pull"]).success();
    ctx.run(&["feed", "rm", &url]).success();

    // Re-add and pull again
    ctx.run(&["feed", "add", &url]).success();
    ctx.run(&["pull"]).success();

    let output = ctx.run(&["show"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("Returning Blog"));
    assert!(stdout.contains("Old Post"));
}
