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
    /// "github" | "local" | "url". Defaults to "github" for pre-existing entries.
    #[serde(default = "default_source_kind")]
    pub source_kind: String,
    /// Human-readable source identifier stored for display and lookup:
    /// - GitHub: "owner/repo"
    /// - Local:  absolute path string
    /// - Url:    the full URL
    pub source: String,
    /// The git URL that was used to clone this package.
    /// None for pre-existing entries that pre-date this field.
    #[serde(default)]
    pub clone_url: Option<String>,
    /// Relative to ~/.jolene/ (e.g. "repos/owner/repo", "repos/local/name")
    pub clone_path: String,
    pub branch: String,
    pub commit: String,
    pub installed_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub installations: Vec<Installation>,
}

impl PackageState {
    /// The key identifying this package within `~/.jolene/repos/`,
    /// i.e. `clone_path` with the `"repos/"` prefix stripped.
    pub fn store_key(&self) -> &str {
        self.clone_path
            .strip_prefix("repos/")
            .unwrap_or(&self.clone_path)
    }
}

fn default_source_kind() -> String {
    "github".to_string()
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
