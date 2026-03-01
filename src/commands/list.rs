use anyhow::Result;

use crate::config::clone_dir;
use crate::output::Output;
use crate::state;
use crate::types::source::Source;
use crate::validation::load_manifest;

pub fn run(target: Option<&str>, out: &Output) -> Result<()> {
    let app_state = state::load()?;

    let packages: Vec<_> = match target {
        Some(slug) => app_state
            .packages
            .iter()
            .filter(|p| p.installations.iter().any(|i| i.target == slug))
            .collect(),
        None => app_state.packages.iter().collect(),
    };

    if packages.is_empty() {
        out.print("No packages installed.");
        return Ok(());
    }

    out.print("Installed packages:\n");

    for pkg in packages {
        let target_names: Vec<_> = pkg.installations.iter().map(|i| i.target.as_str()).collect();

        let content_summary = Source::parse(&pkg.source)
            .ok()
            .and_then(|src| clone_dir(&src.author, &src.repo).ok())
            .and_then(|clone_root| load_manifest(&clone_root).map(|m| m.content.summary()).ok())
            .unwrap_or_else(|| "unknown".to_string());

        let short = &pkg.commit[..pkg.commit.len().min(7)];

        out.print(format!("  {}", pkg.source));
        out.print(format!("    Targets: {}", target_names.join(", ")));
        out.print(format!("    Content: {}", content_summary));
        out.print(format!("    Version: ({}@{})\n", pkg.branch, short));
    }

    Ok(())
}
