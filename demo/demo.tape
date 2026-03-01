Output demo/demo.gif

Require blog

Set Shell "bash"
Set FontSize 16
Set Width 900
Set Height 500
Set Padding 20
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
Sleep 8s

Type "blog show" Sleep 500ms Enter
Sleep 5s
