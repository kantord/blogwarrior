mod commands;
mod feed;
mod feed_source;
mod git;
mod http;
mod progress;
mod query;
mod read_mark;
mod store;
mod synctato;
#[cfg(test)]
mod test_helpers;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// A simple RSS/Atom feed reader
#[derive(Parser)]
#[command(args_conflicts_with_subcommands = true, after_help = QUERY_HELP)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,

    /// Query arguments (see below)
    args: Vec<String>,
}

const QUERY_HELP: &str = "\
QUERY LANGUAGE:
  Grouping (up to 2):
    /d          Group by date
    /w          Group by week
    /f          Group by feed

  Filtering:
    @shorthand  Show only posts from a specific feed

  Date range:
    since:<date>    Show posts from this date onward
    until:<date>    Show posts up to this date
    <date>..<date>  Range shorthand (e.g. 3m..1m)
    <date>..        Open-ended range (from date onward)
    ..<date>        Open-ended range (up to date)

  Date values:
    2024-01-15  Absolute date (YYYY-MM-DD)
    3d          Relative: 3 days ago
    2w          Relative: 2 weeks ago
    1m          Relative: 1 month ago
    today       Start of today
    yesterday   Start of yesterday

EXAMPLES:
  blog /d                     Group by date
  blog /f /d                  Group by feed, then date
  blog @myblog                Show only posts from @myblog
  blog since:1w               Posts from the last week
  blog 3m..1m                 Posts from 1-3 months ago
  blog /d since:2w until:1w   Posts from 1-2 weeks ago, grouped by date";

#[derive(Subcommand)]
enum Command {
    /// Display items from posts.jsonl
    #[command(after_help = QUERY_HELP)]
    Show {
        /// Query arguments (see below)
        args: Vec<String>,
    },
    /// Open a post in the default browser
    Open {
        /// Post shorthand
        shorthand: String,
    },
    /// Print the URL of a post to stdout
    Read {
        /// Post shorthand
        shorthand: String,
    },
    /// Manage feed subscriptions
    Feed {
        #[command(subcommand)]
        command: FeedCommand,
    },
    /// Fetch feeds and sync with remote
    Sync,
    /// Run git commands in the store directory
    Git {
        /// Arguments to pass to git
        args: Vec<String>,
    },
    /// Clone an existing feed database from a git remote
    Clone {
        /// Git-clonable URL
        url: String,
    },
}

#[derive(Subcommand)]
enum FeedCommand {
    /// Subscribe to a feed by URL
    Add {
        /// The feed URL to subscribe to
        url: String,
    },
    /// Unsubscribe from a feed by URL or @shorthand
    Rm {
        /// The feed URL or @shorthand to unsubscribe from
        url: String,
    },
    /// List subscribed feeds
    Ls,
}

fn store_dir() -> anyhow::Result<PathBuf> {
    if let Ok(val) = std::env::var("RSS_STORE") {
        return Ok(PathBuf::from(val));
    }
    // Only one store ("default") is supported for now. The stores/ nesting
    // keeps the door open for multiple named stores in the future.
    dirs::data_dir()
        .map(|d| d.join("blogtato").join("stores").join("default"))
        .ok_or_else(|| anyhow::anyhow!("could not determine data directory; set RSS_STORE"))
}

fn mark_read(store: &mut store::Store, raw_id: String) -> anyhow::Result<()> {
    if store.reads().contains_key(&raw_id) {
        return Ok(());
    }
    store.transact("mark read", |tx| {
        tx.reads.upsert(read_mark::ReadMark {
            post_id: raw_id,
            read_at: chrono::Utc::now(),
        });
        Ok(())
    })
}

fn run() -> anyhow::Result<()> {
    let args = Args::parse();
    let store_dir = store_dir()?;

    if let Some(Command::Clone { ref url }) = args.command {
        return commands::clone::cmd_clone(&store_dir, url);
    }

    let mut store = store::Store::open(&store_dir)?;

    match args.command {
        Some(Command::Show { ref args }) => {
            let q = query::parse_show_args(args)?;
            commands::show::cmd_show(&store, &q.keys, q.filter.as_deref(), &q.date_filter)?;
        }
        Some(Command::Open { ref shorthand }) => {
            let raw_id = commands::open::cmd_open(&store, shorthand)?;
            mark_read(&mut store, raw_id)?;
        }
        Some(Command::Read { ref shorthand }) => {
            let raw_id = commands::open::cmd_read(&store, shorthand)?;
            mark_read(&mut store, raw_id)?;
        }
        Some(Command::Feed {
            command: FeedCommand::Add { ref url },
        }) => {
            let resolved = commands::add::resolve_feed_url(url)?;
            if resolved != *url {
                eprintln!("Discovered feed: {resolved}");
            }
            store.transact(&format!("add feed: {resolved}"), |tx| {
                commands::add::cmd_add(tx, &resolved)
            })?;
            eprintln!("Added {resolved}");
            eprintln!("Run `blog sync` to fetch posts.");
        }
        Some(Command::Feed {
            command: FeedCommand::Rm { ref url },
        }) => {
            store.transact(&format!("remove feed: {url}"), |tx| {
                commands::remove::cmd_remove(tx, url)
            })?;
        }
        Some(Command::Feed {
            command: FeedCommand::Ls,
        }) => {
            commands::feed_ls::cmd_feed_ls(&store)?;
        }
        Some(Command::Sync) => {
            commands::sync::cmd_sync(&mut store)?;
        }
        Some(Command::Git { ref args }) => {
            store.git_passthrough(args)?;
        }
        Some(Command::Clone { .. }) => unreachable!(),
        None => {
            let q = query::parse_show_args(&args.args)?;
            commands::show::cmd_show(&store, &q.keys, q.filter.as_deref(), &q.date_filter)?;
        }
    }
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
