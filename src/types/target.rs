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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_slug_known_values() {
        assert_eq!(Target::from_slug("claude-code"), Some(Target::ClaudeCode));
        assert_eq!(Target::from_slug("opencode"), Some(Target::OpenCode));
        assert_eq!(Target::from_slug("codex"), Some(Target::Codex));
    }

    #[test]
    fn from_slug_unknown_returns_none() {
        assert_eq!(Target::from_slug("vscode"), None);
        assert_eq!(Target::from_slug(""), None);
        assert_eq!(Target::from_slug("Claude-Code"), None);
    }

    #[test]
    fn slug_roundtrips_through_from_slug() {
        for target in Target::all() {
            assert_eq!(Target::from_slug(target.slug()), Some(*target));
        }
    }

    #[test]
    fn supports_commands() {
        assert!(Target::ClaudeCode.supports_commands());
        assert!(Target::OpenCode.supports_commands());
        assert!(!Target::Codex.supports_commands());
    }

    #[test]
    fn supports_skills_for_all_targets() {
        for target in Target::all() {
            assert!(target.supports_skills(), "{} should support skills", target.slug());
        }
    }

    #[test]
    fn supports_agents() {
        assert!(Target::ClaudeCode.supports_agents());
        assert!(Target::OpenCode.supports_agents());
        assert!(!Target::Codex.supports_agents());
    }

    #[test]
    fn display_matches_slug() {
        for target in Target::all() {
            assert_eq!(format!("{target}"), target.slug());
        }
    }

    #[test]
    fn config_root_is_some() {
        for target in Target::all() {
            assert!(
                target.config_root().is_some(),
                "{} config_root returned None",
                target.slug()
            );
        }
    }
}
