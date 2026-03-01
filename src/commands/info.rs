use anyhow::{bail, Result};

use crate::config::clone_root_for;
use crate::output::Output;
use crate::state;
use crate::validation::load_manifest;

pub fn run(package: &str, out: &Output) -> Result<()> {
    let app_state = state::load()?;

    let pkg = state::find_package(&app_state, package)?;
    let pkg = match pkg {
        Some(p) => p,
        None => bail!("Package '{}' is not installed.", package),
    };

    let short = &pkg.commit[..pkg.commit.len().min(7)];

    out.print(format!("{}", pkg.source));
    out.print(format!("  Source:  {}", pkg.source_kind));
    if !pkg.clone_url.is_empty() {
        out.print(format!("  URL:     {}", pkg.clone_url));
    }
    out.print(format!("  Branch:  {}", pkg.branch));
    out.print(format!("  Commit:  {}", short));
    out.print(format!(
        "  Installed: {}",
        pkg.installed_at.format("%Y-%m-%dT%H:%M:%SZ")
    ));
    out.print(format!(
        "  Updated:   {}",
        pkg.updated_at.format("%Y-%m-%dT%H:%M:%SZ")
    ));

    if let Ok(clone_root) = clone_root_for(&pkg.clone_path) {
        if let Ok(manifest) = load_manifest(&clone_root) {
            out.print(format!("  Version:   {}", manifest.package.version));
            out.print(format!("  Description: {}", manifest.package.description));
            out.print(format!("  License:  {}", manifest.package.license));
            out.print(format!("  Authors:  {}", manifest.package.authors.join(", ")));
            if let Some(urls) = &manifest.package.urls {
                if let Some(repo_url) = &urls.repository {
                    out.print(format!("  Repository: {}", repo_url));
                }
                if let Some(homepage) = &urls.homepage {
                    out.print(format!("  Homepage:   {}", homepage));
                }
            }
            out.print(format!("  Content: {}", manifest.content.summary()));
        }
    }

    for inst in &pkg.installations {
        out.print(format!("\n  Target: {}", inst.target));
        for entry in &inst.symlinks {
            out.print(format!("    {} -> {}", entry.dst, entry.src));
        }
    }

    Ok(())
}
