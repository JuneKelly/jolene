use std::collections::HashSet;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::types::content::{ContentItem, ContentType};
use crate::types::manifest::Manifest;

/// Validate that a content name is a safe, simple filename component.
///
/// Rejects names containing path separators (`/`, `\`), parent-directory
/// traversal (`..`), or the current-directory marker (`.`). This prevents
/// path traversal when names are joined into filesystem paths.
pub fn validate_content_name(name: &str, kind: &str) -> Result<()> {
    if name.is_empty() {
        bail!("Invalid jolene.toml: {} name must not be empty", kind);
    }
    if name.contains('/') || name.contains('\\') {
        bail!(
            "Invalid jolene.toml: {} name '{}' must not contain path separators",
            kind,
            name
        );
    }
    if name == "." || name == ".." || name.contains("..") {
        bail!(
            "Invalid jolene.toml: {} name '{}' must not contain path traversal",
            kind,
            name
        );
    }
    Ok(())
}

/// Validate a prefix string for use in content name prefixing.
///
/// A valid prefix must:
/// - Be 1-64 characters long
/// - Contain only lowercase ASCII letters, digits, and hyphens (`[a-z0-9-]`)
/// - Not start or end with a hyphen
/// - Not contain consecutive hyphens (`--`), since `--` is the prefix separator
pub fn validate_prefix(s: &str) -> Result<()> {
    if s.is_empty() {
        bail!("prefix must not be empty");
    }
    if s.len() > 64 {
        bail!("prefix must be at most 64 characters (got {})", s.len());
    }
    if s.starts_with('-') {
        bail!("prefix must not start with a hyphen");
    }
    if s.ends_with('-') {
        bail!("prefix must not end with a hyphen");
    }
    if s.contains("--") {
        bail!("prefix must not contain consecutive hyphens");
    }
    if !s
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        bail!("prefix must contain only lowercase letters, digits, and hyphens");
    }
    Ok(())
}

/// Resolve the effective prefix from CLI flags and manifest.
///
/// Priority: `--no-prefix` → None, `--prefix X` → Some(X), manifest prefix, else None.
pub fn resolve_prefix(
    cli_prefix: Option<&str>,
    cli_no_prefix: bool,
    manifest_prefix: Option<&str>,
) -> Result<Option<String>> {
    if cli_no_prefix {
        return Ok(None);
    }
    if let Some(p) = cli_prefix {
        validate_prefix(p)?;
        return Ok(Some(p.to_string()));
    }
    match manifest_prefix {
        Some(p) => {
            validate_prefix(p)?;
            Ok(Some(p.to_string()))
        }
        None => Ok(None),
    }
}

pub fn load_manifest(clone_root: &Path) -> Result<Manifest> {
    let path = clone_root.join("jolene.toml");

    if !path.exists() {
        bail!(
            "is missing jolene.toml\n  Every jolene bundle must include a jolene.toml manifest.\n  See https://github.com/jolene-pm/jolene#bundle-format"
        );
    }

    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;

    let manifest: Manifest = toml::from_str(&text)
        .with_context(|| format!("Invalid jolene.toml in {}", clone_root.display()))?;

    Ok(manifest)
}

pub fn validate_manifest(manifest: &Manifest, clone_root: &Path) -> Result<()> {
    if let Some(ref prefix) = manifest.bundle.prefix {
        validate_prefix(prefix).with_context(|| "Invalid prefix in jolene.toml [bundle] table")?;
    }

    if manifest.content.is_empty() {
        bail!(
            "has no installable content.\n  Expected at least one of: commands/, skills/, agents/"
        );
    }

    for name in &manifest.content.commands {
        validate_content_name(name, "command")?;
        let path = clone_root.join("commands").join(format!("{}.md", name));
        if !path.exists() {
            bail!(
                "Invalid jolene.toml: declared command '{}' not found at commands/{}.md",
                name,
                name
            );
        }
    }

    for name in &manifest.content.skills {
        validate_content_name(name, "skill")?;
        let skill_dir = clone_root.join("skills").join(name);
        if !skill_dir.exists() {
            bail!(
                "Invalid jolene.toml: declared skill '{}' not found at skills/{}/",
                name,
                name
            );
        }
        let skill_md = skill_dir.join("SKILL.md");
        if !skill_md.exists() {
            bail!("Invalid jolene.toml: skill '{}' is missing SKILL.md", name);
        }
    }

    for name in &manifest.content.agents {
        validate_content_name(name, "agent")?;
        let path = clone_root.join("agents").join(format!("{}.md", name));
        if !path.exists() {
            bail!(
                "Invalid jolene.toml: declared agent '{}' not found at agents/{}.md",
                name,
                name
            );
        }
    }

    let exclude = manifest.template_exclude();
    if !exclude.is_empty() {
        let all_names: HashSet<&str> = manifest.content.commands.iter()
            .chain(manifest.content.skills.iter())
            .chain(manifest.content.agents.iter())
            .map(String::as_str)
            .collect();
        for name in exclude {
            if !all_names.contains(name.as_str()) {
                let declared: Vec<String> = manifest.content.commands.iter()
                    .map(|n| format!("{} (command)", n))
                    .chain(manifest.content.skills.iter().map(|n| format!("{} (skill)", n)))
                    .chain(manifest.content.agents.iter().map(|n| format!("{} (agent)", n)))
                    .collect();
                bail!(
                    "Invalid jolene.toml: [template.exclude] name '{}' is not declared in [content].\n  Declared content: {}",
                    name,
                    declared.join(", ")
                );
            }
        }
    }

    Ok(())
}

pub fn collect_content_items(manifest: &Manifest) -> Vec<ContentItem> {
    let mut items = Vec::new();

    for name in &manifest.content.commands {
        items.push(ContentItem::new(ContentType::Command, name));
    }
    for name in &manifest.content.skills {
        items.push(ContentItem::new(ContentType::Skill, name));
    }
    for name in &manifest.content.agents {
        items.push(ContentItem::new(ContentType::Agent, name));
    }

    items
}

#[cfg(test)]
mod tests {
    use super::*;

    // validate_content_name tests

    #[test]
    fn content_name_valid_simple() {
        assert!(validate_content_name("review", "command").is_ok());
    }

    #[test]
    fn content_name_valid_with_hyphens() {
        assert!(validate_content_name("code-review", "command").is_ok());
    }

    #[test]
    fn content_name_rejects_empty() {
        let err = validate_content_name("", "command").unwrap_err();
        assert!(err.to_string().contains("must not be empty"));
    }

    #[test]
    fn content_name_rejects_forward_slash() {
        let err = validate_content_name("../etc/passwd", "command").unwrap_err();
        assert!(err.to_string().contains("path separators"));
    }

    #[test]
    fn content_name_rejects_backslash() {
        let err = validate_content_name("foo\\bar", "skill").unwrap_err();
        assert!(err.to_string().contains("path separators"));
    }

    #[test]
    fn content_name_rejects_dot_dot() {
        let err = validate_content_name("..", "agent").unwrap_err();
        assert!(err.to_string().contains("path traversal"));
    }

    #[test]
    fn content_name_rejects_dot() {
        let err = validate_content_name(".", "command").unwrap_err();
        assert!(err.to_string().contains("path traversal"));
    }

    #[test]
    fn content_name_rejects_embedded_dot_dot() {
        let err = validate_content_name("foo..bar", "command").unwrap_err();
        assert!(err.to_string().contains("path traversal"));
    }

    // validate_prefix tests

    #[test]
    fn valid_simple() {
        assert!(validate_prefix("abc").is_ok());
    }

    #[test]
    fn valid_with_hyphens() {
        assert!(validate_prefix("my-prefix").is_ok());
    }

    #[test]
    fn valid_single_char() {
        assert!(validate_prefix("a").is_ok());
    }

    #[test]
    fn valid_alphanumeric() {
        assert!(validate_prefix("a1").is_ok());
    }

    #[test]
    fn valid_multi_hyphen_segments() {
        assert!(validate_prefix("foo-bar-baz").is_ok());
    }

    #[test]
    fn valid_all_digits() {
        assert!(validate_prefix("123").is_ok());
    }

    #[test]
    fn invalid_empty() {
        let err = validate_prefix("").unwrap_err();
        assert!(err.to_string().contains("must not be empty"));
    }

    #[test]
    fn invalid_leading_hyphen() {
        let err = validate_prefix("-abc").unwrap_err();
        assert!(err.to_string().contains("must not start with a hyphen"));
    }

    #[test]
    fn invalid_trailing_hyphen() {
        let err = validate_prefix("abc-").unwrap_err();
        assert!(err.to_string().contains("must not end with a hyphen"));
    }

    #[test]
    fn invalid_consecutive_hyphens() {
        let err = validate_prefix("a--b").unwrap_err();
        assert!(err.to_string().contains("consecutive hyphens"));
    }

    #[test]
    fn invalid_uppercase() {
        let err = validate_prefix("ABC").unwrap_err();
        assert!(
            err.to_string()
                .contains("lowercase letters, digits, and hyphens")
        );
    }

    #[test]
    fn invalid_underscore() {
        let err = validate_prefix("foo_bar").unwrap_err();
        assert!(
            err.to_string()
                .contains("lowercase letters, digits, and hyphens")
        );
    }

    #[test]
    fn invalid_space() {
        let err = validate_prefix("foo bar").unwrap_err();
        assert!(
            err.to_string()
                .contains("lowercase letters, digits, and hyphens")
        );
    }

    #[test]
    fn invalid_too_long() {
        let long = "a".repeat(65);
        let err = validate_prefix(&long).unwrap_err();
        assert!(err.to_string().contains("at most 64 characters"));
    }

    #[test]
    fn valid_exactly_64_chars() {
        let s = "a".repeat(64);
        assert!(validate_prefix(&s).is_ok());
    }

    // resolve_prefix tests

    #[test]
    fn resolve_cli_prefix() {
        let result = resolve_prefix(Some("abc"), false, None).unwrap();
        assert_eq!(result, Some("abc".to_string()));
    }

    #[test]
    fn resolve_no_prefix_flag() {
        let result = resolve_prefix(None, true, Some("jb")).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn resolve_manifest_prefix_when_no_cli_flags() {
        let result = resolve_prefix(None, false, Some("jb")).unwrap();
        assert_eq!(result, Some("jb".to_string()));
    }

    #[test]
    fn resolve_none_when_nothing_set() {
        let result = resolve_prefix(None, false, None).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn resolve_cli_prefix_overrides_manifest() {
        let result = resolve_prefix(Some("abc"), false, Some("jb")).unwrap();
        assert_eq!(result, Some("abc".to_string()));
    }

    #[test]
    fn resolve_no_prefix_overrides_manifest() {
        let result = resolve_prefix(None, true, Some("jb")).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn resolve_cli_prefix_validates() {
        let result = resolve_prefix(Some("INVALID"), false, None);
        assert!(result.is_err());
    }

    #[test]
    fn resolve_manifest_prefix_validates() {
        let result = resolve_prefix(None, false, Some("INVALID"));
        assert!(result.is_err());
    }

    // validate_manifest [template.exclude] tests

    use std::collections::BTreeMap;
    use crate::types::manifest::{Bundle, ContentDecl, Manifest, TemplateDecl};

    fn make_manifest(commands: &[&str], exclude: &[&str]) -> Manifest {
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
            content: ContentDecl {
                commands: commands.iter().map(|s| s.to_string()).collect(),
                skills: vec![],
                agents: vec![],
            },
            template: if exclude.is_empty() {
                None
            } else {
                Some(TemplateDecl {
                    vars: BTreeMap::new(),
                    exclude: exclude.iter().map(|s| s.to_string()).collect(),
                })
            },
        }
    }

    fn setup_command(dir: &std::path::Path, name: &str) {
        let commands_dir = dir.join("commands");
        std::fs::create_dir_all(&commands_dir).unwrap();
        std::fs::write(commands_dir.join(format!("{}.md", name)), "# content").unwrap();
    }

    #[test]
    fn validate_manifest_exclude_known_command_ok() {
        let dir = tempfile::tempdir().unwrap();
        setup_command(dir.path(), "review");
        let m = make_manifest(&["review"], &["review"]);
        assert!(validate_manifest(&m, dir.path()).is_ok());
    }

    #[test]
    fn validate_manifest_exclude_empty_ok() {
        let dir = tempfile::tempdir().unwrap();
        setup_command(dir.path(), "review");
        let m = make_manifest(&["review"], &[]);
        assert!(validate_manifest(&m, dir.path()).is_ok());
    }

    #[test]
    fn validate_manifest_exclude_unknown_name_errors() {
        let dir = tempfile::tempdir().unwrap();
        setup_command(dir.path(), "review");
        let m = make_manifest(&["review"], &["typo"]);
        let err = validate_manifest(&m, dir.path()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("[template.exclude]"), "expected [template.exclude] in: {msg}");
        assert!(msg.contains("typo"), "expected 'typo' in: {msg}");
    }
}
