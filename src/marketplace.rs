use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

/// Root of `.claude-plugin/marketplace.json`.
#[derive(Debug, Clone, Deserialize)]
pub struct Marketplace {
    pub name: String,
    #[serde(default)]
    pub owner: Option<Owner>,
    #[serde(default)]
    pub metadata: Option<MarketplaceMetadata>,
    #[serde(default)]
    pub plugins: Vec<PluginEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Owner {
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MarketplaceMetadata {
    pub description: Option<String>,
}

/// A single plugin listed in the marketplace catalog.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PluginEntry {
    pub name: String,
    pub source: PluginSource,
    pub description: Option<String>,
    pub version: Option<String>,
}

/// Raw intermediate used only for deserialization of `PluginEntry`.
/// Supports both the tagged format (`"source": "relative", "path": "..."`)
/// and the path-as-source shorthand (`"source": "./plugins/my-plugin"`).
#[derive(Deserialize)]
struct RawPluginEntry {
    name: String,
    source: String,
    path: Option<String>,
    repo: Option<String>,
    url: Option<String>,
    #[serde(rename = "ref")]
    git_ref: Option<String>,
    description: Option<String>,
    version: Option<String>,
}

impl<'de> Deserialize<'de> for PluginEntry {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawPluginEntry::deserialize(deserializer)?;
        let source = match raw.source.as_str() {
            "relative" => {
                let path = raw
                    .path
                    .ok_or_else(|| serde::de::Error::missing_field("path"))?;
                PluginSource::Relative { path }
            }
            "github" => {
                let repo = raw
                    .repo
                    .ok_or_else(|| serde::de::Error::missing_field("repo"))?;
                PluginSource::GitHub {
                    repo,
                    git_ref: raw.git_ref,
                }
            }
            "url" => {
                let url = raw
                    .url
                    .ok_or_else(|| serde::de::Error::missing_field("url"))?;
                PluginSource::Url {
                    url,
                    git_ref: raw.git_ref,
                }
            }
            s if s.starts_with("./") || s.starts_with("../") => PluginSource::Relative {
                path: s.to_string(),
            },
            _ => PluginSource::Unsupported,
        };
        Ok(PluginEntry {
            name: raw.name,
            source,
            description: raw.description,
            version: raw.version,
        })
    }
}

/// How the plugin's code is located.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum PluginSource {
    Relative {
        path: String,
    },
    GitHub {
        repo: String,
        git_ref: Option<String>,
    },
    Url {
        url: String,
        git_ref: Option<String>,
    },
    Unsupported,
}

/// Metadata from `.claude-plugin/plugin.json` (inside a plugin directory).
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct PluginJson {
    pub name: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    #[serde(default)]
    pub hooks: Option<serde_json::Value>,
    #[serde(default)]
    pub mcp: Option<serde_json::Value>,
    #[serde(default)]
    pub lsp: Option<serde_json::Value>,
}

/// Claude Code-specific features that jolene intentionally ignores.
pub struct IgnoredFeatures {
    pub has_hooks: bool,
    pub has_mcp: bool,
    pub has_lsp: bool,
}

impl IgnoredFeatures {
    pub fn any(&self) -> bool {
        self.has_hooks || self.has_mcp || self.has_lsp
    }

    pub fn labels(&self) -> Vec<&'static str> {
        let mut v = Vec::new();
        if self.has_hooks {
            v.push("hooks");
        }
        if self.has_mcp {
            v.push("MCP servers");
        }
        if self.has_lsp {
            v.push("LSP servers");
        }
        v
    }
}

/// Parse the marketplace catalog from a repo clone.
pub fn load_marketplace(clone_root: &Path) -> Result<Marketplace> {
    let path = clone_root.join(".claude-plugin").join("marketplace.json");
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let mp: Marketplace =
        serde_json::from_str(&text).with_context(|| "Invalid marketplace.json")?;
    Ok(mp)
}

/// Parse plugin.json from a plugin directory, if it exists.
pub fn load_plugin_json(plugin_dir: &Path) -> Option<PluginJson> {
    let path = plugin_dir.join(".claude-plugin").join("plugin.json");
    if !path.exists() {
        return None;
    }
    let text = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Detect Claude Code-specific features that jolene won't install.
pub fn detect_ignored_features(plugin_dir: &Path) -> IgnoredFeatures {
    let has_hooks = plugin_dir.join("hooks").join("hooks.json").exists();
    let has_mcp = plugin_dir.join(".mcp.json").exists();
    let has_lsp = plugin_dir.join(".lsp.json").exists();

    // Also check plugin.json for inline declarations
    if let Some(pj) = load_plugin_json(plugin_dir) {
        return IgnoredFeatures {
            has_hooks: has_hooks || pj.hooks.is_some(),
            has_mcp: has_mcp || pj.mcp.is_some(),
            has_lsp: has_lsp || pj.lsp.is_some(),
        };
    }

    IgnoredFeatures {
        has_hooks,
        has_mcp,
        has_lsp,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_marketplace() {
        let json = r#"{
            "name": "test-marketplace",
            "plugins": []
        }"#;
        let mp: Marketplace = serde_json::from_str(json).unwrap();
        assert_eq!(mp.name, "test-marketplace");
        assert!(mp.plugins.is_empty());
    }

    #[test]
    fn parse_marketplace_with_relative_plugin() {
        let json = r#"{
            "name": "acme-tools",
            "owner": { "name": "DevTools Team" },
            "plugins": [
                {
                    "name": "review-plugin",
                    "source": "relative",
                    "path": "./plugins/review-plugin",
                    "description": "Code review skill"
                }
            ]
        }"#;
        let mp: Marketplace = serde_json::from_str(json).unwrap();
        assert_eq!(mp.plugins.len(), 1);
        assert_eq!(mp.plugins[0].name, "review-plugin");
        assert!(matches!(
            mp.plugins[0].source,
            PluginSource::Relative { .. }
        ));
    }

    #[test]
    fn parse_marketplace_with_github_plugin() {
        let json = r#"{
            "name": "acme-tools",
            "plugins": [
                {
                    "name": "deploy-tools",
                    "source": "github",
                    "repo": "acme-corp/deploy-tools",
                    "description": "Deployment automation"
                }
            ]
        }"#;
        let mp: Marketplace = serde_json::from_str(json).unwrap();
        assert_eq!(mp.plugins.len(), 1);
        assert!(matches!(mp.plugins[0].source, PluginSource::GitHub { .. }));
    }

    #[test]
    fn parse_marketplace_with_url_plugin() {
        let json = r#"{
            "name": "acme-tools",
            "plugins": [
                {
                    "name": "scanner",
                    "source": "url",
                    "url": "https://gitlab.com/acme/scanner.git",
                    "description": "Security scanner"
                }
            ]
        }"#;
        let mp: Marketplace = serde_json::from_str(json).unwrap();
        assert_eq!(mp.plugins.len(), 1);
        assert!(matches!(mp.plugins[0].source, PluginSource::Url { .. }));
    }

    #[test]
    fn parse_marketplace_with_path_as_source() {
        let json = r#"{
            "name": "sona-marketplace",
            "plugins": [
                {
                    "name": "walkthrough",
                    "source": "./plugins/skills/walkthrough",
                    "description": "Generate an interactive HTML walkthrough"
                }
            ]
        }"#;
        let mp: Marketplace = serde_json::from_str(json).unwrap();
        assert_eq!(mp.plugins.len(), 1);
        assert_eq!(mp.plugins[0].name, "walkthrough");
        match &mp.plugins[0].source {
            PluginSource::Relative { path } => {
                assert_eq!(path, "./plugins/skills/walkthrough");
            }
            other => panic!("Expected Relative, got {:?}", other),
        }
    }

    #[test]
    fn parse_marketplace_unsupported_source() {
        let json = r#"{
            "name": "acme-tools",
            "plugins": [
                {
                    "name": "npm-thing",
                    "source": "npm",
                    "package": "@acme/thing"
                }
            ]
        }"#;
        let mp: Marketplace = serde_json::from_str(json).unwrap();
        assert_eq!(mp.plugins.len(), 1);
        assert!(matches!(mp.plugins[0].source, PluginSource::Unsupported));
    }

    #[test]
    fn parse_plugin_json() {
        let json = r#"{
            "name": "review-plugin",
            "description": "Code review skill",
            "version": "1.0.0"
        }"#;
        let pj: PluginJson = serde_json::from_str(json).unwrap();
        assert_eq!(pj.name.as_deref(), Some("review-plugin"));
        assert_eq!(pj.version.as_deref(), Some("1.0.0"));
        assert!(pj.hooks.is_none());
    }
}
