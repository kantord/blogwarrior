#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""Inject validated feed URLs directly into a blogtato store as JSONL.

Replicates synctato's ID hashing scheme (truncated SHA-256) so the store
can read the data as if `blog feed add` had been used.

Usage: uv run perf/inject_feeds.py <feeds.txt> <store_dir>
"""

import hashlib
import json
import math
import os
import sys
from datetime import datetime, timezone
from pathlib import Path

EXPECTED_CAPACITY = 50_000  # must match FeedSource::EXPECTED_CAPACITY
SHARD_CHARACTERS = 0        # must match FeedSource::SHARD_CHARACTERS
TABLE_NAME = "feeds"


def id_length_for_capacity(expected_items):
    if expected_items <= 1:
        return 4
    k = float(expected_items)
    n = math.log(500.0 * k * k) / math.log(16.0)
    return max(math.ceil(n), 4)


def hash_id(raw, id_length):
    h = hashlib.sha256(raw.encode()).hexdigest()
    return h[:id_length]


def main():
    if len(sys.argv) != 3:
        print(f"Usage: {sys.argv[0]} <feeds.txt> <store_dir>", file=sys.stderr)
        sys.exit(1)

    feeds_file = Path(sys.argv[1])
    store_dir = Path(sys.argv[2])

    urls = [l.strip() for l in feeds_file.read_text().splitlines() if l.strip()]
    if not urls:
        print("No feeds to inject", file=sys.stderr)
        sys.exit(1)

    id_len = id_length_for_capacity(EXPECTED_CAPACITY)
    table_dir = store_dir / TABLE_NAME
    table_dir.mkdir(parents=True, exist_ok=True)

    now = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.%fZ")
    rows = []
    for url in urls:
        row_id = hash_id(url, id_len)
        row = {
            "id": row_id,
            "url": url,
            "title": "",
            "site_url": "",
            "description": "",
            "updated_at": now,
        }
        rows.append(json.dumps(row, separators=(",", ":")))

    # SHARD_CHARACTERS=0 means single file: items_.jsonl
    out_path = table_dir / "items_.jsonl"
    out_path.write_text("\n".join(rows) + "\n")

    print(f"Injected {len(urls)} feeds into {out_path}")


if __name__ == "__main__":
    main()
