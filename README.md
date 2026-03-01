# blogtato

A simple RSS/Atom feed reader for the terminal.

![demo](demo/demo.gif)

## Install

```bash
cargo install --path .
```

## Usage

```bash
# Subscribe to a feed
blog feed add https://news.ycombinator.com/rss

# Fetch new posts
blog sync

# Read posts
blog show

# Group by date or feed
blog show d
blog show f

# Filter by feed shorthand
blog show @hn

# Open a post in the browser
blog open abc

# List subscriptions
blog feed ls

# Remove a feed
blog feed rm https://news.ycombinator.com/rss
```

## Git sync

blogtato can sync your feed database across machines using git:

```bash
# Clone an existing database
blog clone user/repo

# After that, `blog sync` fetches feeds and pushes/pulls from the remote
blog sync
```
