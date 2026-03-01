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

Type "blog sync" Sleep 500ms Enter
Sleep 3s

Type "blog" Sleep 500ms Enter
Sleep 3s

Type "blog open a" Sleep 500ms Enter
Sleep 3s

Type "w3m $(blog read a)" Sleep 500ms Enter
Sleep 5s

Type "q"
Sleep 1s

Type "blog feed add https://michael.stapelberg.ch/feed.xml" Sleep 500ms Enter
Sleep 3s

Type "blog feed add https://www.justinmklam.com/index.xml" Sleep 500ms Enter
Sleep 3s

Type "blog sync" Sleep 500ms Enter
Sleep 3s

Type "blog" Sleep 500ms Enter
Sleep 5s
