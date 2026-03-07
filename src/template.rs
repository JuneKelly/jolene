use std::collections::HashMap;
use std::fs;
use std::os::unix::fs as unix_fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use regex::Regex;

use crate::config::jolene_root;
use crate::types::content::{ContentItem, ContentType};

/// A reference to a template variable found in a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateRef {
    /// The full matched text, e.g. `%{jolene:command:bar}`.
    pub full_match: String,
    /// The type: "command", "skill", "agent", or "prefix".
    pub ref_type: String,
    /// The name, if present (None for `%{jolene:prefix}`).
    pub name: Option<String>,
}

/// Context for resolving template variables.
pub struct TemplateContext {
    prefix: Option<String>,
    /// Maps (content_type_label, name) -> installed reference name.
    names: HashMap<(String, String), String>,
}

impl TemplateContext {
    /// Build a template context from a set of content items and an optional prefix.
    pub fn build(items: &[ContentItem], prefix: Option<&str>) -> TemplateContext {
        let mut names = HashMap::new();
        for item in items {
            let ref_name = item.installed_ref_name(prefix);
            names.insert(
                (item.content_type.label().to_string(), item.name.clone()),
                ref_name,
            );
        }
        TemplateContext {
            prefix: prefix.map(|s| s.to_string()),
            names,
        }
    }

    /// Resolve a template reference to its replacement string.
    fn resolve(&self, tref: &TemplateRef) -> Option<String> {
        if tref.ref_type == "prefix" {
            return Some(self.prefix.clone().unwrap_or_default());
        }
        if let Some(name) = &tref.name {
            self.names
                .get(&(tref.ref_type.clone(), name.clone()))
                .cloned()
        } else {
            None
        }
    }
}

/// Info about a file that contains template variables.
#[derive(Debug, Clone)]
pub struct TemplatedFile {
    /// Path relative to the content root (e.g. "commands/review.md" or "skills/analysis/SKILL.md").
    pub relative_path: PathBuf,
    /// The content type this file belongs to.
    pub content_type: ContentType,
    /// For skills, the skill name (directory name).
    pub skill_name: Option<String>,
}

fn template_regex() -> Regex {
    Regex::new(r"%\{jolene:(command|skill|agent|prefix)(?::([a-z0-9][a-z0-9-]*))?\}").unwrap()
}

/// Scan file content for template references.
pub fn scan_for_templates(content: &str) -> Vec<TemplateRef> {
    let re = template_regex();
    let mut refs = Vec::new();
    for cap in re.captures_iter(content) {
        let ref_type = cap[1].to_string();
        let name = cap.get(2).map(|m| m.as_str().to_string());
        refs.push(TemplateRef {
            full_match: cap[0].to_string(),
            ref_type,
            name,
        });
    }
    refs
}

/// Validate that all template references can be resolved.
pub fn validate_templates(
    refs: &[TemplateRef],
    ctx: &TemplateContext,
    file_path: &str,
) -> Result<()> {
    let known_types = ["command", "skill", "agent", "prefix"];
    for tref in refs {
        if !known_types.contains(&tref.ref_type.as_str()) {
            bail!(
                "Template error in {}: unknown type '{}'. Supported types: command, skill, agent, prefix",
                file_path,
                tref.ref_type
            );
        }
        if tref.ref_type == "prefix" {
            continue;
        }
        if tref.name.is_none() {
            bail!(
                "Template error in {}: %{{jolene:{}}} requires a name (e.g. %{{jolene:{}:some-name}})",
                file_path,
                tref.ref_type,
                tref.ref_type
            );
        }
        if ctx.resolve(tref).is_none() {
            bail!(
                "Template error in {}: references {} '{}' which does not exist in this package",
                file_path,
                tref.ref_type,
                tref.name.as_deref().unwrap_or("(none)")
            );
        }
    }
    Ok(())
}

/// Render content by substituting all template variables.
pub fn render_content(content: &str, ctx: &TemplateContext) -> Result<String> {
    let re = template_regex();
    let mut result = content.to_string();

    // Process all matches (iterate in reverse to preserve positions).
    let matches: Vec<_> = re.captures_iter(content).collect();
    for cap in matches.iter().rev() {
        let full = &cap[0];
        let ref_type = cap[1].to_string();
        let name = cap.get(2).map(|m| m.as_str().to_string());
        let tref = TemplateRef {
            full_match: full.to_string(),
            ref_type,
            name,
        };
        if let Some(replacement) = ctx.resolve(&tref) {
            // Replace the specific occurrence
            if let Some(pos) = result.rfind(full) {
                result.replace_range(pos..pos + full.len(), &replacement);
            }
        }
    }

    Ok(result)
}

/// Scan all content files and return those that contain template variables.
pub fn needs_templating(items: &[ContentItem], content_dir: &Path) -> Result<Vec<TemplatedFile>> {
    let mut templated = Vec::new();

    for item in items {
        match item.content_type {
            ContentType::Command | ContentType::Agent => {
                let rel = item.relative_path();
                let abs = content_dir.join(&rel);
                if abs.exists() {
                    let content = fs::read_to_string(&abs).with_context(|| {
                        format!("Failed to read {}", abs.display())
                    })?;
                    if !scan_for_templates(&content).is_empty() {
                        templated.push(TemplatedFile {
                            relative_path: rel,
                            content_type: item.content_type,
                            skill_name: None,
                        });
                    }
                }
            }
            ContentType::Skill => {
                let skill_md = content_dir
                    .join("skills")
                    .join(&item.name)
                    .join("SKILL.md");
                if skill_md.exists() {
                    let content = fs::read_to_string(&skill_md).with_context(|| {
                        format!("Failed to read {}", skill_md.display())
                    })?;
                    if !scan_for_templates(&content).is_empty() {
                        templated.push(TemplatedFile {
                            relative_path: PathBuf::from("skills")
                                .join(&item.name)
                                .join("SKILL.md"),
                            content_type: ContentType::Skill,
                            skill_name: Some(item.name.clone()),
                        });
                    }
                }
            }
        }
    }

    Ok(templated)
}

/// Render all templated files into the build directory.
///
/// For commands/agents: renders the .md file into `build_dir/{type}/{name}.md`.
/// For skills: creates a composite directory with the rendered SKILL.md and
/// symlinks for all other files/subdirectories.
pub fn build_templated_files(
    templated: &[TemplatedFile],
    content_dir: &Path,
    build_dir: &Path,
    ctx: &TemplateContext,
) -> Result<()> {
    // Clean and recreate build dir
    if build_dir.exists() {
        fs::remove_dir_all(build_dir)
            .with_context(|| format!("Failed to clean build dir {}", build_dir.display()))?;
    }

    for tf in templated {
        let src_path = content_dir.join(&tf.relative_path);
        let content = fs::read_to_string(&src_path)
            .with_context(|| format!("Failed to read {}", src_path.display()))?;

        let refs = scan_for_templates(&content);
        validate_templates(&refs, ctx, &tf.relative_path.to_string_lossy())?;

        let rendered = render_content(&content, ctx)?;

        match tf.content_type {
            ContentType::Command | ContentType::Agent => {
                let dest = build_dir.join(&tf.relative_path);
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&dest, rendered)
                    .with_context(|| format!("Failed to write {}", dest.display()))?;
            }
            ContentType::Skill => {
                let skill_name = tf.skill_name.as_deref().unwrap();
                let built_skill_dir = build_dir.join("skills").join(skill_name);
                fs::create_dir_all(&built_skill_dir)?;

                // Write rendered SKILL.md
                fs::write(built_skill_dir.join("SKILL.md"), rendered)
                    .with_context(|| {
                        format!(
                            "Failed to write SKILL.md for skill '{}'",
                            skill_name
                        )
                    })?;

                // Symlink all other entries in the source skill directory
                let src_skill_dir = content_dir.join("skills").join(skill_name);
                for entry in fs::read_dir(&src_skill_dir).with_context(|| {
                    format!("Failed to read skill dir {}", src_skill_dir.display())
                })? {
                    let entry = entry?;
                    let name = entry.file_name();
                    if name == "SKILL.md" {
                        continue; // Already rendered
                    }
                    let link_path = built_skill_dir.join(&name);
                    let target = entry.path();
                    unix_fs::symlink(&target, &link_path).with_context(|| {
                        format!(
                            "Failed to symlink {} -> {}",
                            link_path.display(),
                            target.display()
                        )
                    })?;
                }
            }
        }
    }

    Ok(())
}

/// Remove the build directory for a given store key.
pub fn clean_build_dir(store_key: &str) -> Result<()> {
    let root = jolene_root()?;
    let build_dir = root.join("built").join(store_key);
    if build_dir.exists() {
        fs::remove_dir_all(&build_dir)
            .with_context(|| format!("Failed to remove build dir {}", build_dir.display()))?;
    }
    Ok(())
}

/// Get the build directory path for a store key.
pub fn build_dir_for(store_key: &str) -> Result<PathBuf> {
    let root = jolene_root()?;
    Ok(root.join("built").join(store_key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn scan_finds_command_ref() {
        let content = "Use %{jolene:command:review} to review code.";
        let refs = scan_for_templates(content);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].ref_type, "command");
        assert_eq!(refs[0].name, Some("review".to_string()));
    }

    #[test]
    fn scan_finds_prefix_ref() {
        let content = "Prefix is %{jolene:prefix}.";
        let refs = scan_for_templates(content);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].ref_type, "prefix");
        assert_eq!(refs[0].name, None);
    }

    #[test]
    fn scan_finds_multiple_refs() {
        let content = "Use %{jolene:command:review} and %{jolene:skill:analysis} with %{jolene:prefix}";
        let refs = scan_for_templates(content);
        assert_eq!(refs.len(), 3);
    }

    #[test]
    fn scan_ignores_handlebars() {
        let content = "{{jolene:command:review}} is not a template";
        let refs = scan_for_templates(content);
        assert!(refs.is_empty());
    }

    #[test]
    fn scan_ignores_partial_match() {
        let content = "%{jolene:bad} is not valid";
        let refs = scan_for_templates(content);
        assert!(refs.is_empty());
    }

    #[test]
    fn render_with_prefix() {
        let items = vec![
            ContentItem::new(ContentType::Command, "review"),
            ContentItem::new(ContentType::Skill, "analysis"),
        ];
        let ctx = TemplateContext::build(&items, Some("xyz"));
        let input = "Use %{jolene:command:review} and %{jolene:skill:analysis}";
        let result = render_content(input, &ctx).unwrap();
        assert_eq!(result, "Use xyz--review and xyz--analysis");
    }

    #[test]
    fn render_without_prefix() {
        let items = vec![ContentItem::new(ContentType::Command, "review")];
        let ctx = TemplateContext::build(&items, None);
        let input = "Use %{jolene:command:review}";
        let result = render_content(input, &ctx).unwrap();
        assert_eq!(result, "Use review");
    }

    #[test]
    fn render_prefix_variable() {
        let items = vec![];
        let ctx = TemplateContext::build(&items, Some("xyz"));
        let input = "Prefix: %{jolene:prefix}";
        let result = render_content(input, &ctx).unwrap();
        assert_eq!(result, "Prefix: xyz");
    }

    #[test]
    fn render_prefix_variable_empty() {
        let items = vec![];
        let ctx = TemplateContext::build(&items, None);
        let input = "Prefix: %{jolene:prefix}";
        let result = render_content(input, &ctx).unwrap();
        assert_eq!(result, "Prefix: ");
    }

    #[test]
    fn validate_rejects_unknown_name() {
        let items = vec![ContentItem::new(ContentType::Command, "review")];
        let ctx = TemplateContext::build(&items, None);
        let refs = vec![TemplateRef {
            full_match: "%{jolene:command:nonexistent}".to_string(),
            ref_type: "command".to_string(),
            name: Some("nonexistent".to_string()),
        }];
        let result = validate_templates(&refs, &ctx, "test.md");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("nonexistent"));
    }

    #[test]
    fn validate_accepts_valid_refs() {
        let items = vec![
            ContentItem::new(ContentType::Command, "review"),
            ContentItem::new(ContentType::Skill, "analysis"),
        ];
        let ctx = TemplateContext::build(&items, Some("xyz"));
        let refs = vec![
            TemplateRef {
                full_match: "%{jolene:command:review}".to_string(),
                ref_type: "command".to_string(),
                name: Some("review".to_string()),
            },
            TemplateRef {
                full_match: "%{jolene:prefix}".to_string(),
                ref_type: "prefix".to_string(),
                name: None,
            },
        ];
        assert!(validate_templates(&refs, &ctx, "test.md").is_ok());
    }

    #[test]
    fn needs_templating_detects_templated_command() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("commands")).unwrap();
        fs::write(
            dir.path().join("commands/review.md"),
            "Use %{jolene:command:deploy} to deploy.",
        )
        .unwrap();
        fs::write(
            dir.path().join("commands/deploy.md"),
            "# Deploy\nNo templates here.",
        )
        .unwrap();

        let items = vec![
            ContentItem::new(ContentType::Command, "review"),
            ContentItem::new(ContentType::Command, "deploy"),
        ];

        let result = needs_templating(&items, dir.path()).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].relative_path,
            PathBuf::from("commands/review.md")
        );
    }

    #[test]
    fn needs_templating_detects_templated_skill() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("skills/analysis")).unwrap();
        fs::write(
            dir.path().join("skills/analysis/SKILL.md"),
            "Use %{jolene:command:review}",
        )
        .unwrap();

        let items = vec![
            ContentItem::new(ContentType::Skill, "analysis"),
            ContentItem::new(ContentType::Command, "review"),
        ];

        let result = needs_templating(&items, dir.path()).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].skill_name.as_deref() == Some("analysis"));
    }

    #[test]
    fn build_renders_command() {
        let src = TempDir::new().unwrap();
        let build = TempDir::new().unwrap();

        fs::create_dir_all(src.path().join("commands")).unwrap();
        fs::write(
            src.path().join("commands/review.md"),
            "Use %{jolene:command:deploy} to deploy.",
        )
        .unwrap();

        let items = vec![
            ContentItem::new(ContentType::Command, "review"),
            ContentItem::new(ContentType::Command, "deploy"),
        ];
        let ctx = TemplateContext::build(&items, Some("xyz"));

        let templated = vec![TemplatedFile {
            relative_path: PathBuf::from("commands/review.md"),
            content_type: ContentType::Command,
            skill_name: None,
        }];

        build_templated_files(&templated, src.path(), build.path(), &ctx).unwrap();

        let built = fs::read_to_string(build.path().join("commands/review.md")).unwrap();
        assert_eq!(built, "Use xyz--deploy to deploy.");
    }

    #[test]
    fn build_renders_skill_with_symlinks() {
        let src = TempDir::new().unwrap();
        let build = TempDir::new().unwrap();

        fs::create_dir_all(src.path().join("skills/analysis/references")).unwrap();
        fs::write(
            src.path().join("skills/analysis/SKILL.md"),
            "Use %{jolene:command:review}",
        )
        .unwrap();
        fs::write(
            src.path().join("skills/analysis/references/patterns.md"),
            "# Patterns",
        )
        .unwrap();

        let items = vec![
            ContentItem::new(ContentType::Skill, "analysis"),
            ContentItem::new(ContentType::Command, "review"),
        ];
        let ctx = TemplateContext::build(&items, Some("xyz"));

        let templated = vec![TemplatedFile {
            relative_path: PathBuf::from("skills/analysis/SKILL.md"),
            content_type: ContentType::Skill,
            skill_name: Some("analysis".to_string()),
        }];

        build_templated_files(&templated, src.path(), build.path(), &ctx).unwrap();

        // SKILL.md should be rendered
        let built = fs::read_to_string(build.path().join("skills/analysis/SKILL.md")).unwrap();
        assert_eq!(built, "Use xyz--review");

        // references/ should be a symlink back to source
        let refs_link = build.path().join("skills/analysis/references");
        assert!(refs_link.is_symlink());
        let target = fs::read_link(&refs_link).unwrap();
        assert_eq!(target, src.path().join("skills/analysis/references"));
    }
}
