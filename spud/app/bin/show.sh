#!/bin/sh
blog .unread 2w.. export 2>/dev/null | jq -c '{id: .id, title: .title, link: .link, feed_title: .feed.title, date: (.date // "" | if . != "" then .[:10] else "" end)}'
