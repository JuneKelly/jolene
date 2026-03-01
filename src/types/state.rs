use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Source type discriminant stored in state.toml.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    #[default]
    GitHub,
    Local,
    Url,
}

impl fmt::Display for SourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceKind::GitHub => write!(f, "github"),
            SourceKind::Local => write!(f, "local"),
            SourceKind::Url => write!(f, "url"),
        }
    }
}

/// Root of state.toml.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct State {
    #[serde(default)]
    pub packages: Vec<PackageState>,
}

/// One installed package entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageState {
    /// Source type. Defaults to GitHub for pre-existing entries.
    #[serde(default)]
    pub source_kind: SourceKind,
    /// Human-readable source identifier stored for display and lookup:
    /// - GitHub: "owner/repo"
    /// - Local:  absolute path string
    /// - Url:    the full URL
    pub source: String,
    /// The git URL that was used to clone this package.
    /// None for pre-existing entries that pre-date this field.
    #[serde(default)]
    pub clone_url: Option<String>,
    /// Relative to ~/.jolene/ — always "repos/{64-char-hex}".
    pub clone_path: String,
    pub branch: String,
    pub commit: String,
    pub installed_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub installations: Vec<Installation>,
}

impl PackageState {
    /// The 64-char hex SHA256 identifying this package within `~/.jolene/repos/`.
    pub fn store_key(&self) -> &str {
        let key = self
            .clone_path
            .strip_prefix("repos/")
            .unwrap_or(&self.clone_path);
        debug_assert!(
            key.len() == 64 && key.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')),
            "clone_path invariant violated: expected repos/{{64-char-hex}}, got: {}",
            self.clone_path
        );
        key
    }
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
