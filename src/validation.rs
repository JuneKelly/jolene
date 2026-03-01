use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::types::content::{ContentItem, ContentType};
use crate::types::manifest::Manifest;

pub fn load_manifest(clone_root: &Path) -> Result<Manifest> {
    let path = clone_root.join("jolene.toml");

    if !path.exists() {
        bail!(
            "is missing jolene.toml\n  Every jolene package must include a jolene.toml manifest.\n  See https://github.com/jolene-pm/jolene#package-format"
        );
    }

    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;

    let manifest: Manifest = toml::from_str(&text)
        .with_context(|| format!("Invalid jolene.toml in {}", clone_root.display()))?;

    Ok(manifest)
}

pub fn validate_manifest(manifest: &Manifest, clone_root: &Path) -> Result<()> {
    if manifest.content.is_empty() {
        bail!(
            "has no installable content.\n  Expected at least one of: commands/, skills/, agents/"
        );
    }

    for name in &manifest.content.commands {
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
            bail!(
                "Invalid jolene.toml: skill '{}' is missing SKILL.md",
                name
            );
        }
    }

    for name in &manifest.content.agents {
        let path = clone_root.join("agents").join(format!("{}.md", name));
        if !path.exists() {
            bail!(
                "Invalid jolene.toml: declared agent '{}' not found at agents/{}.md",
                name,
                name
            );
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
