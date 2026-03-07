Output demo/demo.gif

Require blog

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

Type "blog feed add https://michael.stapelberg.ch" Sleep 500ms Enter
Sleep 3s

Type "blog feed add https://www.justinmklam.com" Sleep 500ms Enter
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
Type "blog 1w.." Sleep 500ms Enter
Sleep 5s

Type "blog 3m..1m" Sleep 500ms Enter
Sleep 5s

# Show only unread posts
Type "blog .unread" Sleep 500ms Enter
Sleep 5s

# Filter by feed shorthand
Type "blog @s" Sleep 500ms Enter
Sleep 5s
