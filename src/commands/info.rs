use anyhow::{Result, bail};

use crate::config::clone_root_for;
use crate::output::Output;
use crate::state;
use crate::validation::load_manifest;

pub fn run(bundle: &str, out: &Output) -> Result<()> {
    let app_state = state::load()?;

    let pkg = state::find_bundle(&app_state, bundle)?;
    let pkg = match pkg {
        Some(p) => p,
        None => bail!("Bundle '{}' is not installed.", bundle),
    };

    let short = &pkg.commit[..pkg.commit.len().min(7)];

    out.print(pkg.source.to_string());
    out.print(format!("  Source:  {}", pkg.source_kind));
    if let Some(ref prefix) = pkg.prefix {
        out.print(format!("  Prefix:  {}", prefix));
    }
    if let Some(url) = &pkg.clone_url {
        out.print(format!("  URL:     {}", url));
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

    if let Ok(clone_root) = clone_root_for(&pkg.clone_path)
        && let Ok(manifest) = load_manifest(&clone_root)
    {
        out.print(format!("  Version:   {}", manifest.bundle.version));
        out.print(format!("  Description: {}", manifest.bundle.description));
        out.print(format!("  License:  {}", manifest.bundle.license));
        out.print(format!(
            "  Authors:  {}",
            manifest.bundle.authors.join(", ")
        ));
        if let Some(urls) = &manifest.bundle.urls {
            if let Some(repo_url) = &urls.repository {
                out.print(format!("  Repository: {}", repo_url));
            }
            if let Some(homepage) = &urls.homepage {
                out.print(format!("  Homepage:   {}", homepage));
            }
        }
        out.print(format!("  Content: {}", manifest.content.summary()));
    }

    for inst in &pkg.installations {
        out.print(format!("\n  Target: {}", inst.target));
        for entry in &inst.symlinks {
            out.print(format!("    {} -> {}", entry.dst, entry.src));
        }
    }

    Ok(())
}
