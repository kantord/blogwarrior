mod commands;
mod feed;
mod feed_source;
mod git;
mod store;

use std::path::PathBuf;

use anyhow::ensure;
use clap::{Parser, Subcommand};
use synctato::Database;

/// A simple RSS/Atom feed reader
#[derive(Parser)]
#[command(args_conflicts_with_subcommands = true)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,

    /// Positional arguments: grouping mode (d, f, df, fd) and/or @shorthand filter
    args: Vec<String>,
}

#[derive(Subcommand)]
enum Command {
    /// Display items from posts.jsonl
    Show {
        /// Positional arguments: grouping mode (d, f, df, fd) and/or @shorthand filter
        args: Vec<String>,
    },
    /// Open a post in the default browser
    Open {
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

fn parse_show_args(args: &[String]) -> anyhow::Result<(String, Option<String>)> {
    let mut group = String::new();
    let mut filter = None;
    for arg in args {
        if arg.starts_with('@') {
            filter = Some(arg.clone());
        } else {
            ensure!(
                group.is_empty(),
                "Multiple grouping arguments: '{}' and '{}'. Use a single argument like '{}{}'.",
                group,
                arg,
                group,
                arg
            );
            group = arg.clone();
        }
    }
    Ok((group, filter))
}

fn store_dir() -> anyhow::Result<PathBuf> {
    if let Ok(val) = std::env::var("RSS_STORE") {
        return Ok(PathBuf::from(val));
    }
    dirs::data_dir()
        .map(|d| d.join("blogtato"))
        .ok_or_else(|| anyhow::anyhow!("could not determine data directory; set RSS_STORE"))
}

fn transact(
    store: &mut store::Store,
    msg: &str,
    f: impl FnOnce(&mut store::Transaction<'_>) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let repo = git::try_open_repo(store.path());
    if let Some(ref repo) = repo {
        git::ensure_clean(repo)?;
    }
    store.transaction(f)?;
    if let Some(ref repo) = repo {
        git::auto_commit(repo, msg)?;
    }
    Ok(())
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
            let (group, filter) = parse_show_args(args)?;
            commands::show::cmd_show(&store, &group, filter.as_deref())?;
        }
        Some(Command::Open { ref shorthand }) => {
            commands::open::cmd_open(&store, shorthand)?;
        }
        Some(Command::Feed {
            command: FeedCommand::Add { ref url },
        }) => {
            let resolved = commands::add::resolve_feed_url(url)?;
            if resolved != *url {
                eprintln!("Discovered feed: {resolved}");
            }
            transact(&mut store, &format!("add feed: {resolved}"), |tx| {
                commands::add::cmd_add(tx, &resolved)
            })?;
            eprintln!("Added {resolved}");
            eprintln!("Run `blog sync` to fetch posts.");
        }
        Some(Command::Feed {
            command: FeedCommand::Rm { ref url },
        }) => {
            transact(&mut store, &format!("remove feed: {url}"), |tx| {
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
            git::git_passthrough(store.path(), args)?;
        }
        Some(Command::Clone { .. }) => unreachable!(),
        None => {
            let (group, filter) = parse_show_args(&args.args)?;
            commands::show::cmd_show(&store, &group, filter.as_deref())?;
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
