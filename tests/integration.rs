use std::fs;
use std::io::BufRead;

use assert_cmd::Command;
use httpmock::prelude::*;
use tempfile::TempDir;

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
        let lines: Vec<String> = urls
            .iter()
            .map(|u| format!(r#"{{"url":"{}"}}"#, u))
            .collect();
        fs::write(self.dir.path().join("feeds.jsonl"), lines.join("\n")).unwrap();
    }

    fn read_posts(&self) -> Vec<serde_json::Value> {
        let posts_dir = self.dir.path().join("posts");
        let mut items = Vec::new();
        if let Ok(entries) = fs::read_dir(&posts_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(fname) = path.file_name().and_then(|f| f.to_str()) {
                    if fname.starts_with("items_") && fname.ends_with(".jsonl") {
                        let file = fs::File::open(&path).unwrap();
                        for line in std::io::BufReader::new(file).lines() {
                            let line = line.unwrap();
                            if !line.trim().is_empty() {
                                items.push(serde_json::from_str(&line).unwrap());
                            }
                        }
                    }
                }
            }
        }
        items
    }

    fn run(&self, args: &[&str]) -> assert_cmd::assert::Assert {
        #[allow(deprecated)]
        Command::cargo_bin("rss-reader")
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
    assert!(posts.iter().all(|p| p["author"] == "Test Blog"));
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

    let posts = r#"{"id":"1","source_id":"src","title":"Hello World","date":"2024-01-15T00:00:00Z","author":"Alice"}
{"id":"2","source_id":"src","title":"Second Post","date":"2024-01-14T00:00:00Z","author":"Bob"}"#;
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

    let posts = r#"{"id":"1","source_id":"src","title":"Post A","date":"2024-01-15T00:00:00Z","author":"Alice"}
{"id":"2","source_id":"src","title":"Post B","date":"2024-01-15T00:00:00Z","author":"Bob"}
{"id":"3","source_id":"src","title":"Post C","date":"2024-01-14T00:00:00Z","author":"Alice"}"#;
    fs::create_dir_all(ctx.dir.path().join("posts")).unwrap();
    fs::write(ctx.dir.path().join("posts").join("items_.jsonl"), posts).unwrap();

    let output = ctx.run(&["show", "-g", "d"]).success();
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
        r#"{"id":"1","source_id":"src","title":"Default Show","date":"2024-01-15T00:00:00Z","author":"Alice"}"#;
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
            if let Some(fname) = path.file_name().and_then(|f| f.to_str()) {
                if fname.starts_with("items_") && fname.ends_with(".jsonl") {
                    fs::remove_file(&path).unwrap();
                }
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
