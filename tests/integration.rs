use std::fs;
use std::io::BufRead;
use std::io::Write;
use std::path::Path;

use sha2::{Digest, Sha256};

use assert_cmd::Command;
use httpmock::prelude::*;
use tempfile::TempDir;

fn read_table(dir: &Path) -> Vec<serde_json::Value> {
    let mut items = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(fname) = path.file_name().and_then(|f| f.to_str())
                && fname.starts_with("items_")
                && fname.ends_with(".jsonl")
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
            insert_feed(self.dir.path(), url);
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

    fn mock_rss_feed_bytes(&self, path: &str, body: &[u8]) {
        self.server.mock(|when, then| {
            when.method(GET).path(path);
            then.status(200)
                .header("Content-Type", "application/rss+xml")
                .body(body);
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

fn rss_xml_with_links(title: &str, items: &[(&str, &str, &str, &str)]) -> String {
    let items_xml: String = items
        .iter()
        .map(|(item_title, date, guid, link)| {
            format!(
                "<item><title>{}</title><pubDate>{}</pubDate><guid>{}</guid><link>{}</link></item>",
                item_title, date, guid, link
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
fn test_sync_creates_posts_file() {
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

    ctx.run(&["sync"]).success();

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
fn test_sync_multiple_feeds() {
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

    ctx.run(&["sync"]).success();

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
fn test_show_rejects_multiple_grouping_args() {
    let ctx = TestContext::new();

    let posts = r#"{"id":"1","title":"Post A","date":"2024-01-15T00:00:00Z","feed":"Alice"}"#;
    fs::create_dir_all(ctx.dir.path().join("posts")).unwrap();
    fs::write(ctx.dir.path().join("posts").join("items_.jsonl"), posts).unwrap();

    let output = ctx.run(&["show", "d", "f"]).failure();
    let stderr = String::from_utf8(output.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("Multiple grouping arguments"),
        "expected error about multiple grouping args, got: {}",
        stderr,
    );
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

    let posts = r#"{"id":"1","title":"Default Show","date":"2024-01-15T00:00:00Z","feed":"Alice"}"#;
    fs::create_dir_all(ctx.dir.path().join("posts")).unwrap();
    fs::write(ctx.dir.path().join("posts").join("items_.jsonl"), posts).unwrap();

    let output = ctx.run(&[]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    assert!(stdout.contains("Default Show"));
    assert!(stdout.contains("Alice"));
}

#[test]
fn test_sync_then_show() {
    let ctx = TestContext::new();
    let xml = rss_xml(
        "Roundtrip Blog",
        &[("Roundtrip Post", "Wed, 03 Jan 2024 00:00:00 +0000")],
    );
    ctx.mock_rss_feed("/feed.xml", &xml);

    let url = ctx.server.url("/feed.xml");
    ctx.write_feeds(&[&url]);

    ctx.run(&["sync"]).success();

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

    ctx.run(&["sync"]).success();

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
                && fname.starts_with("items_")
                && fname.ends_with(".jsonl")
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
fn test_sync_twice_no_duplicates() {
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

    ctx.run(&["sync"]).success();
    let posts1 = ctx.read_posts();
    assert_eq!(posts1.len(), 2);

    // Second pull with overlapping + new item
    let xml2 = rss_xml_with_guids(
        "Blog",
        &[
            (
                "Post B Updated",
                "Tue, 02 Jan 2024 00:00:00 +0000",
                "guid-b",
            ),
            ("Post C", "Wed, 03 Jan 2024 00:00:00 +0000", "guid-c"),
        ],
    );
    ctx.mock_rss_feed("/feed2.xml", &xml2);

    let url2 = ctx.server.url("/feed2.xml");
    ctx.write_feeds(&[&url2]);

    ctx.run(&["sync"]).success();
    let posts2 = ctx.read_posts();

    // Should have 3 items: A (from first pull, preserved), B (updated), C (new)
    assert_eq!(posts2.len(), 3);

    let titles: Vec<&str> = posts2
        .iter()
        .map(|p| p["title"].as_str().unwrap())
        .collect();
    assert!(titles.contains(&"Post A"));
    assert!(titles.contains(&"Post B Updated"));
    assert!(titles.contains(&"Post C"));
    // Original "Post B" should be overwritten
    assert!(!titles.contains(&"Post B"));
}

#[test]
fn test_add_creates_feed() {
    let ctx = TestContext::new();
    let xml = rss_xml("Test Feed", &[]);
    ctx.mock_rss_feed("/feed.xml", &xml);

    let url = ctx.server.url("/feed.xml");
    ctx.run(&["feed", "add", &url]).success();

    let feeds = ctx.read_feeds();
    assert_eq!(feeds.len(), 1);
    assert_eq!(feeds[0]["url"].as_str().unwrap(), url);
}

#[test]
fn test_add_then_sync() {
    let ctx = TestContext::new();
    let xml = rss_xml(
        "Added Blog",
        &[("Added Post", "Mon, 01 Jan 2024 00:00:00 +0000")],
    );
    ctx.mock_rss_feed("/added.xml", &xml);

    let url = ctx.server.url("/added.xml");
    ctx.run(&["feed", "add", &url]).success();
    ctx.run(&["sync"]).success();

    let posts = ctx.read_posts();
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0]["title"].as_str().unwrap(), "Added Post");
}

#[test]
fn test_sync_continues_after_feed_failure() {
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

    let output = ctx.run(&["sync"]).success();
    let stderr = String::from_utf8(output.get_output().stderr.clone()).unwrap();

    // Error message should mention the HTTP status, not a confusing XML parse error
    assert!(
        stderr.contains("500"),
        "error should mention HTTP 500 status, got: {}",
        stderr
    );

    let posts = ctx.read_posts();
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0]["title"].as_str().unwrap(), "Good Post");
}

#[test]
fn test_sync_reports_http_404_clearly() {
    let ctx = TestContext::new();

    ctx.server.mock(|when, then| {
        when.method(GET).path("/gone.xml");
        then.status(404).body("Not Found");
    });

    let xml = rss_xml(
        "Good Blog",
        &[("Good Post", "Mon, 01 Jan 2024 00:00:00 +0000")],
    );
    ctx.mock_rss_feed("/good.xml", &xml);

    let gone_url = ctx.server.url("/gone.xml");
    let good_url = ctx.server.url("/good.xml");
    ctx.write_feeds(&[&gone_url, &good_url]);

    let output = ctx.run(&["sync"]).success();
    let stderr = String::from_utf8(output.get_output().stderr.clone()).unwrap();

    assert!(
        stderr.contains("404"),
        "error should mention HTTP 404 status, got: {}",
        stderr
    );

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
    ctx.run(&["sync"]).success();

    let output = ctx.run(&["show"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("Blog to Remove"));

    ctx.run(&["feed", "rm", &url]).success();

    // Pull should no longer fetch the removed feed
    ctx.run(&["sync"]).success();

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
        &[(
            "Remove Post",
            "Tue, 02 Jan 2024 00:00:00 +0000",
            "guid-remove",
        )],
    );
    ctx.mock_rss_feed("/remove.xml", &xml2);

    let keep_url = ctx.server.url("/keep.xml");
    let remove_url = ctx.server.url("/remove.xml");
    ctx.write_feeds(&[&keep_url, &remove_url]);
    ctx.run(&["sync"]).success();

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

    insert_feed(ctx.dir.path(), "https://example.com/feed1.xml");
    insert_feed(ctx.dir.path(), "https://example.com/feed2.xml");

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
        assert!(
            first_word.starts_with('@'),
            "line should start with @shorthand: {}",
            line
        );
        let shorthand = &first_word[1..];
        assert!(
            !shorthand.is_empty(),
            "shorthand should not be empty: {}",
            line
        );
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
    assert!(
        stderr.contains("No matching feeds"),
        "expected 'No matching feeds' on stderr, got: {}",
        stderr
    );
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
        &[(
            "Remove Post",
            "Tue, 02 Jan 2024 00:00:00 +0000",
            "guid-remove",
        )],
    );
    ctx.mock_rss_feed("/remove.xml", &xml2);

    let keep_url = ctx.server.url("/keep.xml");
    let remove_url = ctx.server.url("/remove.xml");
    ctx.write_feeds(&[&keep_url, &remove_url]);
    ctx.run(&["sync"]).success();

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
    assert!(
        stderr.contains("No matching posts"),
        "expected 'No matching posts' on stderr, got: {}",
        stderr
    );
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
        &[("Beta Post 1", "Wed, 03 Jan 2024 00:00:00 +0000", "guid-b1")],
    );
    ctx.mock_rss_feed("/beta.xml", &xml2);

    let alpha_url = ctx.server.url("/alpha.xml");
    let beta_url = ctx.server.url("/beta.xml");
    ctx.write_feeds(&[&alpha_url, &beta_url]);
    ctx.run(&["sync"]).success();

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

    assert!(
        stdout.contains("Alpha Post 1"),
        "should contain Alpha Post 1"
    );
    assert!(
        stdout.contains("Alpha Post 2"),
        "should contain Alpha Post 2"
    );
    assert!(
        !stdout.contains("Beta Post 1"),
        "should NOT contain Beta Post 1"
    );

    // Also test with no subcommand: `blog @shorthand`
    let output = ctx.run(&[&alpha_shorthand]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    assert!(
        stdout.contains("Alpha Post 1"),
        "no-subcommand: should contain Alpha Post 1"
    );
    assert!(
        stdout.contains("Alpha Post 2"),
        "no-subcommand: should contain Alpha Post 2"
    );
    assert!(
        !stdout.contains("Beta Post 1"),
        "no-subcommand: should NOT contain Beta Post 1"
    );
}

#[test]
fn test_show_filter_unknown_shorthand() {
    let ctx = TestContext::new();

    insert_feed(ctx.dir.path(), "https://example.com/feed.xml");

    let output = ctx.run(&["show", "@zzz"]).failure();
    let stderr = String::from_utf8(output.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("Unknown feed shorthand"),
        "expected unknown shorthand error, got: {}",
        stderr
    );
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
    ctx.run(&["sync"]).success();
    ctx.run(&["feed", "rm", &url]).success();

    // Re-add and pull again
    ctx.run(&["feed", "add", &url]).success();
    ctx.run(&["sync"]).success();

    let output = ctx.run(&["show"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("Returning Blog"));
    assert!(stdout.contains("Old Post"));
}

#[test]
fn test_show_displays_post_shorthands() {
    let ctx = TestContext::new();

    let xml = rss_xml_with_guids(
        "Shorthand Blog",
        &[
            (
                "Post Alpha",
                "Mon, 01 Jan 2024 00:00:00 +0000",
                "guid-alpha",
            ),
            ("Post Beta", "Tue, 02 Jan 2024 00:00:00 +0000", "guid-beta"),
        ],
    );
    ctx.mock_rss_feed("/shorthand.xml", &xml);

    let url = ctx.server.url("/shorthand.xml");
    ctx.write_feeds(&[&url]);
    ctx.run(&["sync"]).success();

    let output = ctx.run(&["show"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    let post_alphabet: &[char] = &[
        'a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l', 'A', 'S', 'D', 'F', 'G', 'H', 'J', 'K', 'L',
        'q', 'w', 'e', 'r', 't', 'y', 'i', 'o', 'p', 'z', 'x', 'c', 'v', 'b', 'n', 'm',
    ];

    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }
        // Each post line should start with a shorthand of POST_ALPHABET characters
        let first_word: String = line.chars().take_while(|c| !c.is_whitespace()).collect();
        assert!(
            !first_word.is_empty(),
            "line should start with a shorthand: {}",
            line
        );
        assert!(
            first_word.chars().all(|c| post_alphabet.contains(&c)),
            "shorthand '{}' should only contain POST_ALPHABET characters in line: {}",
            first_word,
            line,
        );
    }
}

#[test]
fn test_open_unknown_shorthand() {
    let ctx = TestContext::new();

    let output = ctx.run(&["open", "zzzzz"]).failure();
    let stderr = String::from_utf8(output.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("Unknown shorthand"),
        "expected 'Unknown shorthand' on stderr, got: {}",
        stderr,
    );
}

#[test]
fn test_open_valid_shorthand() {
    let ctx = TestContext::new();

    let xml = rss_xml_with_links(
        "Open Blog",
        &[(
            "Open Post",
            "Mon, 01 Jan 2024 00:00:00 +0000",
            "guid-open",
            "https://example.com/post/1",
        )],
    );
    ctx.mock_rss_feed("/open.xml", &xml);

    let url = ctx.server.url("/open.xml");
    ctx.write_feeds(&[&url]);
    ctx.run(&["sync"]).success();

    // Running `open a` should resolve the shorthand without error.
    // We can't verify the browser actually opens in CI, but we verify
    // it doesn't fail with "Unknown shorthand" or "Post has no link".
    let output = ctx.run(&["open", "a"]);
    let stderr = String::from_utf8(output.get_output().stderr.clone()).unwrap();
    assert!(
        !stderr.contains("Unknown shorthand"),
        "should resolve shorthand 'a', got: {}",
        stderr,
    );
    assert!(
        !stderr.contains("Post has no link"),
        "post should have a link, got: {}",
        stderr,
    );
}

#[test]
fn test_open_post_without_link() {
    let ctx = TestContext::new();

    let posts = r#"{"id":"1","title":"No Link Post","date":"2024-01-15T00:00:00Z","feed":"Alice","raw_id":"no-link-1","link":""}"#;
    std::fs::create_dir_all(ctx.dir.path().join("posts")).unwrap();
    std::fs::write(ctx.dir.path().join("posts").join("items_.jsonl"), posts).unwrap();

    let output = ctx.run(&["open", "a"]).failure();
    let stderr = String::from_utf8(output.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("Post has no link"),
        "expected 'Post has no link' on stderr, got: {}",
        stderr,
    );
}

#[test]
fn test_remove_nonexistent_feed() {
    let ctx = TestContext::new();

    insert_feed(ctx.dir.path(), "https://example.com/keep.xml");

    let output = ctx
        .run(&["feed", "rm", "https://example.com/nonexistent.xml"])
        .failure();
    let stderr = String::from_utf8(output.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("Feed not found"),
        "expected 'Feed not found' on stderr, got: {}",
        stderr
    );

    // The existing feed should still be there
    let feeds = ctx.read_feeds();
    assert_eq!(feeds.len(), 1);
    assert_eq!(
        feeds[0]["url"].as_str().unwrap(),
        "https://example.com/keep.xml"
    );
}

#[test]
fn test_sync_continues_after_non_utf8_feed() {
    let ctx = TestContext::new();

    // Build an RSS feed with a non-UTF8 byte (0xe9 for Latin-1 'é') in the title
    let non_utf8_body: Vec<u8> = [
        b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
          <rss version=\"2.0\">\n\
            <channel>\n\
              <title>Caf"
            .as_slice(),
        &[0xe9],
        b"</title>\n\
              <item>\n\
                <title>Post</title>\n\
                <guid>urn:bad:1</guid>\n\
              </item>\n\
            </channel>\n\
          </rss>"
            .as_slice(),
    ]
    .concat();

    ctx.mock_rss_feed_bytes("/bad.xml", &non_utf8_body);

    // A second feed that is valid
    let good_xml = rss_xml(
        "Good Blog",
        &[("Good Post", "Mon, 01 Jan 2024 00:00:00 +0000")],
    );
    ctx.mock_rss_feed("/good.xml", &good_xml);

    let bad_url = ctx.server.url("/bad.xml");
    let good_url = ctx.server.url("/good.xml");
    ctx.write_feeds(&[&bad_url, &good_url]);

    // Pull should succeed overall — the bad feed errors but doesn't crash
    ctx.run(&["sync"]).success();

    // At minimum the good feed's post should be present
    let posts = ctx.read_posts();
    assert!(
        posts
            .iter()
            .any(|p| p["title"].as_str() == Some("Good Post")),
        "good feed's post should be present"
    );
}

#[test]
fn test_atom_feed_with_rss_in_content_is_parsed_as_atom() {
    let ctx = TestContext::new();

    // An Atom feed whose entry summary uses CDATA containing the literal "<rss" —
    // the naive `text.contains("<rss")` check misidentifies this as RSS.
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>Atom Blog</title>
  <id>urn:atom-blog</id>
  <updated>2024-01-01T00:00:00Z</updated>
  <entry>
    <title>How to migrate to Atom</title>
    <id>urn:post:1</id>
    <updated>2024-01-01T00:00:00Z</updated>
    <published>2024-01-01T00:00:00Z</published>
    <summary type="html"><![CDATA[Replace <rss version="2.0"> with Atom]]></summary>
  </entry>
</feed>"#;
    ctx.mock_atom_feed("/atom-with-rss-mention.xml", xml);

    let url = ctx.server.url("/atom-with-rss-mention.xml");
    ctx.write_feeds(&[&url]);

    ctx.run(&["sync"]).success();

    let posts = ctx.read_posts();
    assert_eq!(posts.len(), 1);
    assert_eq!(
        posts[0]["title"].as_str().unwrap(),
        "How to migrate to Atom"
    );
}

// --- Sync integration tests ---

/// Helper to run a git command in a directory.
fn git(dir: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("failed to run git");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Write a feed URL directly to the store, bypassing the CLI.
/// Also commits to git if the store directory is a git repository.
fn insert_feed(store_dir: &Path, url: &str) {
    let feeds_dir = store_dir.join("feeds");
    fs::create_dir_all(&feeds_dir).unwrap();
    let file_path = feeds_dir.join("items_.jsonl");
    // Replicate synctato's hash_id: SHA-256 of the key, first 11 hex chars
    // (id_length_for_capacity(50_000) == 11 for FeedSource)
    let hash = format!("{:x}", Sha256::digest(url.as_bytes()));
    let id = &hash[..11];
    let entry = serde_json::json!({
        "id": id,
        "url": url,
        "title": "",
        "site_url": "",
        "description": ""
    });
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file_path)
        .unwrap();
    writeln!(file, "{}", entry).unwrap();

    let git_check = std::process::Command::new("git")
        .args(["-C", &store_dir.to_string_lossy(), "rev-parse", "--git-dir"])
        .output();
    if let Ok(output) = git_check {
        if output.status.success() {
            git(store_dir, &["add", "feeds/"]);
            git(store_dir, &["commit", "-m", &format!("add feed: {url}")]);
        }
    }
}

/// Initialize a git repo, configure user, add remote, and make initial commit.
fn init_git_store(store_dir: &Path, origin_dir: &Path) {
    git(store_dir, &["init"]);
    git(store_dir, &["config", "user.name", "Test"]);
    git(store_dir, &["config", "user.email", "test@test.com"]);
    git(
        store_dir,
        &[
            "remote",
            "add",
            "origin",
            &format!("file://{}", origin_dir.display()),
        ],
    );
    // Make an initial commit so we have HEAD
    fs::write(store_dir.join(".keep"), "").unwrap();
    git(store_dir, &["add", "."]);
    git(store_dir, &["commit", "-m", "init"]);
    git(store_dir, &["push", "-u", "origin", "HEAD"]);
}

/// Clone from a bare origin into a new temp dir, return its path.
fn clone_store(origin_dir: &Path) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let output = std::process::Command::new("git")
        .args([
            "clone",
            &format!("file://{}", origin_dir.display()),
            &dir.path().to_string_lossy(),
        ])
        .output()
        .expect("failed to clone");
    assert!(
        output.status.success(),
        "clone failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    git(dir.path(), &["config", "user.name", "Test"]);
    git(dir.path(), &["config", "user.email", "test@test.com"]);
    let p = dir.path().to_path_buf();
    (dir, p)
}

fn run_blog(store_dir: &Path, args: &[&str]) -> assert_cmd::assert::Assert {
    #[allow(deprecated)]
    Command::cargo_bin("blog")
        .unwrap()
        .args(args)
        .env("RSS_STORE", store_dir)
        .assert()
}

#[test]
fn test_sync_no_remote_warns() {
    let dir = TempDir::new().unwrap();
    git(dir.path(), &["init"]);
    git(dir.path(), &["config", "user.name", "Test"]);
    git(dir.path(), &["config", "user.email", "test@test.com"]);

    let output = run_blog(dir.path(), &["sync"]).success();
    let stderr = String::from_utf8(output.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("no remote"),
        "expected 'no remote' warning, got: {}",
        stderr
    );
}

#[test]
fn test_sync_dirty_repo_fails() {
    let origin_dir = TempDir::new().unwrap();
    git(origin_dir.path(), &["init", "--bare"]);

    let store_dir = TempDir::new().unwrap();
    init_git_store(store_dir.path(), origin_dir.path());

    // Make it dirty with a data file (use a dir the store doesn't read)
    fs::create_dir_all(store_dir.path().join("extra")).unwrap();
    fs::write(store_dir.path().join("extra/items_00.jsonl"), "dirty").unwrap();

    let output = run_blog(store_dir.path(), &["sync"]).failure();
    let stderr = String::from_utf8(output.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("uncommitted"),
        "expected 'uncommitted' error, got: {}",
        stderr
    );
}

#[test]
fn test_sync_first_push() {
    let origin_dir = TempDir::new().unwrap();
    git(origin_dir.path(), &["init", "--bare"]);

    let store_dir = TempDir::new().unwrap();
    // Init repo but don't push yet (no remote branch)
    git(store_dir.path(), &["init"]);
    git(store_dir.path(), &["config", "user.name", "Test"]);
    git(store_dir.path(), &["config", "user.email", "test@test.com"]);
    git(
        store_dir.path(),
        &[
            "remote",
            "add",
            "origin",
            &format!("file://{}", origin_dir.path().display()),
        ],
    );

    // Add a feed (insert_feed auto-commits since git repo exists)
    insert_feed(store_dir.path(), "https://example.com/feed.xml");

    // Sync should push
    run_blog(store_dir.path(), &["sync"]).success();

    // Verify we can clone and see the data
    let (clone_td, clone_dir) = clone_store(origin_dir.path());
    let feeds = read_table(&clone_dir.join("feeds"));
    assert_eq!(feeds.len(), 1);
    assert_eq!(
        feeds[0]["url"].as_str().unwrap(),
        "https://example.com/feed.xml"
    );
    drop(clone_td);
}

#[test]
fn test_sync_local_ahead_only() {
    let origin_dir = TempDir::new().unwrap();
    git(origin_dir.path(), &["init", "--bare"]);

    let store_dir = TempDir::new().unwrap();
    init_git_store(store_dir.path(), origin_dir.path());

    // Add feed locally (insert_feed auto-commits)
    insert_feed(store_dir.path(), "https://example.com/a.xml");

    // Sync should push
    run_blog(store_dir.path(), &["sync"]).success();

    // Verify
    let (clone_td, clone_dir) = clone_store(origin_dir.path());
    let feeds = read_table(&clone_dir.join("feeds"));
    assert!(
        feeds
            .iter()
            .any(|f| f["url"].as_str() == Some("https://example.com/a.xml")),
        "remote should have the feed after sync"
    );
    drop(clone_td);
}

#[test]
fn test_sync_remote_ahead_only() {
    let origin_dir = TempDir::new().unwrap();
    git(origin_dir.path(), &["init", "--bare"]);

    let store_dir = TempDir::new().unwrap();
    init_git_store(store_dir.path(), origin_dir.path());

    // Create a second clone, add feed there (auto-committed), push
    let (other_td, other_dir) = clone_store(origin_dir.path());
    insert_feed(&other_dir, "https://example.com/remote.xml");
    git(&other_dir, &["push", "origin", "HEAD"]);
    drop(other_td);

    // Local sync should merge the remote feed
    run_blog(store_dir.path(), &["sync"]).success();

    let feeds = read_table(&store_dir.path().join("feeds"));
    assert!(
        feeds
            .iter()
            .any(|f| f["url"].as_str() == Some("https://example.com/remote.xml")),
        "local should have remote feed after sync"
    );
}

#[test]
fn test_sync_both_diverged() {
    let origin_dir = TempDir::new().unwrap();
    git(origin_dir.path(), &["init", "--bare"]);

    let store_dir = TempDir::new().unwrap();
    init_git_store(store_dir.path(), origin_dir.path());

    // Add feed on remote side (auto-committed), push
    let (other_td, other_dir) = clone_store(origin_dir.path());
    insert_feed(&other_dir, "https://example.com/b.xml");
    git(&other_dir, &["push", "origin", "HEAD"]);
    drop(other_td);

    // Add feed on local side (diverged, auto-committed)
    insert_feed(store_dir.path(), "https://example.com/a.xml");

    // Sync merges both
    run_blog(store_dir.path(), &["sync"]).success();

    let feeds = read_table(&store_dir.path().join("feeds"));
    let urls: Vec<&str> = feeds.iter().filter_map(|f| f["url"].as_str()).collect();
    assert!(
        urls.contains(&"https://example.com/a.xml"),
        "should have local feed A"
    );
    assert!(
        urls.contains(&"https://example.com/b.xml"),
        "should have remote feed B"
    );
}

#[test]
fn test_sync_two_way() {
    let origin_dir = TempDir::new().unwrap();
    git(origin_dir.path(), &["init", "--bare"]);

    // Clone 1
    let store1 = TempDir::new().unwrap();
    init_git_store(store1.path(), origin_dir.path());

    // Clone 2
    let (store2_td, store2_dir) = clone_store(origin_dir.path());

    // Clone 1 adds feed A (auto-committed) and syncs
    insert_feed(store1.path(), "https://example.com/a.xml");
    run_blog(store1.path(), &["sync"]).success();

    // Clone 2 adds feed B (auto-committed) and syncs
    insert_feed(&store2_dir, "https://example.com/b.xml");
    run_blog(&store2_dir, &["sync"]).success();

    // Clone 1 syncs again to pick up B
    run_blog(store1.path(), &["sync"]).success();

    // Both should have A and B
    let feeds1 = read_table(&store1.path().join("feeds"));
    let urls1: Vec<&str> = feeds1.iter().filter_map(|f| f["url"].as_str()).collect();
    assert!(urls1.contains(&"https://example.com/a.xml"));
    assert!(urls1.contains(&"https://example.com/b.xml"));

    let feeds2 = read_table(&store2_dir.join("feeds"));
    let urls2: Vec<&str> = feeds2.iter().filter_map(|f| f["url"].as_str()).collect();
    assert!(urls2.contains(&"https://example.com/a.xml"));
    assert!(urls2.contains(&"https://example.com/b.xml"));

    drop(store2_td);
}

#[test]
fn test_git_passthrough() {
    let dir = TempDir::new().unwrap();

    let output = run_blog(dir.path(), &["git", "init"]).success();
    let _ = output;

    // Now git status should work
    run_blog(dir.path(), &["git", "status"]).success();
}

#[test]
fn test_git_remote_add() {
    let dir = TempDir::new().unwrap();
    run_blog(dir.path(), &["git", "init"]).success();
    run_blog(
        dir.path(),
        &[
            "git",
            "remote",
            "add",
            "origin",
            "https://example.com/repo.git",
        ],
    )
    .success();

    // Verify remote was added
    let output = std::process::Command::new("git")
        .args([
            "-C",
            &dir.path().to_string_lossy(),
            "remote",
            "get-url",
            "origin",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let url = String::from_utf8_lossy(&output.stdout);
    assert!(url.trim() == "https://example.com/repo.git");
}

#[test]
fn test_transact_auto_commits_with_existing_repo() {
    let dir = TempDir::new().unwrap();
    let server = MockServer::start();
    let xml = rss_xml("Test Feed", &[]);
    server.mock(|when, then| {
        when.method(GET).path("/feed.xml");
        then.status(200)
            .header("Content-Type", "application/rss+xml")
            .body(&xml);
    });
    let url = server.url("/feed.xml");

    git(dir.path(), &["init"]);
    git(dir.path(), &["config", "user.name", "Test"]);
    git(dir.path(), &["config", "user.email", "test@test.com"]);
    // Initial commit so HEAD exists
    fs::write(dir.path().join(".keep"), "").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-m", "init"]);

    // Add a feed — should auto-commit because git repo exists
    run_blog(dir.path(), &["feed", "add", &url]).success();

    // Check that a commit was made
    let output = std::process::Command::new("git")
        .args([
            "-C",
            &dir.path().to_string_lossy(),
            "log",
            "--oneline",
            "-1",
        ])
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&output.stdout);
    assert!(
        log.contains("add feed"),
        "commit message should contain 'add feed', got: {}",
        log
    );
}

#[test]
fn test_transact_no_git_repo_still_works() {
    let dir = TempDir::new().unwrap();
    let server = MockServer::start();
    let xml = rss_xml("Test Feed", &[]);
    server.mock(|when, then| {
        when.method(GET).path("/feed.xml");
        then.status(200)
            .header("Content-Type", "application/rss+xml")
            .body(&xml);
    });
    let url = server.url("/feed.xml");

    // No git init — just add a feed
    run_blog(dir.path(), &["feed", "add", &url]).success();

    // Feed should be saved
    let feeds = read_table(&dir.path().join("feeds"));
    assert_eq!(feeds.len(), 1);
}

#[test]
fn test_transact_dirty_repo_fails() {
    let dir = TempDir::new().unwrap();
    let server = MockServer::start();
    let xml = rss_xml("Test Feed", &[]);
    server.mock(|when, then| {
        when.method(GET).path("/feed.xml");
        then.status(200)
            .header("Content-Type", "application/rss+xml")
            .body(&xml);
    });
    let url = server.url("/feed.xml");

    git(dir.path(), &["init"]);
    git(dir.path(), &["config", "user.name", "Test"]);
    git(dir.path(), &["config", "user.email", "test@test.com"]);
    fs::write(dir.path().join(".keep"), "").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-m", "init"]);

    // Make dirty with a data file (use a dir the store doesn't read)
    fs::create_dir_all(dir.path().join("extra")).unwrap();
    fs::write(dir.path().join("extra/items_00.jsonl"), "dirty").unwrap();

    let output = run_blog(dir.path(), &["feed", "add", &url]).failure();
    let stderr = String::from_utf8(output.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("uncommitted"),
        "expected 'uncommitted' error, got: {}",
        stderr
    );
}

fn commit_count(dir: &Path) -> usize {
    let output = std::process::Command::new("git")
        .args(["-C", &dir.to_string_lossy(), "rev-list", "--count", "HEAD"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .unwrap()
}

#[test]
fn test_sync_already_in_sync_creates_no_commits() {
    let origin_dir = TempDir::new().unwrap();
    git(origin_dir.path(), &["init", "--bare"]);

    let store_dir = TempDir::new().unwrap();
    init_git_store(store_dir.path(), origin_dir.path());

    // Add a feed and sync
    insert_feed(store_dir.path(), "https://example.com/feed.xml");
    run_blog(store_dir.path(), &["sync"]).success();

    let commits_before = commit_count(store_dir.path());

    // Sync again — nothing changed, should not create any new commits
    run_blog(store_dir.path(), &["sync"]).success();

    let commits_after = commit_count(store_dir.path());
    assert_eq!(
        commits_before, commits_after,
        "sync with no changes should not create new commits"
    );
}

#[test]
fn test_sync_no_git_repo() {
    // sync should work with no git repo at all (pure feed pulling)
    let dir = TempDir::new().unwrap();
    insert_feed(dir.path(), "https://example.com/feed.xml");
    run_blog(dir.path(), &["sync"]).success();

    let feeds = read_table(&dir.path().join("feeds"));
    assert_eq!(feeds.len(), 1);
}

#[test]
fn test_sync_local_ahead_pushes_without_merge() {
    let origin_dir = TempDir::new().unwrap();
    git(origin_dir.path(), &["init", "--bare"]);

    let store_dir = TempDir::new().unwrap();
    init_git_store(store_dir.path(), origin_dir.path());

    // Add two feeds locally (auto-committed each time)
    insert_feed(store_dir.path(), "https://example.com/a.xml");
    insert_feed(store_dir.path(), "https://example.com/b.xml");

    // Sync should just push (no merge commit)
    run_blog(store_dir.path(), &["sync"]).success();

    // Verify no merge commits (all commits should have at most 1 parent)
    let output = std::process::Command::new("git")
        .args([
            "-C",
            &store_dir.path().to_string_lossy(),
            "log",
            "--format=%P",
        ])
        .output()
        .unwrap();
    let parents = String::from_utf8_lossy(&output.stdout);
    for line in parents.lines() {
        let parent_count = line.split_whitespace().count();
        assert!(
            parent_count <= 1,
            "expected no merge commits, but found a commit with {} parents",
            parent_count
        );
    }
}

#[test]
fn test_pull_command_removed() {
    let dir = TempDir::new().unwrap();
    run_blog(dir.path(), &["pull"]).failure();
}

#[test]
fn test_add_direct_feed_url_still_works() {
    let ctx = TestContext::new();
    let xml = rss_xml(
        "Direct Feed",
        &[("Post One", "Mon, 01 Jan 2024 00:00:00 +0000")],
    );
    ctx.mock_rss_feed("/feed.xml", &xml);

    let url = ctx.server.url("/feed.xml");
    ctx.run(&["feed", "add", &url]).success();

    let feeds = ctx.read_feeds();
    assert_eq!(feeds.len(), 1);
    assert_eq!(feeds[0]["url"].as_str().unwrap(), url);
}

#[test]
fn test_add_html_page_discovers_feed() {
    let ctx = TestContext::new();

    let feed_xml = rss_xml(
        "Discovered Feed",
        &[("Discovered Post", "Mon, 01 Jan 2024 00:00:00 +0000")],
    );
    ctx.mock_rss_feed("/feed.xml", &feed_xml);

    let feed_url = ctx.server.url("/feed.xml");
    let html = format!(
        r#"<html><head>
        <link rel="alternate" type="application/rss+xml" href="{feed_url}" title="My Blog Feed">
        </head><body><p>Hello</p></body></html>"#
    );
    ctx.server.mock(|when, then| {
        when.method(GET).path("/blog");
        then.status(200)
            .header("Content-Type", "text/html")
            .body(&html);
    });

    let blog_url = ctx.server.url("/blog");
    ctx.run(&["feed", "add", &blog_url]).success();

    let feeds = ctx.read_feeds();
    assert_eq!(feeds.len(), 1);
    assert_eq!(feeds[0]["url"].as_str().unwrap(), feed_url);
}

#[test]
fn test_add_html_page_multiple_feeds_fails() {
    let ctx = TestContext::new();

    let feed1_url = ctx.server.url("/feed1.xml");
    let feed2_url = ctx.server.url("/feed2.xml");
    let html = format!(
        r#"<html><head>
        <link rel="alternate" type="application/rss+xml" href="{feed1_url}" title="Feed 1">
        <link rel="alternate" type="application/atom+xml" href="{feed2_url}" title="Feed 2">
        </head><body></body></html>"#
    );
    ctx.server.mock(|when, then| {
        when.method(GET).path("/blog");
        then.status(200)
            .header("Content-Type", "text/html")
            .body(&html);
    });

    let blog_url = ctx.server.url("/blog");
    ctx.run(&["feed", "add", &blog_url]).failure();

    let feeds = ctx.read_feeds();
    assert_eq!(feeds.len(), 0);
}

#[test]
fn test_add_html_page_no_feeds_fails() {
    let ctx = TestContext::new();

    let html = r#"<html><head><title>No Feeds</title></head><body><p>Hello</p></body></html>"#;
    ctx.server.mock(|when, then| {
        when.method(GET).path("/blog");
        then.status(200)
            .header("Content-Type", "text/html")
            .body(html);
    });

    let blog_url = ctx.server.url("/blog");
    ctx.run(&["feed", "add", &blog_url]).failure();

    let feeds = ctx.read_feeds();
    assert_eq!(feeds.len(), 0);
}

#[test]
fn test_add_html_page_ignores_non_feed_candidates() {
    let ctx = TestContext::new();

    // Real feed linked only via <a> tag (no <link rel="alternate">)
    let feed_xml = rss_xml(
        "Real Feed",
        &[("A Post", "Mon, 01 Jan 2024 00:00:00 +0000")],
    );
    ctx.mock_rss_feed("/feed.xml", &feed_xml);

    // Non-feed URL with "feed" in the path — feedfinder picks it up from <a> tags
    ctx.server.mock(|when, then| {
        when.method(GET).path("/buzzfeed");
        then.status(200)
            .header("Content-Type", "text/html")
            .body("<html><body>not a feed</body></html>");
    });

    // No <link> tags — feedfinder falls through to <a> tag scanning,
    // which matches both URLs because they contain "feed" in the href
    let feed_url = ctx.server.url("/feed.xml");
    let not_a_feed_url = ctx.server.url("/buzzfeed");
    let html = format!(
        r#"<html><head><title>Blog</title></head><body>
        <a href="{feed_url}">RSS feed</a>
        <a href="{not_a_feed_url}">BuzzFeed article</a>
        </body></html>"#
    );
    ctx.server.mock(|when, then| {
        when.method(GET).path("/blog");
        then.status(200)
            .header("Content-Type", "text/html")
            .body(&html);
    });

    let blog_url = ctx.server.url("/blog");
    ctx.run(&["feed", "add", &blog_url]).success();

    let feeds = ctx.read_feeds();
    assert_eq!(feeds.len(), 1);
    assert_eq!(feeds[0]["url"].as_str().unwrap(), feed_url);
}

#[test]
fn test_add_html_page_deduplicates_feed_candidates() {
    let ctx = TestContext::new();

    let feed_xml = rss_xml("My Feed", &[("A Post", "Mon, 01 Jan 2024 00:00:00 +0000")]);
    ctx.mock_rss_feed("/index.xml", &feed_xml);

    // Same feed URL appears in multiple <a> tags (e.g., header + footer)
    // No <link> tags, so feedfinder falls through to <a> scanning
    let feed_url = ctx.server.url("/index.xml");
    let html = format!(
        r#"<html><head><title>Blog</title></head><body>
        <nav><a href="{feed_url}">RSS</a></nav>
        <article><p>Content</p></article>
        <footer><a href="{feed_url}">RSS feed</a></footer>
        </body></html>"#
    );
    ctx.server.mock(|when, then| {
        when.method(GET).path("/blog");
        then.status(200)
            .header("Content-Type", "text/html")
            .body(&html);
    });

    let blog_url = ctx.server.url("/blog");
    ctx.run(&["feed", "add", &blog_url]).success();

    let feeds = ctx.read_feeds();
    assert_eq!(feeds.len(), 1);
    assert_eq!(feeds[0]["url"].as_str().unwrap(), feed_url);
}

// --- clone command tests ---

#[test]
fn test_clone_into_empty_dir() {
    // Set up a bare origin repo with some feed data
    let origin_dir = TempDir::new().unwrap();
    std::process::Command::new("git")
        .args(["init", "--bare"])
        .arg(origin_dir.path())
        .output()
        .unwrap();

    // Create a temporary working repo, add data, push to origin
    let work_dir = TempDir::new().unwrap();
    std::process::Command::new("git")
        .args(["clone", &format!("file://{}", origin_dir.path().display())])
        .arg(work_dir.path())
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args([
            "-C",
            &work_dir.path().to_string_lossy(),
            "config",
            "user.name",
            "Test",
        ])
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args([
            "-C",
            &work_dir.path().to_string_lossy(),
            "config",
            "user.email",
            "t@t.com",
        ])
        .output()
        .unwrap();

    insert_feed(work_dir.path(), "https://example.com/feed.xml");

    std::process::Command::new("git")
        .args(["-C", &work_dir.path().to_string_lossy(), "add", "."])
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args([
            "-C",
            &work_dir.path().to_string_lossy(),
            "commit",
            "-m",
            "add feed",
        ])
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args([
            "-C",
            &work_dir.path().to_string_lossy(),
            "push",
            "origin",
            "HEAD",
        ])
        .output()
        .unwrap();

    // Clone into a fresh store dir using the blog clone command
    let store_dir = TempDir::new().unwrap();
    let target = store_dir.path().join("store");

    #[allow(deprecated)]
    Command::cargo_bin("blog")
        .unwrap()
        .args(["clone", &format!("file://{}", origin_dir.path().display())])
        .env("RSS_STORE", &target)
        .assert()
        .success();

    // Verify feed data is present
    let feeds = read_table(&target.join("feeds"));
    assert_eq!(feeds.len(), 1);
    assert_eq!(
        feeds[0]["url"].as_str().unwrap(),
        "https://example.com/feed.xml"
    );
}

#[test]
fn test_clone_existing_store_fails() {
    let store_dir = TempDir::new().unwrap();
    insert_feed(store_dir.path(), "https://example.com/feed.xml");

    #[allow(deprecated)]
    let output = Command::cargo_bin("blog")
        .unwrap()
        .args(["clone", "https://example.com/repo.git"])
        .env("RSS_STORE", store_dir.path())
        .assert()
        .failure();

    let stderr = String::from_utf8_lossy(&output.get_output().stderr);
    assert!(
        stderr.contains("already exists"),
        "error should mention existing database: {stderr}"
    );
    assert!(
        stderr.contains(&store_dir.path().display().to_string()),
        "error should include the store path: {stderr}"
    );
}
