use std::fmt;
use std::path::PathBuf;

use dirs::home_dir;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Target {
    ClaudeCode,
    OpenCode,
    Codex,
}

impl Target {
    pub fn all() -> &'static [Target] {
        &[Target::ClaudeCode, Target::OpenCode, Target::Codex]
    }

    pub fn slug(self) -> &'static str {
        match self {
            Target::ClaudeCode => "claude-code",
            Target::OpenCode => "opencode",
            Target::Codex => "codex",
        }
    }

    pub fn from_slug(s: &str) -> Option<Target> {
        match s {
            "claude-code" => Some(Target::ClaudeCode),
            "opencode" => Some(Target::OpenCode),
            "codex" => Some(Target::Codex),
            _ => None,
        }
    }

    pub fn config_root(self) -> Option<PathBuf> {
        let home = home_dir()?;
        let path = match self {
            Target::ClaudeCode => home.join(".claude"),
            Target::OpenCode => home.join(".config").join("opencode"),
            Target::Codex => home.join(".codex"),
        };
        Some(path)
    }

    pub fn supports_commands(self) -> bool {
        matches!(self, Target::ClaudeCode | Target::OpenCode)
    }

    pub fn supports_skills(self) -> bool {
        true
    }

    pub fn supports_agents(self) -> bool {
        matches!(self, Target::ClaudeCode | Target::OpenCode)
    }

    pub fn exists(self) -> bool {
        self.config_root()
            .map(|p| p.exists())
            .unwrap_or(false)
    }

    pub fn detect_available() -> Vec<Target> {
        Target::all()
            .iter()
            .copied()
            .filter(|t| t.exists())
            .collect()
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.slug())
    }
}
