#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# dependencies = [
#     "listparser>=0.19",
#     "feedparser>=6.0",
#     "requests>=2.28",
# ]
# ///
"""Download OPML files, validate feeds, write surviving URLs to feeds.txt."""

import sys
from pathlib import Path

import feedparser
import listparser
import requests

PERF_DIR = Path(__file__).parent
SOURCES_FILE = PERF_DIR / "sources.txt"
FEEDS_FILE = PERF_DIR / "feeds.txt"
TIMEOUT = 10
MAX_FEEDS_PER_OPML = 50  # cap per source to keep setup time reasonable


def load_sources():
    lines = SOURCES_FILE.read_text().splitlines()
    return [l.strip() for l in lines if l.strip() and not l.strip().startswith("#")]


def fetch_opml_feeds(url):
    """Parse an OPML URL and return a list of feed URLs."""
    try:
        r = requests.get(url, timeout=30, headers={"User-Agent": "blogtato-perf/1.0"})
        r.raise_for_status()
        result = listparser.parse(r.content)
        urls = [f.url for f in result.feeds if f.url]
        return urls[:MAX_FEEDS_PER_OPML]
    except Exception as e:
        print(f"  SKIP opml {url}: {e}", file=sys.stderr)
        return []


def is_valid_feed(url):
    """Check if a URL returns a valid RSS/Atom feed."""
    try:
        r = requests.get(url, timeout=TIMEOUT, headers={"User-Agent": "blogtato-perf/1.0"})
        if r.status_code != 200:
            return False
        d = feedparser.parse(r.content)
        # bozo means parse error; also skip feeds with no entries
        return not d.bozo and len(d.entries) > 0
    except Exception:
        return False


def main():
    if FEEDS_FILE.exists():
        existing = [l for l in FEEDS_FILE.read_text().splitlines() if l.strip()]
        print(f"feeds.txt already exists with {len(existing)} feeds. Delete it to re-validate.")
        return

    sources = load_sources()
    print(f"Loaded {len(sources)} OPML sources")

    all_urls = []
    for src in sources:
        print(f"Fetching OPML: {src}")
        urls = fetch_opml_feeds(src)
        print(f"  Found {len(urls)} feed URLs")
        all_urls.extend(urls)

    # Deduplicate
    all_urls = list(dict.fromkeys(all_urls))
    print(f"\n{len(all_urls)} unique feed URLs to validate")

    valid = []
    for i, url in enumerate(all_urls, 1):
        status = "OK" if is_valid_feed(url) else "DEAD"
        tag = "+" if status == "OK" else "-"
        print(f"  [{i}/{len(all_urls)}] {tag} {url}")
        if status == "OK":
            valid.append(url)

    FEEDS_FILE.write_text("\n".join(valid) + "\n")
    print(f"\nWrote {len(valid)}/{len(all_urls)} valid feeds to {FEEDS_FILE}")


if __name__ == "__main__":
    main()
