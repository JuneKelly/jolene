use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

const ALLOWED_SCHEMES: &[&str] = &["https://", "http://", "git://", "ssh://", "git@"];

/// Validate that a git URL uses an allowed scheme.
///
/// Accepts `https://`, `http://`, `git://`, `ssh://`, and `git@` (SCP-style).
/// Also accepts absolute filesystem paths (for `Source::Local` after canonicalize).
pub fn validate_url(url: &str) -> Result<()> {
    // Absolute filesystem paths are valid (used by Source::Local after canonicalize).
    if url.starts_with('/') {
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
        if let Some(authority) = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))
            && let Some(at_pos) = authority.find('@')
            && authority[..at_pos].contains(':')
        {
            bail!(
                "Git URL appears to contain embedded credentials. Use git credential helpers instead."
            );
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
    let output = Command::new("git")
        .args(["pull", "--ff-only"])
        .current_dir(repo_dir)
        .output()
        .context("Failed to run git pull")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("Not possible to fast-forward") || stderr.contains("fatal: Need to specify") {
            bail!(
                "git pull --ff-only failed in {} (local commits diverge from upstream).\n  \
                 If you have unpushed local commits (e.g. from a failed `jolene push`),\n  \
                 either push them manually or re-run with --force to discard local changes.",
                repo_dir.display()
            );
        }
        bail!("git pull failed in {}", repo_dir.display());
    }
    Ok(())
}

/// Force-pull by resetting the current branch to match origin.
/// This discards any local commits or changes.
pub fn pull_force(repo_dir: &Path) -> Result<()> {
    // Fetch latest from origin.
    let status = Command::new("git")
        .args(["fetch", "origin"])
        .current_dir(repo_dir)
        .status()
        .context("Failed to run git fetch")?;
    if !status.success() {
        bail!("git fetch failed in {}", repo_dir.display());
    }

    // Determine the current branch name.
    let branch = current_branch(repo_dir)?;

    // Reset to origin/<branch>.
    let remote_ref = format!("origin/{}", branch);
    let status = Command::new("git")
        .args(["reset", "--hard", &remote_ref])
        .current_dir(repo_dir)
        .status()
        .context("Failed to run git reset")?;
    if !status.success() {
        bail!("git reset --hard {} failed in {}", remote_ref, repo_dir.display());
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

pub fn status_short(repo_dir: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["status", "--short"])
        .current_dir(repo_dir)
        .output()
        .context("Failed to run git status")?;

    if !output.status.success() {
        bail!("git status failed in {}", repo_dir.display());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub fn add_all(repo_dir: &Path) -> Result<()> {
    let status = Command::new("git")
        .args(["add", "-A"])
        .current_dir(repo_dir)
        .status()
        .context("Failed to run git add")?;

    if !status.success() {
        bail!("git add failed in {}", repo_dir.display());
    }
    Ok(())
}

pub fn commit(repo_dir: &Path, message: &str) -> Result<()> {
    let status = Command::new("git")
        .args(["commit", "-m", message])
        .current_dir(repo_dir)
        .status()
        .context("Failed to run git commit")?;

    if !status.success() {
        bail!("git commit failed in {}", repo_dir.display());
    }
    Ok(())
}

pub fn push(repo_dir: &Path) -> Result<()> {
    let status = Command::new("git")
        .args(["push"])
        .current_dir(repo_dir)
        .status()
        .context("Failed to run git push")?;

    if !status.success() {
        bail!("git push failed in {}", repo_dir.display());
    }
    Ok(())
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
    fn rejects_relative_path() {
        let err = validate_url("./my-repo").unwrap_err();
        assert!(err.to_string().contains("Unsupported git URL scheme"));
    }

    #[test]
    fn rejects_dotdot_relative_path() {
        let err = validate_url("../my-repo").unwrap_err();
        assert!(err.to_string().contains("Unsupported git URL scheme"));
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
