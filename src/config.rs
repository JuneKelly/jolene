use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use dirs::home_dir;

/// Return the effective home directory, checking `JOLENE_EFFECTIVE_HOME` first.
#[allow(clippy::disallowed_methods)]
pub fn effective_home() -> Option<PathBuf> {
    if let Ok(val) = std::env::var("JOLENE_EFFECTIVE_HOME")
        && !val.is_empty()
    {
        return Some(PathBuf::from(val));
    }
    home_dir()
}

pub fn jolene_root() -> Result<PathBuf> {
    if let Ok(val) = std::env::var("JOLENE_ROOT")
        && !val.is_empty()
    {
        return Ok(PathBuf::from(val));
    }
    effective_home()
        .map(|h| h.join(".jolene"))
        .context("Could not determine home directory")
}

pub fn state_file() -> Result<PathBuf> {
    Ok(jolene_root()?.join("state.json"))
}

pub fn legacy_state_file() -> Result<PathBuf> {
    Ok(jolene_root()?.join("state.toml"))
}

/// Resolve an absolute clone directory from a `clone_path` value as stored in
/// state.json (e.g. `"repos/owner/repo"`, `"repos/local/name"`).
pub fn clone_root_for(clone_path: &str) -> Result<PathBuf> {
    Ok(jolene_root()?.join(clone_path))
}

pub fn rendered_root() -> Result<PathBuf> {
    Ok(jolene_root()?.join("rendered"))
}

/// Path to rendered content for a specific bundle+target combination.
pub fn rendered_path_for(store_key: &str, target_slug: &str) -> Result<PathBuf> {
    Ok(rendered_root()?.join(store_key).join(target_slug))
}

/// Convert an absolute path to a `~/...` display string for user-facing output.
pub fn display_path(path: &Path) -> String {
    if let Some(home) = effective_home()
        && let Ok(rel) = path.strip_prefix(&home)
    {
        return format!("~/{}", rel.display());
    }
    path.display().to_string()
}
