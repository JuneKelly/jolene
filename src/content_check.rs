use std::path::Path;

use crate::output::Output;
use crate::types::content::{ContentItem, ContentType};

/// Result of checking a single skill directory.
pub struct SkillCheckReport {
    pub skill_name: String,
    pub missing_fields: Vec<&'static str>,
    pub compatibility: Option<String>,
    pub non_executable_scripts: Vec<String>,
}

/// Result of checking a single agent definition file.
pub struct AgentCheckReport {
    pub agent_name: String,
    pub missing_fields: Vec<&'static str>,
}

/// Parsed SKILL.md frontmatter fields.
struct Frontmatter {
    name: Option<String>,
    description: Option<String>,
    compatibility: Option<String>,
}

/// Parse YAML frontmatter from a SKILL.md file's content.
///
/// Extracts `name`, `description`, and `compatibility` fields from the
/// YAML block between `---` delimiters at the top of the file. Uses simple
/// line-by-line parsing — no YAML library needed for these flat key-value pairs.
fn parse_frontmatter(content: &str) -> Frontmatter {
    let mut name = None;
    let mut description = None;
    let mut compatibility = None;

    let mut lines = content.lines();

    // First line must be exactly "---"
    match lines.next() {
        Some(line) if line.trim() == "---" => {}
        _ => return Frontmatter { name, description, compatibility },
    }

    // Read lines until the closing "---"
    for line in lines {
        if line.trim() == "---" {
            break;
        }

        // Split on first ": " to get key and value.
        // Bail on any line that doesn't match key: value — return what we have so far.
        let Some((key, value)) = line.split_once(": ") else {
            break;
        };

        let key = key.trim();
        let value = strip_quotes(value.trim());

        match key {
            "name" => name = Some(value),
            "description" => description = Some(value),
            "compatibility" => {
                let mut v = value;
                if v.len() > 500 {
                    v = v.chars().take(500).collect();
                }
                compatibility = Some(v);
            }
            _ => {}
        }
    }

    Frontmatter { name, description, compatibility }
}

/// Strip surrounding single or double quotes from a string value.
fn strip_quotes(s: &str) -> String {
    if s.len() < 2 {
        return s.to_string();
    }
    if (s.starts_with('"') && s.ends_with('"'))
        || (s.starts_with('\'') && s.ends_with('\''))
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// Check all skills under `content_dir` for frontmatter quality and script executability.
pub fn check_skills(content_dir: &Path, skill_names: &[String]) -> Vec<SkillCheckReport> {
    let mut reports = Vec::new();

    for name in skill_names {
        let skill_md = content_dir.join("skills").join(name).join("SKILL.md");

        let mut missing_fields = Vec::new();
        let mut compatibility = None;

        // Parse frontmatter if readable
        if let Ok(content) = std::fs::read_to_string(&skill_md) {
            let fm = parse_frontmatter(&content);

            if fm.name.is_none() {
                missing_fields.push("name");
            }
            if fm.description.is_none() {
                missing_fields.push("description");
            }
            compatibility = fm.compatibility;
        } else {
            // Can't read the file — warn about both fields
            missing_fields.push("name");
            missing_fields.push("description");
        }

        let non_executable_scripts = check_script_executability(content_dir, name);

        reports.push(SkillCheckReport {
            skill_name: name.clone(),
            missing_fields,
            compatibility,
            non_executable_scripts,
        });
    }

    reports
}

/// Check that files in `skills/{name}/scripts/` are executable (Unix only).
#[cfg(unix)]
fn check_script_executability(content_dir: &Path, skill_name: &str) -> Vec<String> {
    use std::os::unix::fs::PermissionsExt;

    let scripts_dir = content_dir.join("skills").join(skill_name).join("scripts");
    if !scripts_dir.is_dir() {
        return Vec::new();
    }

    let mut non_executable = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&scripts_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if let Ok(meta) = path.metadata() {
                if meta.permissions().mode() & 0o111 == 0 {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        non_executable.push(name.to_string());
                    }
                }
            }
        }
    }

    non_executable.sort();
    non_executable
}

#[cfg(not(unix))]
fn check_script_executability(_content_dir: &Path, _skill_name: &str) -> Vec<String> {
    Vec::new()
}

/// Print advisory warnings from skill check reports.
pub fn print_warnings(reports: &[SkillCheckReport], out: &Output, indent: &str) {
    for report in reports {
        if let Some(ref compat) = report.compatibility {
            out.print(format!(
                "{}Note: skill '{}' compatibility: {}",
                indent, report.skill_name, compat
            ));
        }
        for field in &report.missing_fields {
            out.print(format!(
                "{}Warning: skill '{}' SKILL.md is missing '{}'",
                indent, report.skill_name, field
            ));
        }
        for script in &report.non_executable_scripts {
            out.print(format!(
                "{}Warning: skill '{}' has non-executable script: scripts/{}",
                indent, report.skill_name, script
            ));
        }
    }
}

/// Run advisory skill quality checks for any skills in the content items.
///
/// Filters items to skills, runs frontmatter and executability checks,
/// and prints warnings. This is a convenience wrapper used by install and update.
pub fn check_and_warn_skills(items: &[ContentItem], content_dir: &Path, out: &Output, indent: &str) {
    let skill_names: Vec<String> = items
        .iter()
        .filter(|i| i.content_type == ContentType::Skill)
        .map(|i| i.name.clone())
        .collect();
    if !skill_names.is_empty() {
        let reports = check_skills(content_dir, &skill_names);
        print_warnings(&reports, out, indent);
    }
}

/// Check all agents under `content_dir` for frontmatter quality.
pub fn check_agents(content_dir: &Path, agent_names: &[String]) -> Vec<AgentCheckReport> {
    let mut reports = Vec::new();

    for name in agent_names {
        let agent_md = content_dir.join("agents").join(format!("{}.md", name));

        let mut missing_fields = Vec::new();

        if let Ok(content) = std::fs::read_to_string(&agent_md) {
            let fm = parse_frontmatter(&content);

            if fm.name.is_none() {
                missing_fields.push("name");
            }
            if fm.description.is_none() {
                missing_fields.push("description");
            }
        } else {
            missing_fields.push("name");
            missing_fields.push("description");
        }

        reports.push(AgentCheckReport {
            agent_name: name.clone(),
            missing_fields,
        });
    }

    reports
}

/// Print advisory warnings from agent check reports.
pub fn print_agent_warnings(reports: &[AgentCheckReport], out: &Output, indent: &str) {
    for report in reports {
        for field in &report.missing_fields {
            out.print(format!(
                "{}Warning: agent '{}' is missing '{}'",
                indent, report.agent_name, field
            ));
        }
    }
}

/// Run advisory agent quality checks for any agents in the content items.
///
/// Filters items to agents, runs frontmatter checks, and prints warnings.
/// This is a convenience wrapper used by install and update.
pub fn check_and_warn_agents(items: &[ContentItem], content_dir: &Path, out: &Output, indent: &str) {
    let agent_names: Vec<String> = items
        .iter()
        .filter(|i| i.content_type == ContentType::Agent)
        .map(|i| i.name.clone())
        .collect();
    if !agent_names.is_empty() {
        let reports = check_agents(content_dir, &agent_names);
        print_agent_warnings(&reports, out, indent);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn parse_frontmatter_all_fields() {
        let content = "---\nname: my-skill\ndescription: A useful skill\ncompatibility: Requires git and docker\n---\n\n# My Skill\n";
        let fm = parse_frontmatter(content);
        assert_eq!(fm.name.as_deref(), Some("my-skill"));
        assert_eq!(fm.description.as_deref(), Some("A useful skill"));
        assert_eq!(fm.compatibility.as_deref(), Some("Requires git and docker"));
    }

    #[test]
    fn parse_frontmatter_missing_fields() {
        let content = "---\nname: my-skill\n---\n\nBody text\n";
        let fm = parse_frontmatter(content);
        assert_eq!(fm.name.as_deref(), Some("my-skill"));
        assert!(fm.description.is_none());
        assert!(fm.compatibility.is_none());
    }

    #[test]
    fn parse_frontmatter_no_frontmatter() {
        let content = "# My Skill\n\nJust a body, no frontmatter.\n";
        let fm = parse_frontmatter(content);
        assert!(fm.name.is_none());
        assert!(fm.description.is_none());
        assert!(fm.compatibility.is_none());
    }

    #[test]
    fn parse_frontmatter_empty_file() {
        let fm = parse_frontmatter("");
        assert!(fm.name.is_none());
        assert!(fm.description.is_none());
        assert!(fm.compatibility.is_none());
    }

    #[test]
    fn parse_frontmatter_quoted_values() {
        let content = "---\nname: \"quoted-name\"\ndescription: 'single quoted'\n---\n";
        let fm = parse_frontmatter(content);
        assert_eq!(fm.name.as_deref(), Some("quoted-name"));
        assert_eq!(fm.description.as_deref(), Some("single quoted"));
    }

    #[test]
    fn parse_frontmatter_compatibility_truncated_at_500() {
        let long_value = "x".repeat(600);
        let content = format!("---\ncompatibility: {}\n---\n", long_value);
        let fm = parse_frontmatter(&content);
        assert_eq!(fm.compatibility.as_deref().unwrap().len(), 500);
    }

    #[test]
    fn parse_frontmatter_bails_on_malformed_line() {
        let content = "---\nname: my-skill\nthis is garbage\ndescription: should not appear\n---\n";
        let fm = parse_frontmatter(content);
        assert_eq!(fm.name.as_deref(), Some("my-skill"));
        assert!(fm.description.is_none(), "description after malformed line should not be parsed");
        assert!(fm.compatibility.is_none());
    }

    #[test]
    fn parse_frontmatter_bails_on_blank_line() {
        let content = "---\nname: my-skill\n\ndescription: should not appear\n---\n";
        let fm = parse_frontmatter(content);
        assert_eq!(fm.name.as_deref(), Some("my-skill"));
        assert!(fm.description.is_none(), "description after blank line should not be parsed");
    }

    #[test]
    fn check_skills_warns_missing_name_description() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("skills/my-skill")).unwrap();
        fs::write(dir.path().join("skills/my-skill/SKILL.md"), "# No frontmatter").unwrap();

        let reports = check_skills(dir.path(), &["my-skill".to_string()]);
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].missing_fields, vec!["name", "description"]);
        assert!(reports[0].compatibility.is_none());
    }

    #[test]
    fn check_skills_reports_compatibility() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("skills/my-skill")).unwrap();
        fs::write(
            dir.path().join("skills/my-skill/SKILL.md"),
            "---\nname: my-skill\ndescription: A skill\ncompatibility: Requires git\n---\n",
        )
        .unwrap();

        let reports = check_skills(dir.path(), &["my-skill".to_string()]);
        assert_eq!(reports.len(), 1);
        assert!(reports[0].missing_fields.is_empty());
        assert_eq!(reports[0].compatibility.as_deref(), Some("Requires git"));
    }

    #[cfg(unix)]
    #[test]
    fn check_skills_non_executable_script() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new().unwrap();
        let skill_dir = dir.path().join("skills/my-skill");
        fs::create_dir_all(skill_dir.join("scripts")).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "---\nname: my-skill\ndescription: A skill\n---\n").unwrap();
        let script = skill_dir.join("scripts/analyze.sh");
        fs::write(&script, "#!/bin/sh\necho hi").unwrap();
        fs::set_permissions(&script, std::fs::Permissions::from_mode(0o644)).unwrap();

        let reports = check_skills(dir.path(), &["my-skill".to_string()]);
        assert_eq!(reports[0].non_executable_scripts, vec!["analyze.sh"]);
    }

    #[cfg(unix)]
    #[test]
    fn check_skills_executable_script_clean() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new().unwrap();
        let skill_dir = dir.path().join("skills/my-skill");
        fs::create_dir_all(skill_dir.join("scripts")).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "---\nname: my-skill\ndescription: A skill\n---\n").unwrap();
        let script = skill_dir.join("scripts/run.sh");
        fs::write(&script, "#!/bin/sh\necho hi").unwrap();
        fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

        let reports = check_skills(dir.path(), &["my-skill".to_string()]);
        assert!(reports[0].non_executable_scripts.is_empty());
    }

    #[test]
    fn check_skills_no_scripts_dir() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("skills/my-skill")).unwrap();
        fs::write(
            dir.path().join("skills/my-skill/SKILL.md"),
            "---\nname: my-skill\ndescription: A skill\n---\n",
        )
        .unwrap();

        let reports = check_skills(dir.path(), &["my-skill".to_string()]);
        assert!(reports[0].non_executable_scripts.is_empty());
    }

    #[test]
    fn check_agents_warns_missing_name_description() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("agents")).unwrap();
        fs::write(
            dir.path().join("agents/reviewer.md"),
            "# Just a body, no frontmatter\n",
        )
        .unwrap();

        let reports = check_agents(dir.path(), &["reviewer".to_string()]);
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].agent_name, "reviewer");
        assert_eq!(reports[0].missing_fields, vec!["name", "description"]);
    }

    #[test]
    fn check_agents_clean_with_both_fields() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("agents")).unwrap();
        fs::write(
            dir.path().join("agents/reviewer.md"),
            "---\nname: reviewer\ndescription: Reviews code for quality\n---\n\nYou are a code reviewer.\n",
        )
        .unwrap();

        let reports = check_agents(dir.path(), &["reviewer".to_string()]);
        assert_eq!(reports.len(), 1);
        assert!(reports[0].missing_fields.is_empty());
    }

    #[test]
    fn check_agents_warns_missing_description_only() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("agents")).unwrap();
        fs::write(
            dir.path().join("agents/planner.md"),
            "---\nname: planner\n---\n\nYou are a planner.\n",
        )
        .unwrap();

        let reports = check_agents(dir.path(), &["planner".to_string()]);
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].missing_fields, vec!["description"]);
    }

    #[test]
    fn check_agents_unreadable_file_warns_both() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("agents")).unwrap();
        // Don't create the file — simulates unreadable

        let reports = check_agents(dir.path(), &["missing-agent".to_string()]);
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].missing_fields, vec!["name", "description"]);
    }
}
