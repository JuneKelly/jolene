use anyhow::Result;

use crate::config::clone_dir;
use crate::output::Output;
use crate::state;
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
        let parts: Vec<&str> = pkg.source.splitn(2, '/').collect();
        let (author, repo) = match parts.as_slice() {
            [a, r] => (*a, *r),
            _ => (pkg.source.as_str(), ""),
        };

        let target_names: Vec<_> = pkg.installations.iter().map(|i| i.target.as_str()).collect();

        // Load manifest for content summary
        let content_summary = if let Ok(clone_root) = clone_dir(author, repo) {
            load_manifest(&clone_root)
                .map(|m| m.content.summary())
                .unwrap_or_else(|_| "unknown".to_string())
        } else {
            "unknown".to_string()
        };

        let short = &pkg.commit[..pkg.commit.len().min(7)];

        out.print(format!("  {}", pkg.source));
        out.print(format!("    Targets: {}", target_names.join(", ")));
        out.print(format!("    Content: {}", content_summary));
        out.print(format!(
            "    Version: ({}@{})\n",
            pkg.branch, short
        ));
    }

    Ok(())
}
