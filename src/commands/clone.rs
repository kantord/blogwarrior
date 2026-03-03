use std::path::Path;
use std::time::Duration;

use crate::store;
use anyhow::{Context, bail};
use indicatif::{ProgressBar, ProgressStyle};

fn expand_url(url: &str) -> String {
    let is_full_url = url.contains(':'); // https://, git@host:, file://
    let is_relative_path = url.starts_with('.'); // ./repo, ../dir/repo

    if is_full_url || is_relative_path {
        return url.to_string();
    }

    if let Some((user, repo)) = url.split_once('/')
        && !repo.contains('/')
    {
        return format!("git@github.com:{user}/{repo}.git");
    }
    url.to_string()
}

pub(crate) fn cmd_clone(store_dir: &Path, url: &str) -> anyhow::Result<()> {
    if store_dir.exists() {
        let has_entries = std::fs::read_dir(store_dir)
            .context("failed to read store directory")?
            .next()
            .is_some();
        if has_entries {
            bail!(
                "a local database already exists at {}; remove it first if you want to re-clone",
                store_dir.display()
            );
        }
    }

    let expanded = expand_url(url);

    let sp = ProgressBar::new_spinner();
    sp.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    sp.enable_steady_tick(Duration::from_millis(80));
    sp.set_message(format!("Cloning into {}...", store_dir.display()));

    store::clone_store(store_dir, &expanded)?;

    sp.finish_with_message(format!("Cloned into {}.", store_dir.display()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::shorthand("foolorem/newsbar", "git@github.com:foolorem/newsbar.git")]
    #[case::https_url("https://github.com/user/repo.git", "https://github.com/user/repo.git")]
    #[case::ssh_url("git@github.com:user/repo.git", "git@github.com:user/repo.git")]
    #[case::relative_path("./local/repo", "./local/repo")]
    #[case::bare_name("something", "something")]
    fn test_expand_url(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(expand_url(input), expected);
    }
}
