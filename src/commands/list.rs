use anyhow::Result;

use crate::config::clone_root_for;
use crate::output::Output;
use crate::state;
use crate::validation::load_manifest;

pub fn run(target: Option<&str>, out: &Output) -> Result<()> {
    let app_state = state::load()?;

    let bundles: Vec<_> = match target {
        Some(slug) => app_state
            .bundles
            .iter()
            .filter(|p| p.installations.iter().any(|i| i.target == slug))
            .collect(),
        None => app_state.bundles.iter().collect(),
    };

    if bundles.is_empty() {
        out.print("No bundles installed.");
        return Ok(());
    }

    out.print("Installed bundles:\n");

    for pkg in bundles {
        let target_names: Vec<_> = pkg
            .installations
            .iter()
            .map(|i| i.target.as_str())
            .collect();

        let content_summary = clone_root_for(&pkg.clone_path)
            .ok()
            .and_then(|clone_root| load_manifest(&clone_root).map(|m| m.content.summary()).ok())
            .unwrap_or_else(|| "unknown".to_string());

        let short = &pkg.commit[..pkg.commit.len().min(7)];

        out.print(format!("  {}", pkg.source));
        out.print(format!("    Source:  {}", pkg.source_kind));
        if let Some(ref prefix) = pkg.prefix {
            out.print(format!("    Prefix:  {}", prefix));
        }
        out.print(format!("    Targets: {}", target_names.join(", ")));
        out.print(format!("    Content: {}", content_summary));
        out.print(format!("    Version: ({}@{})\n", pkg.branch, short));
    }

    Ok(())
}
