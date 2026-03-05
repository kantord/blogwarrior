# blogtato

A CLI RSS/Atom feed reader inspired by Taskwarrior.

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

# Show posts (defaults to unread posts from the last 3 months, grouped by week)
blog

# Group by date, week, or feed
blog /d
blog /w
blog /f

# Combine groupings
blog /d /f

# Filter by feed shorthand
blog @hn

# Filter by read status
blog .unread
blog .read
blog .all

# Filter by date
blog since:1w
blog 3m..1m
blog /d since:2w until:1w

# Combine filters
blog @hn .unread /d

# Open a post in the default browser
blog abc open

# Print a post URL (useful with CLI browsers)
blog abc read
w3m $(blog abc read)

# Mark a post as unread
blog abc unread

# Export matching posts as JSONL
blog .all export
blog @myblog export
blog since:1w export

# List subscriptions
blog feed ls

# Remove a feed
blog feed rm https://news.ycombinator.com/rss
blog feed rm @hn
```

## Git sync

### Why sync using git

blogtato is built around the idea of subscription detox. You shouldn't need to
create yet another account or pay for yet another service just to read some
blogs. If you're comfortable with the command line, you almost certainly
already have access to a git host - GitHub, GitLab, a self-hosted Forgejo -
where you can store a private repo for free.

Is git the ideal database for an RSS reader? No - but it is a pragmatic one for
these design goals. Your feeds and posts live as simple JSONL files in a repo,
and when two machines diverge, blogtato merges them automatically based on
timestamps. You never have to resolve conflicts or touch git yourself -
`blog sync` handles everything. There's no additional server to run, no account
to create, no continuous network dependency.

Even though `git` was not design to store databases, to anyone who uses
`blogtato` as intended (a personal RSS client) the repo stays small and syncs
are fast. It is not scalable to large archives at not technically optimal - but
the trad-off is worth it.

### How to enable `git` based sync

To set it up, create a private repo on your git host, then:

```bash
# On your first machine
blog clone user/repo

# From now on, sync fetches feeds and pushes/pulls from the remote
blog sync
```

On another machine, run the same `blog clone` to pull down your feeds and
posts.
