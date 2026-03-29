#!/bin/sh
LOCKFILE=/tmp/blogtato-sync.lock
if [ -f "$LOCKFILE" ]; then
    # Already syncing, just show current posts
    exec blog .unread 2w.. export 2>/dev/null | jq -c '{title: .title, link: .link, feed_title: .feed.title, date: (.date // "" | if . != "" then .[:10] else "" end)}'
fi
trap 'rm -f "$LOCKFILE"' EXIT
touch "$LOCKFILE"
blog sync 2>/dev/null
blog .unread 2w.. export 2>/dev/null | jq -c '{title: .title, link: .link, feed_title: .feed.title, date: (.date // "" | if . != "" then .[:10] else "" end)}'
