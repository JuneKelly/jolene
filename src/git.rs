use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

pub fn clone(url: &str, dest: &Path) -> Result<()> {
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
