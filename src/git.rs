use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

const ALLOWED_SCHEMES: &[&str] = &["https://", "http://", "git://", "ssh://", "git@"];

/// Validate that a git URL uses an allowed scheme.
///
/// Accepts `https://`, `http://`, `git://`, `ssh://`, and `git@` (SCP-style).
/// Also accepts bare filesystem paths (for local clones).
pub fn validate_url(url: &str) -> Result<()> {
    // Local filesystem paths are always valid (used by Source::Local).
    if url.starts_with('/') || url.starts_with('.') {
        return Ok(());
    }

    if !ALLOWED_SCHEMES.iter().any(|s| url.starts_with(s)) {
        bail!(
            "Unsupported git URL scheme: '{}'\n  Supported schemes: https://, http://, git://, ssh://, git@",
            url
        );
    }

    if url.contains('@') && url.contains(':') {
        // SCP-style or URL with credentials — check for embedded credentials
        // in URL-style (not SCP-style git@host:path).
        if let Some(authority) = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")) {
            if let Some(at_pos) = authority.find('@') {
                if authority[..at_pos].contains(':') {
                    bail!(
                        "Git URL appears to contain embedded credentials. Use git credential helpers instead."
                    );
                }
            }
        }
    }

    Ok(())
}

pub fn clone(url: &str, dest: &Path) -> Result<()> {
    validate_url(url)?;

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    let status = Command::new("git")
        .args(["clone", url, &dest.to_string_lossy()])
        .status()
        .context("Failed to run git clone")?;

    if !status.success() {
        bail!(
            "Failed to clone {}\n  Repository not found or not accessible.",
            url
        );
    }
    Ok(())
}

pub fn pull(repo_dir: &Path) -> Result<()> {
    let status = Command::new("git")
        .args(["pull", "--ff-only"])
        .current_dir(repo_dir)
        .status()
        .context("Failed to run git pull")?;

    if !status.success() {
        bail!("git pull failed in {}", repo_dir.display());
    }
    Ok(())
}

pub fn current_branch(repo_dir: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(repo_dir)
        .output()
        .context("Failed to run git rev-parse")?;

    if !output.status.success() {
        bail!("Could not determine branch in {}", repo_dir.display());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn full_commit(repo_dir: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_dir)
        .output()
        .context("Failed to run git rev-parse")?;

    if !output.status.success() {
        bail!("Could not determine commit in {}", repo_dir.display());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_https() {
        assert!(validate_url("https://github.com/foo/bar.git").is_ok());
    }

    #[test]
    fn accepts_http() {
        assert!(validate_url("http://example.com/repo.git").is_ok());
    }

    #[test]
    fn accepts_git_protocol() {
        assert!(validate_url("git://example.com/repo.git").is_ok());
    }

    #[test]
    fn accepts_ssh() {
        assert!(validate_url("ssh://git@example.com/repo.git").is_ok());
    }

    #[test]
    fn accepts_scp_style() {
        assert!(validate_url("git@github.com:foo/bar.git").is_ok());
    }

    #[test]
    fn accepts_local_absolute_path() {
        assert!(validate_url("/Users/junebug/my-repo").is_ok());
    }

    #[test]
    fn accepts_local_relative_path() {
        assert!(validate_url("./my-repo").is_ok());
    }

    #[test]
    fn rejects_unknown_scheme() {
        let err = validate_url("ftp://example.com/repo.git").unwrap_err();
        assert!(err.to_string().contains("Unsupported git URL scheme"));
    }

    #[test]
    fn rejects_embedded_credentials() {
        let err = validate_url("https://user:pass@example.com/repo.git").unwrap_err();
        assert!(err.to_string().contains("embedded credentials"));
    }

    #[test]
    fn accepts_https_with_username_only() {
        assert!(validate_url("https://token@github.com/foo/bar.git").is_ok());
    }
}
