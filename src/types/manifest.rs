use std::collections::BTreeMap;

use anyhow::Result;
use serde::Deserialize;

use super::var_value::VarValue;

/// Top-level jolene.toml structure.
#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    #[serde(alias = "package")]
    pub bundle: Bundle,
    pub content: ContentDecl,
    #[serde(default)]
    pub template: Option<TemplateDecl>,
}

/// Optional `[template]` section in the manifest.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct TemplateDecl {
    #[serde(default)]
    pub vars: BTreeMap<String, toml::Value>,
    /// Content item names that must never be treated as templates, regardless
    /// of whether their files contain template delimiters. Useful when a file
    /// contains literal `{~` or `{%~` sequences that are not Jolene syntax
    /// (e.g., documentation that explains Jolene's own template syntax).
    #[serde(default)]
    pub exclude: Vec<String>,
}

impl Manifest {
    /// Returns the list of content item names excluded from template detection.
    /// Returns an empty slice when no `[template]` section is present.
    pub fn template_exclude(&self) -> &[String] {
        self.template.as_ref().map_or(&[], |t| t.exclude.as_slice())
    }

    /// Convert the raw TOML `[template.vars]` into typed `VarValue` entries.
    pub fn template_vars(&self) -> Result<BTreeMap<String, VarValue>> {
        let Some(ref tmpl) = self.template else {
            return Ok(BTreeMap::new());
        };
        let mut out = BTreeMap::new();
        for (key, val) in &tmpl.vars {
            out.insert(
                key.clone(),
                VarValue::from_toml_value(val.clone())
                    .map_err(|e| anyhow::anyhow!("[template.vars].{}: {}", key, e))?,
            );
        }
        Ok(out)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Bundle {
    #[allow(dead_code)]
    pub name: String,
    pub description: String,
    pub version: String,
    pub authors: Vec<String>,
    pub license: String,
    pub urls: Option<Urls>,
    /// Optional prefix for content names (e.g. `prefix = "jb"` → `jb--review.md`).
    pub prefix: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Urls {
    pub repository: Option<String>,
    pub homepage: Option<String>,
}

/// Declares which content items the bundle provides.
/// At least one field must be non-empty.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ContentDecl {
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub agents: Vec<String>,
}

impl ContentDecl {
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty() && self.skills.is_empty() && self.agents.is_empty()
    }

    /// Human-readable summary, e.g. "2 commands, 1 skill".
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if !self.commands.is_empty() {
            let n = self.commands.len();
            parts.push(format!(
                "{} {}",
                n,
                if n == 1 { "command" } else { "commands" }
            ));
        }
        if !self.skills.is_empty() {
            let n = self.skills.len();
            parts.push(format!("{} {}", n, if n == 1 { "skill" } else { "skills" }));
        }
        if !self.agents.is_empty() {
            let n = self.agents.len();
            parts.push(format!("{} {}", n, if n == 1 { "agent" } else { "agents" }));
        }
        if parts.is_empty() {
            "no content".to_string()
        } else {
            parts.join(", ")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decl(commands: &[&str], skills: &[&str], agents: &[&str]) -> ContentDecl {
        ContentDecl {
            commands: commands.iter().map(|s| s.to_string()).collect(),
            skills: skills.iter().map(|s| s.to_string()).collect(),
            agents: agents.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn is_empty_when_all_vecs_empty() {
        assert!(ContentDecl::default().is_empty());
    }

    #[test]
    fn is_empty_false_when_commands_present() {
        assert!(!decl(&["review"], &[], &[]).is_empty());
    }

    #[test]
    fn is_empty_false_when_skills_present() {
        assert!(!decl(&[], &["code-analysis"], &[]).is_empty());
    }

    #[test]
    fn is_empty_false_when_agents_present() {
        assert!(!decl(&[], &[], &["reviewer"]).is_empty());
    }

    #[test]
    fn summary_empty_returns_no_content() {
        assert_eq!(ContentDecl::default().summary(), "no content");
    }

    #[test]
    fn summary_singular_command() {
        assert_eq!(decl(&["review"], &[], &[]).summary(), "1 command");
    }

    #[test]
    fn summary_plural_commands() {
        assert_eq!(
            decl(&["review", "deploy"], &[], &[]).summary(),
            "2 commands"
        );
    }

    #[test]
    fn summary_singular_skill() {
        assert_eq!(decl(&[], &["analysis"], &[]).summary(), "1 skill");
    }

    #[test]
    fn summary_plural_skills() {
        assert_eq!(decl(&[], &["analysis", "style"], &[]).summary(), "2 skills");
    }

    #[test]
    fn summary_singular_agent() {
        assert_eq!(decl(&[], &[], &["reviewer"]).summary(), "1 agent");
    }

    #[test]
    fn summary_plural_agents() {
        assert_eq!(
            decl(&[], &[], &["reviewer", "planner"]).summary(),
            "2 agents"
        );
    }

    #[test]
    fn summary_all_types() {
        assert_eq!(
            decl(&["review"], &["analysis", "style"], &["reviewer"]).summary(),
            "1 command, 2 skills, 1 agent"
        );
    }

    // template_exclude tests

    fn manifest_with_exclude(exclude: &[&str]) -> Manifest {
        Manifest {
            bundle: Bundle {
                name: "test".to_string(),
                description: "test".to_string(),
                version: "1.0.0".to_string(),
                authors: vec![],
                license: "MIT".to_string(),
                urls: None,
                prefix: None,
            },
            content: ContentDecl::default(),
            template: Some(TemplateDecl {
                vars: BTreeMap::new(),
                exclude: exclude.iter().map(|s| s.to_string()).collect(),
            }),
        }
    }

    #[test]
    fn template_exclude_absent_returns_empty() {
        let m = Manifest {
            bundle: Bundle {
                name: "test".to_string(),
                description: "test".to_string(),
                version: "1.0.0".to_string(),
                authors: vec![],
                license: "MIT".to_string(),
                urls: None,
                prefix: None,
            },
            content: ContentDecl::default(),
            template: None,
        };
        assert!(m.template_exclude().is_empty());
    }

    #[test]
    fn template_exclude_present_no_key_returns_empty() {
        let m = manifest_with_exclude(&[]);
        assert!(m.template_exclude().is_empty());
    }

    #[test]
    fn template_exclude_returns_single_name() {
        let m = manifest_with_exclude(&["review"]);
        assert_eq!(m.template_exclude(), &["review".to_string()]);
    }

    #[test]
    fn template_exclude_returns_multiple_names() {
        let m = manifest_with_exclude(&["review", "code-analysis"]);
        assert_eq!(
            m.template_exclude(),
            &["review".to_string(), "code-analysis".to_string()]
        );
    }
}
