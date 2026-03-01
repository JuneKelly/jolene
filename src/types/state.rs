use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Root of state.toml.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct State {
    #[serde(default)]
    pub packages: Vec<PackageState>,
}

/// One installed package entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageState {
    /// "Author/repo"
    pub source: String,
    /// Relative to ~/.jolene/ (e.g. "repos/author/repo")
    pub clone_path: String,
    pub branch: String,
    pub commit: String,
    pub installed_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub installations: Vec<Installation>,
}

/// One target's installation record within a package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Installation {
    /// Target slug, e.g. "claude-code"
    pub target: String,
    #[serde(default)]
    pub symlinks: Vec<SymlinkEntry>,
}

/// A single managed symlink.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymlinkEntry {
    /// Relative to clone root, e.g. "commands/review.md"
    pub src: String,
    /// Display path with ~, e.g. "~/.claude/commands/review.md"
    pub dst: String,
}
