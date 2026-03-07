mod cli;
mod local;

pub(crate) use cli::*;
pub(crate) use local::*;

// Re-exported for the store! macro expansion in external crates.
pub use local::{open_repo, read_remote_table};
