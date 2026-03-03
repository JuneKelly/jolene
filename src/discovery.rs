use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::types::content::{ContentItem, ContentType};

/// Discover installable content from a plugin directory by scanning the filesystem.
///
/// Looks for:
/// - `commands/*.md` → Command items
/// - `skills/*/SKILL.md` → Skill items (requires SKILL.md to exist)
/// - `agents/*.md` → Agent items
pub fn discover_content(plugin_dir: &Path) -> Result<Vec<ContentItem>> {
    let mut items = Vec::new();

    // Commands: each .md file in commands/
    let commands_dir = plugin_dir.join("commands");
    if commands_dir.is_dir() {
        for entry in std::fs::read_dir(&commands_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|e| e == "md") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    items.push(ContentItem::new(ContentType::Command, stem));
                }
            }
        }
    }

    // Skills: each subdirectory of skills/ that contains SKILL.md
    let skills_dir = plugin_dir.join("skills");
    if skills_dir.is_dir() {
        for entry in std::fs::read_dir(&skills_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() && path.join("SKILL.md").exists() {
                if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                    items.push(ContentItem::new(ContentType::Skill, name));
                }
            }
        }
    }

    // Agents: each .md file in agents/
    let agents_dir = plugin_dir.join("agents");
    if agents_dir.is_dir() {
        for entry in std::fs::read_dir(&agents_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|e| e == "md") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    items.push(ContentItem::new(ContentType::Agent, stem));
                }
            }
        }
    }

    // Sort for deterministic output
    items.sort_by(|a, b| {
        a.content_type
            .dir_name()
            .cmp(b.content_type.dir_name())
            .then_with(|| a.name.cmp(&b.name))
    });

    Ok(items)
}

/// Resolve a plugin subdirectory within a clone root, with path traversal protection.
///
/// If `plugin_path` is `Some`, joins it to `clone_root`, canonicalizes both paths,
/// and verifies the result stays within the clone root. Returns the clone root itself
/// if `plugin_path` is `None`.
pub fn resolve_plugin_dir(clone_root: &Path, plugin_path: Option<&str>) -> Result<PathBuf> {
    match plugin_path {
        Some(subdir) => {
            let dir = clone_root.join(subdir);
            let dir = dir
                .canonicalize()
                .with_context(|| format!("Failed to resolve plugin path '{}'", subdir))?;
            let root = clone_root
                .canonicalize()
                .with_context(|| "Failed to canonicalize clone root".to_string())?;
            if !dir.starts_with(&root) {
                bail!("Plugin path '{}' escapes the clone directory", subdir);
            }
            Ok(dir)
        }
        None => Ok(clone_root.to_path_buf()),
    }
}

/// Like `resolve_plugin_dir` but returns `None` instead of erroring on failure.
/// Useful for display-only contexts where a missing or invalid path is not fatal.
pub fn resolve_plugin_dir_lossy(clone_root: &Path, subdir: &str) -> Option<PathBuf> {
    let cleaned = subdir.strip_prefix("./").unwrap_or(subdir);
    let dir = clone_root.join(cleaned).canonicalize().ok()?;
    let root = clone_root.canonicalize().ok()?;
    if !dir.starts_with(&root) {
        return None;
    }
    Some(dir)
}

/// Human-readable summary of discovered content, e.g. "2 commands, 1 skill".
pub fn content_summary(items: &[ContentItem]) -> String {
    let commands = items
        .iter()
        .filter(|i| i.content_type == ContentType::Command)
        .count();
    let skills = items
        .iter()
        .filter(|i| i.content_type == ContentType::Skill)
        .count();
    let agents = items
        .iter()
        .filter(|i| i.content_type == ContentType::Agent)
        .count();

    let mut parts = Vec::new();
    if commands > 0 {
        parts.push(format!(
            "{} {}",
            commands,
            if commands == 1 { "command" } else { "commands" }
        ));
    }
    if skills > 0 {
        parts.push(format!(
            "{} {}",
            skills,
            if skills == 1 { "skill" } else { "skills" }
        ));
    }
    if agents > 0 {
        parts.push(format!(
            "{} {}",
            agents,
            if agents == 1 { "agent" } else { "agents" }
        ));
    }
    if parts.is_empty() {
        "no content".to_string()
    } else {
        parts.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_plugin_dir() -> TempDir {
        let dir = TempDir::new().unwrap();

        // commands/
        fs::create_dir_all(dir.path().join("commands")).unwrap();
        fs::write(dir.path().join("commands/review.md"), "# Review").unwrap();
        fs::write(dir.path().join("commands/deploy.md"), "# Deploy").unwrap();

        // skills/ with SKILL.md
        fs::create_dir_all(dir.path().join("skills/analysis")).unwrap();
        fs::write(dir.path().join("skills/analysis/SKILL.md"), "# Analysis").unwrap();

        // skills/ without SKILL.md (should be ignored)
        fs::create_dir_all(dir.path().join("skills/incomplete")).unwrap();

        // agents/
        fs::create_dir_all(dir.path().join("agents")).unwrap();
        fs::write(dir.path().join("agents/planner.md"), "# Planner").unwrap();

        dir
    }

    #[test]
    fn discovers_commands() {
        let dir = setup_plugin_dir();
        let items = discover_content(dir.path()).unwrap();
        let commands: Vec<_> = items
            .iter()
            .filter(|i| i.content_type == ContentType::Command)
            .map(|i| i.name.as_str())
            .collect();
        assert!(commands.contains(&"review"));
        assert!(commands.contains(&"deploy"));
        assert_eq!(commands.len(), 2);
    }

    #[test]
    fn discovers_skills_with_skill_md_only() {
        let dir = setup_plugin_dir();
        let items = discover_content(dir.path()).unwrap();
        let skills: Vec<_> = items
            .iter()
            .filter(|i| i.content_type == ContentType::Skill)
            .map(|i| i.name.as_str())
            .collect();
        assert_eq!(skills, vec!["analysis"]);
    }

    #[test]
    fn discovers_agents() {
        let dir = setup_plugin_dir();
        let items = discover_content(dir.path()).unwrap();
        let agents: Vec<_> = items
            .iter()
            .filter(|i| i.content_type == ContentType::Agent)
            .map(|i| i.name.as_str())
            .collect();
        assert_eq!(agents, vec!["planner"]);
    }

    #[test]
    fn empty_dir_returns_no_content() {
        let dir = TempDir::new().unwrap();
        let items = discover_content(dir.path()).unwrap();
        assert!(items.is_empty());
    }

    #[test]
    fn content_summary_formats_correctly() {
        let items = vec![
            ContentItem::new(ContentType::Command, "review"),
            ContentItem::new(ContentType::Command, "deploy"),
            ContentItem::new(ContentType::Skill, "analysis"),
            ContentItem::new(ContentType::Agent, "planner"),
        ];
        assert_eq!(content_summary(&items), "2 commands, 1 skill, 1 agent");
    }

    #[test]
    fn content_summary_empty() {
        assert_eq!(content_summary(&[]), "no content");
    }

    #[test]
    fn ignores_non_md_files_in_commands() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("commands")).unwrap();
        fs::write(dir.path().join("commands/review.md"), "# Review").unwrap();
        fs::write(dir.path().join("commands/readme.txt"), "not a command").unwrap();
        let items = discover_content(dir.path()).unwrap();
        assert_eq!(items.len(), 1);
    }
}
