use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, bail};

/// Run a git CLI command, capturing stdout/stderr while inheriting stdin.
/// Inheriting stdin is required so that SSH can prompt for passphrases or
/// host-key confirmation when the remote uses SSH transport; without it the
/// subprocess blocks indefinitely because the closed pipe cannot display a
/// prompt or receive input.
pub(crate) fn git_output(args: &[&str]) -> anyhow::Result<std::process::Output> {
    Command::new("git")
        .args(args)
        .stdin(Stdio::inherit())
        .output()
        .context("failed to run git")
}

pub(crate) fn fetch(path: &Path) -> anyhow::Result<()> {
    let output = git_output(&["-C", &path.to_string_lossy(), "fetch", "origin"])?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git fetch failed: {}", stderr.trim());
    }
    Ok(())
}

pub(crate) fn push(path: &Path) -> anyhow::Result<()> {
    let output = git_output(&["-C", &path.to_string_lossy(), "push", "origin", "HEAD"])?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git push failed: {}", stderr.trim());
    }
    Ok(())
}

pub(crate) fn has_remote(path: &Path) -> bool {
    Command::new("git")
        .args(["-C", &path.to_string_lossy(), "remote", "get-url", "origin"])
        .output()
        .is_ok_and(|o| o.status.success())
}

pub(crate) fn git_passthrough(path: &Path, args: &[String]) -> anyhow::Result<()> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(path);
    cmd.args(args);

    let status = cmd.status().context("failed to run git")?;
    if !status.success() {
        bail!("git exited with {}", status);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::Repository;
    use std::path::Path;
    use tempfile::TempDir;

    fn init_repo(path: &Path) -> Repository {
        let mut opts = git2::RepositoryInitOptions::new();
        opts.initial_head("main");
        Repository::init_opts(path, &opts).unwrap()
    }

    #[test]
    fn test_has_remote_false() {
        let dir = TempDir::new().unwrap();
        init_repo(dir.path());
        assert!(!has_remote(dir.path()));
    }

    #[test]
    fn test_has_remote_true() {
        let dir = TempDir::new().unwrap();
        let repo = init_repo(dir.path());
        repo.remote("origin", "https://example.com/repo.git")
            .unwrap();
        assert!(has_remote(dir.path()));
    }

    #[test]
    fn test_fetch_no_remote() {
        let dir = TempDir::new().unwrap();
        init_repo(dir.path());
        let result = fetch(dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_git_passthrough_status() {
        let dir = TempDir::new().unwrap();
        init_repo(dir.path());
        let result = git_passthrough(dir.path(), &["status".to_string()]);
        assert!(result.is_ok());
    }
}
