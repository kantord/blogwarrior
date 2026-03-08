# blogtato

A CLI RSS/Atom feed reader inspired by Taskwarrior.

![demo](demo/demo.gif)

## Features

- Subscribe to RSS and Atom feeds
- Simple query language for filtering by feed, read status, and date, with
  grouping and export
- Git-based sync across machines with conflict-free merge
  ([why git?](#design-philosophy))
- No accounts, no servers, no continuous network dependency
- Mark content as read
- Designed to be distraction free, minimalistic and work out of the box

## Install

```bash
cargo install blogtato
```

### Git sync

`git` based synchronization is entire optional. `blogtato` can work entirely
offline on a single device.

To set up git synchronization, create a private repo on your git host, then:

```bash
# On your first machine
blog clone user/repo

# From now on, sync fetches feeds and pushes/pulls from the remote
# with no remote repository, `blog sync` just pulls the latest posts from
# all feeds
blog sync
```

On your device(s), run the same `blog clone` to pull down your feeds and posts.

Don't worry about setting git sync up if you are just trying `blogtato` out:
you can run `blog clone user/repo` later and your existing feeds will be merged
with the remote automatically.

### Quick start

Once you set up your `git`-based sync, or if you decided to skip it, subscribe
to your favorite feeds using `blog feed add`:

```bash
blog feed add https://michael.stapelberg.ch
blog feed add https://www.justinmklam.com
```

Fetch and list latest posts:

```bash
blog sync
blog
```

Read whatever you found interesting by referring to its shorthand

```bash
blog df read
```

You can subscribe to `blogtato` releases to know when new features or fixes are
available:

```bash
blog feed add https://github.com/kantord/blogtato/releases.atom
```

## Usage examples

```bash
# Subscribe to a feed
blog feed add https://news.ycombinator.com/rss

# Fetch new posts and sync with git remote
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
blog 1w..
blog 3m..1m
blog /d 2w..1w

# Combine filters - list unread posts form HackerNews grouped by date
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
blog 1w.. export

# List subscriptions
blog feed ls

# Remove a feed
blog feed rm https://news.ycombinator.com/rss
blog feed rm @hn
```

## Design philosophy

I built `blogtato` around the idea of subscription detox and simplicity. I just
wanted to use a simple and RSS reader that is not distracting, but can be
synced between different devices seamlessly without having to set up another
user account and paying another monthly subscription fee.

`blogtato` uses a simple database that stores data in JSONL files and syncs
them using `git`. From a performance standpoint, this is admittedly
sub-optimal, and an quite esoteric design. At the same time, if you are
comfortable with CLI tools you likely has access to a remote `git` host such as
GitHub, GitLab or a Forgejo instance: and that's all `blogtato` needs to be
able to keep data up to date on all of your devices. From a user perspective,
this just works with effectively zero configuration.

`blogtato`'s database uses a conflict-free design: even if you have diverging
changes between different devices, you will never have to manually resolve
conflicts. You can forget about `git` being there.

Network operations are always initiated by the user. There is no need for a
continuously running server. And all operations that don't strictly need
network access work offline.

It is my goal to keep the feature-set and the complexity of this project down,
so that it can be maintained with minimal effort and can be considered to be
"done".

## Naming

The naming is meant to symbolize simplicity and pragmatic silliness: I just
mashed the word "blog" together with the first word I could think of: potato.
