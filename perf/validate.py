#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""Property-test JSONL export output from `blog .all export`.

Reads JSONL from stdin, validates each line, prints a summary report.
Exits non-zero if any hard failures are found.
"""

import json
import sys
from collections import Counter
from datetime import datetime


def parse_iso_date(s):
    """Try to parse an ISO 8601 datetime string."""
    for fmt in ("%Y-%m-%dT%H:%M:%S%z", "%Y-%m-%dT%H:%M:%SZ"):
        try:
            return datetime.strptime(s, fmt)
        except (ValueError, TypeError):
            continue
    # fallback: fromisoformat handles most variants
    try:
        return datetime.fromisoformat(s)
    except (ValueError, TypeError):
        return None


def validate_line(line_num, obj):
    """Validate a single export item. Returns a list of issue strings."""
    issues = []

    # title
    title = obj.get("title")
    if not title or not isinstance(title, str) or not title.strip():
        issues.append("missing/empty title")

    # link
    link = obj.get("link")
    if not link or not isinstance(link, str) or not link.strip():
        issues.append("missing/empty link")

    # date (optional but should parse if present)
    date = obj.get("date")
    if date is not None:
        if not isinstance(date, str):
            issues.append(f"date is not a string: {type(date).__name__}")
        elif parse_iso_date(date) is None:
            issues.append(f"unparseable date: {date!r}")

    # feed object
    feed = obj.get("feed")
    if not isinstance(feed, dict):
        issues.append("missing/invalid feed object")
    else:
        feed_url = feed.get("url")
        if not feed_url or not isinstance(feed_url, str) or not feed_url.strip():
            issues.append("missing/empty feed.url")

    # read_at should be null (nothing was opened during perf test)
    read_at = obj.get("read_at")
    if read_at is not None:
        issues.append(f"unexpected read_at: {read_at}")

    return issues


def main():
    total = 0
    hard_failures = 0
    issue_counter = Counter()
    feeds_seen = set()
    links_by_feed = {}  # feed_url -> set of links (for duplicate detection)
    items_per_feed = Counter()
    dates_present = 0
    dates_missing = 0

    for i, line in enumerate(sys.stdin, 1):
        line = line.rstrip("\n")
        if not line:
            continue

        # Parse JSON
        try:
            obj = json.loads(line)
        except json.JSONDecodeError as e:
            print(f"  HARD FAIL line {i}: invalid JSON: {e}")
            hard_failures += 1
            continue

        total += 1
        issues = validate_line(i, obj)

        for issue in issues:
            issue_counter[issue] += 1
            if issue.startswith("missing") or issue.startswith("unparseable"):
                pass  # soft failures
            else:
                hard_failures += 1

        # Track feeds
        feed = obj.get("feed", {})
        feed_url = feed.get("url", "")
        if feed_url:
            feeds_seen.add(feed_url)
            items_per_feed[feed_url] += 1

        # Track duplicate links
        link = obj.get("link", "")
        if link and feed_url:
            links_by_feed.setdefault(feed_url, set())
            if link in links_by_feed[feed_url]:
                issue_counter["duplicate link within feed"] += 1
            links_by_feed[feed_url].add(link)

        # Date stats
        if obj.get("date") is not None:
            dates_present += 1
        else:
            dates_missing += 1

    # Summary report
    print("\n=== Export Validation Report ===\n")
    print(f"Total items:      {total}")
    print(f"Feeds seen:       {len(feeds_seen)}")
    print(f"Dates present:    {dates_present} ({100*dates_present/max(total,1):.1f}%)")
    print(f"Dates missing:    {dates_missing} ({100*dates_missing/max(total,1):.1f}%)")

    if items_per_feed:
        counts = sorted(items_per_feed.values())
        print(f"Items per feed:   min={counts[0]}, median={counts[len(counts)//2]}, max={counts[-1]}")

    if issue_counter:
        print(f"\nIssues found:")
        for issue, count in issue_counter.most_common():
            print(f"  {count:>5}x  {issue}")
    else:
        print(f"\nNo issues found.")

    if total == 0:
        print("ERROR: no output from export")
        sys.exit(1)

    print(f"\nHard failures:    {hard_failures}")

    if hard_failures > 0:
        print("\nFAILED")
        sys.exit(1)
    else:
        print("\nPASSED")


if __name__ == "__main__":
    main()
