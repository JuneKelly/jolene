use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use dirs::home_dir;

pub fn jolene_root() -> Result<PathBuf> {
    home_dir()
        .map(|h| h.join(".jolene"))
        .context("Could not determine home directory")
}

pub fn repos_dir() -> Result<PathBuf> {
    Ok(jolene_root()?.join("repos"))
}

pub fn state_file() -> Result<PathBuf> {
    Ok(jolene_root()?.join("state.toml"))
}

pub fn clone_dir(author: &str, repo: &str) -> Result<PathBuf> {
    Ok(repos_dir()?.join(author).join(repo))
}

/// Convert an absolute path to a `~/...` display string for user-facing output.
pub fn display_path(path: &Path) -> String {
    if let Some(home) = home_dir() {
        if let Ok(rel) = path.strip_prefix(&home) {
            return format!("~/{}", rel.display());
        }
    }
    path.display().to_string()
}
