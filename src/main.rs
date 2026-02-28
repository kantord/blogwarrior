mod commands;
mod feed;
mod feed_source;

use std::path::PathBuf;

use anyhow::ensure;
use clap::{Parser, Subcommand};

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
    /// Fetch feeds and save items to posts.jsonl
    Pull,
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

fn run() -> anyhow::Result<()> {
    let args = Args::parse();
    let store = store_dir()?;

    match args.command {
        Some(Command::Pull) => {
            commands::pull::cmd_pull(&store)?;
        }
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
            commands::add::cmd_add(&store, url)?;
        }
        Some(Command::Feed {
            command: FeedCommand::Rm { ref url },
        }) => {
            commands::remove::cmd_remove(&store, url)?;
        }
        Some(Command::Feed {
            command: FeedCommand::Ls,
        }) => {
            commands::feed_ls::cmd_feed_ls(&store)?;
        }
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
