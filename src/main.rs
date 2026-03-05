mod commands;
mod feed;
mod git;
mod http;
mod progress;
mod query;
mod store;
mod synctato;
mod tables;
#[cfg(test)]
mod test_helpers;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use commands::RESERVED_COMMANDS;

/// A simple RSS/Atom feed reader
#[derive(Parser)]
#[command(after_help = QUERY_HELP)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,
}

const QUERY_HELP: &str = "\
QUERY LANGUAGE:
  Grouping (up to 2):
    /d          Group by date
    /w          Group by week
    /f          Group by feed

  Filtering:
    @shorthand  Show only posts from a specific feed
    .read       Show only read posts
    .unread     Show only unread posts
    .all        Show all posts (override default filter)

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
  blog /d since:2w until:1w   Posts from 1-2 weeks ago, grouped by date
  blog a open                 Open post with shorthand 'a'
  blog a read                 Print URL of post 'a'
  blog a unread               Mark post 'a' as unread
  blog .unread                Show only unread posts
  blog @myblog .unread        Show unread posts from @myblog
  blog .all                   Show all posts (bypass defaults)
  blog .all export             Export all posts as JSONL
  blog @myblog export          Export posts from @myblog as JSONL";

#[derive(Subcommand)]
enum Command {
    /// Display items from posts.jsonl
    #[command(after_help = QUERY_HELP)]
    Show {
        /// Query arguments (see below)
        args: Vec<String>,
    },
    /// Open a post in the default browser
    Open,
    /// Print the URL of a post to stdout
    Read,
    /// Manage feed subscriptions
    Feed {
        #[command(subcommand)]
        command: FeedCommand,
    },
    /// Fetch feeds and sync with remote
    Sync,
    /// Mark a post as unread
    Unread,
    /// Export matching posts as JSONL
    #[command(after_help = QUERY_HELP)]
    Export {
        /// Query arguments (see below)
        args: Vec<String>,
    },
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

fn split_at_command(args: Vec<String>) -> (Vec<String>, Vec<String>) {
    let cmd_pos = args[1..]
        .iter()
        .position(|a| RESERVED_COMMANDS.contains(&a.as_str()));
    let split = cmd_pos.map(|p| p + 1).unwrap_or(args.len());
    let filter = args[1..split]
        .iter()
        .filter(|a| !a.starts_with('-'))
        .cloned()
        .collect();
    let mut cmd_args = vec![args[0].clone()];
    cmd_args.extend(
        args[1..split]
            .iter()
            .filter(|a| a.starts_with('-'))
            .cloned(),
    );
    cmd_args.extend_from_slice(&args[split..]);
    (filter, cmd_args)
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

fn ensure_no_query(query: &query::Query, command: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        query.is_empty(),
        "{command} command does not accept a filter"
    );
    Ok(())
}

fn run() -> anyhow::Result<()> {
    let (filter, cmd_args) = split_at_command(std::env::args().collect());
    let args = Args::parse_from(cmd_args);
    let store_dir = store_dir()?;

    if let Some(Command::Clone { ref url }) = args.command {
        return commands::clone::cmd_clone(&store_dir, url);
    }

    let mut store = store::Store::open(&store_dir)?;

    match args.command {
        Some(Command::Show { ref args }) => {
            let all_args: Vec<String> = filter.into_iter().chain(args.iter().cloned()).collect();
            let q = query::parse_query(&all_args)?;
            commands::show::cmd_show(&store, &q)?;
        }
        Some(Command::Open) => {
            let q = query::parse_query(&filter)?;
            commands::open::cmd_open(&mut store, &q)?;
        }
        Some(Command::Read) => {
            let q = query::parse_query(&filter)?;
            commands::open::cmd_read(&mut store, &q)?;
        }
        Some(Command::Unread) => {
            let q = query::parse_query(&filter)?;
            commands::open::cmd_unread(&mut store, &q)?;
        }
        Some(Command::Export { ref args }) => {
            let all_args: Vec<String> = filter.into_iter().chain(args.iter().cloned()).collect();
            let q = query::parse_query(&all_args)?;
            commands::export::cmd_export(&store, &q)?;
        }
        Some(Command::Feed {
            command: FeedCommand::Add { ref url },
        }) => {
            let q = query::parse_query(&filter)?;
            ensure_no_query(&q, "feed")?;
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
            let q = query::parse_query(&filter)?;
            ensure_no_query(&q, "feed")?;
            store.transact(&format!("remove feed: {url}"), |tx| {
                commands::remove::cmd_remove(tx, url)
            })?;
        }
        Some(Command::Feed {
            command: FeedCommand::Ls,
        }) => {
            let q = query::parse_query(&filter)?;
            ensure_no_query(&q, "feed")?;
            commands::feed_ls::cmd_feed_ls(&store)?;
        }
        Some(Command::Sync) => {
            let q = query::parse_query(&filter)?;
            ensure_no_query(&q, "sync")?;
            commands::sync::cmd_sync(&mut store)?;
        }
        Some(Command::Git { ref args }) => {
            let q = query::parse_query(&filter)?;
            ensure_no_query(&q, "git")?;
            store.git_passthrough(args)?;
        }
        Some(Command::Clone { .. }) => unreachable!(),
        None => {
            let q = query::parse_query(&filter)?;
            commands::show::cmd_show(&store, &q)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn args(input: &[&str]) -> Vec<String> {
        input.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_split_at_command_filter_before_command() {
        let (filter, cmd) = split_at_command(args(&["blog", "a", "open"]));
        assert_eq!(filter, args(&["a"]));
        assert_eq!(cmd, args(&["blog", "open"]));
    }

    #[test]
    fn test_split_at_command_no_filter() {
        let (filter, cmd) = split_at_command(args(&["blog", "sync"]));
        assert_eq!(filter, Vec::<String>::new());
        assert_eq!(cmd, args(&["blog", "sync"]));
    }

    #[test]
    fn test_split_at_command_no_command() {
        let (filter, cmd) = split_at_command(args(&["blog", "/d", "@hn"]));
        assert_eq!(filter, args(&["/d", "@hn"]));
        assert_eq!(cmd, args(&["blog"]));
    }

    #[test]
    fn test_split_at_command_help_flag() {
        let (filter, cmd) = split_at_command(args(&["blog", "--help"]));
        assert_eq!(filter, Vec::<String>::new());
        assert_eq!(cmd, args(&["blog", "--help"]));
    }

    #[test]
    fn test_split_at_command_feed_subcommand() {
        let (filter, cmd) = split_at_command(args(&["blog", "feed", "add", "url"]));
        assert_eq!(filter, Vec::<String>::new());
        assert_eq!(cmd, args(&["blog", "feed", "add", "url"]));
    }

    #[test]
    fn test_split_at_command_show_with_args() {
        let (filter, cmd) = split_at_command(args(&["blog", "show", "/d"]));
        assert_eq!(filter, Vec::<String>::new());
        assert_eq!(cmd, args(&["blog", "show", "/d"]));
    }

    #[test]
    fn test_split_at_command_filter_with_show() {
        let (filter, cmd) = split_at_command(args(&["blog", "/d", "show"]));
        assert_eq!(filter, args(&["/d"]));
        assert_eq!(cmd, args(&["blog", "show"]));
    }

    #[test]
    fn test_reserved_commands_match_clap_subcommands() {
        use clap::CommandFactory;
        let clap_names: std::collections::HashSet<_> = Args::command()
            .get_subcommands()
            .map(|c| c.get_name().to_string())
            .collect();
        let reserved: std::collections::HashSet<_> =
            RESERVED_COMMANDS.iter().map(|s| s.to_string()).collect();
        assert_eq!(
            clap_names, reserved,
            "RESERVED_COMMANDS must match clap subcommands"
        );
    }
}
