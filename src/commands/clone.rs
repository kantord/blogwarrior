use std::path::Path;
use std::process::Command;
use std::time::Duration;

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

    let output = Command::new("git")
        .args([
            "clone",
            "--depth",
            "1",
            &expanded,
            &store_dir.to_string_lossy(),
        ])
        .output()
        .context("failed to run git clone")?;

    if !output.status.success() {
        sp.finish_and_clear();
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git clone failed: {}", stderr.trim());
    }

    sp.finish_with_message(format!("Cloned into {}.", store_dir.display()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_shorthand() {
        assert_eq!(
            expand_url("foolorem/newsbar"),
            "git@github.com:foolorem/newsbar.git"
        );
    }

    #[test]
    fn test_expand_preserves_full_urls() {
        assert_eq!(
            expand_url("https://github.com/user/repo.git"),
            "https://github.com/user/repo.git"
        );
        assert_eq!(
            expand_url("git@github.com:user/repo.git"),
            "git@github.com:user/repo.git"
        );
    }

    #[test]
    fn test_expand_preserves_relative_paths() {
        assert_eq!(expand_url("./local/repo"), "./local/repo");
    }

    #[test]
    fn test_expand_preserves_bare_name() {
        assert_eq!(expand_url("something"), "something");
    }
}
