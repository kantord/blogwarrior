#!/bin/sh
link=$(jq -r '.link')
blog .all export 2>/dev/null | jq -c --arg link "$link" 'select(.link == $link) | {title: .title, link: .link, feed_title: .feed.title, date: (.date // "" | if . != "" then .[:10] else "" end)}'
