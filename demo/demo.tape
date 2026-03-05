Output demo/demo.gif

Require blog
Require w3m

Set Shell "bash"
Set FontFamily "JetBrains Mono"
Set FontSize 28
Set Width 1600
Set Height 800
Set Padding 40
Set Margin 20
Set MarginFill "#1e1e2e"
Set BorderRadius 10
Set Theme "Catppuccin Mocha"
Set TypingSpeed 40ms

Hide
Type `export RSS_STORE=$(mktemp -d)` Enter
Sleep 500ms
Type "clear" Enter
Sleep 500ms
Show

Type "blog feed add https://news.ycombinator.com/rss" Sleep 500ms Enter
Sleep 3s

Type "blog feed add https://michael.stapelberg.ch/feed.xml" Sleep 500ms Enter
Sleep 3s

Type "blog feed add https://www.justinmklam.com/index.xml" Sleep 500ms Enter
Sleep 3s

Type "blog sync" Sleep 500ms Enter
Sleep 3s

# Default view: unread posts from the last 3 months, grouped by week
Type "blog" Sleep 500ms Enter
Sleep 5s

# Group by date
Type "blog /d" Sleep 500ms Enter
Sleep 5s

# Group by feed
Type "blog /f" Sleep 500ms Enter
Sleep 5s

# Group by date and feed
Type "blog /d /f" Sleep 500ms Enter
Sleep 5s

# List feeds and their shorthands
Type "blog feed ls" Sleep 500ms Enter
Sleep 3s

# Filter by read status
Type "blog .read" Sleep 500ms Enter
Sleep 3s

# Date range filter
Type "blog since:1w" Sleep 500ms Enter
Sleep 5s

Type "blog 3m..1m" Sleep 500ms Enter
Sleep 5s

# Show only unread posts
Type "blog .unread" Sleep 500ms Enter
Sleep 5s

# Read a post URL with w3m (using shorthand from the list)
Type "w3m $(blog s read)" Sleep 500ms Enter
Sleep 3s

# Scroll down a couple of times
Type "j" Sleep 200ms
Type "j" Sleep 200ms
Type "j" Sleep 200ms
Type "j" Sleep 200ms
Type "j" Sleep 200ms
Type "j"
Sleep 2s

Type "q"
Sleep 500ms
Type "y"
Sleep 1s
