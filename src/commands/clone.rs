use std::path::Path;

use anyhow::Context;

use crate::data::BlogData;
use crate::utils::progress::spinner;

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

fn has_existing_store(store_dir: &Path) -> bool {
    if !store_dir.exists() {
        return false;
    }
    std::fs::read_dir(store_dir)
        .map(|mut d| d.next().is_some())
        .unwrap_or(false)
}

pub(crate) fn cmd_clone(store_dir: &Path, url: &str) -> anyhow::Result<()> {
    let expanded = expand_url(url);

    if has_existing_store(store_dir) {
        crate::data::ensure_gitattributes(store_dir)?;
        // Existing store: add remote and sync to merge unrelated histories
        let mut store = BlogData::open(store_dir)?;
        store.git_passthrough(&[
            "remote".to_string(),
            "add".to_string(),
            "origin".to_string(),
            expanded.clone(),
        ])?;
        // Fetch first so sync_remote sees the remote branch and merges
        // instead of trying a direct push into unrelated history.
        store.git_passthrough(&["fetch".to_string(), "origin".to_string()])?;

        let sp = spinner("Syncing with remote...");
        store.sync_remote(|_| {})?;
        sp.finish_with_message("Syncing with remote... done.");
    } else {
        // Fresh clone
        let sp = spinner(&format!("Cloning into {}...", store_dir.display()));
        synctato::clone_store(store_dir, &expanded).context("failed to clone store")?;
        sp.finish_with_message(format!("Cloned into {}.", store_dir.display()));
    }

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
