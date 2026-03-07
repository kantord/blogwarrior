#!/usr/bin/env bash
#
# Performance benchmark for blogtato.
#
# Prerequisites:
#   uv (https://docs.astral.sh/uv/)
#   hyperfine (cargo install hyperfine, or package manager)
#
# Usage:
#   ./perf/bench.sh
#
set -euo pipefail

PERF_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(cd "$PERF_DIR/.." && pwd)"
FEEDS_FILE="$PERF_DIR/feeds.txt"
RESULTS_DIR="$PERF_DIR/results"

# ── Preflight checks ──────────────────────────────────────────────────
check_command() {
    if ! command -v "$1" &>/dev/null; then
        echo "ERROR: $1 is not installed. $2"
        exit 1
    fi
}

check_command hyperfine "Install with: cargo install hyperfine"
check_command uv "Install from: https://docs.astral.sh/uv/"

# ── Step 1: Validate feeds (if not already done) ─────────────────────
echo "=== Step 1: Feed validation ==="
if [ ! -f "$FEEDS_FILE" ]; then
    echo "Running setup.py to discover and validate feeds..."
    uv run "$PERF_DIR/setup.py"
else
    FEED_COUNT=$(wc -l < "$FEEDS_FILE" | tr -d ' ')
    echo "Using cached feeds.txt ($FEED_COUNT feeds)"
fi

FEED_COUNT=$(wc -l < "$FEEDS_FILE" | tr -d ' ')
if [ "$FEED_COUNT" -eq 0 ]; then
    echo "ERROR: No valid feeds found. Delete perf/feeds.txt and try again."
    exit 1
fi
echo ""

# ── Step 2: Build release binary ─────────────────────────────────────
echo "=== Step 2: Building release binary ==="
cargo build --release --manifest-path "$REPO_DIR/Cargo.toml" 2>&1
BLOG="$REPO_DIR/target/release/blog"
echo "Binary: $BLOG"
echo ""

# ── Step 3: Set up temp store ─────────────────────────────────────────
STORE_DIR=$(mktemp -d)
export RSS_STORE="$STORE_DIR"
echo "=== Step 3: Temp store at $STORE_DIR ==="

cleanup() {
    echo ""
    echo "Cleaning up $STORE_DIR..."
    rm -rf "$STORE_DIR"
}
trap cleanup EXIT

# ── Step 4: Inject feeds directly into store ──────────────────────────
# setup.py already validated these are feed URLs. We inject them as JSONL
# directly into the store instead of running `blog feed add` (which does
# HTTP discovery per URL — slow and not what we're benchmarking).
echo "=== Step 4: Injecting $FEED_COUNT feeds into store ==="
uv run "$PERF_DIR/inject_feeds.py" "$FEEDS_FILE" "$STORE_DIR"
echo ""

# ── Step 5: Benchmark sync (first run — full fetch) ──────────────────
echo "=== Step 5: Benchmark sync (first run, includes network) ==="
mkdir -p "$RESULTS_DIR"
hyperfine \
    --runs 1 \
    --export-json "$RESULTS_DIR/sync_first.json" \
    --show-output \
    "RSS_STORE=$STORE_DIR $BLOG sync"
echo ""

# ── Step 6: Benchmark sync (second run — already up to date) ─────────
echo "=== Step 6: Benchmark sync (no-op, already up to date) ==="
hyperfine \
    --runs 3 \
    --export-json "$RESULTS_DIR/sync_noop.json" \
    "RSS_STORE=$STORE_DIR $BLOG sync"
echo ""

# ── Step 7: Benchmark export (read path) ──────────────────────────────
echo "=== Step 7: Benchmark export ==="
hyperfine \
    --runs 10 \
    --warmup 2 \
    --export-json "$RESULTS_DIR/export.json" \
    "RSS_STORE=$STORE_DIR $BLOG .all export > /dev/null"
echo ""

# ── Step 8: Validate export output ────────────────────────────────────
echo "=== Step 8: Export validation ==="
RSS_STORE=$STORE_DIR "$BLOG" .all export | uv run "$PERF_DIR/validate.py"
VALIDATE_EXIT=$?
echo ""

# ── Step 9: Summary ──────────────────────────────────────────────────
echo "=== Summary ==="
echo ""

for result_file in "$RESULTS_DIR"/*.json; do
    name=$(basename "$result_file" .json)
    if command -v jq &>/dev/null; then
        mean=$(jq -r '.results[0].mean // empty' "$result_file" 2>/dev/null)
        stddev=$(jq -r '.results[0].stddev // empty' "$result_file" 2>/dev/null)
        if [ -n "$mean" ] && [ -n "$stddev" ]; then
            printf "  %-20s  mean=%.3fs  stddev=%.3fs\n" "$name" "$mean" "$stddev"
        elif [ -n "$mean" ]; then
            printf "  %-20s  mean=%.3fs\n" "$name" "$mean"
        else
            printf "  %-20s  (see %s)\n" "$name" "$result_file"
        fi
    else
        printf "  %-20s  (see %s)\n" "$name" "$result_file"
    fi
done

echo ""
echo "Results saved to: $RESULTS_DIR/"

if [ "$VALIDATE_EXIT" -ne 0 ]; then
    echo ""
    echo "VALIDATION FAILED"
    exit 1
fi
