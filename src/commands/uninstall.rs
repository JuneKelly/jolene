use anyhow::{Result, bail};

use crate::config::jolene_root;
use crate::output::Output;
use crate::state;
use crate::symlink::{expand_tilde, remove_symlink};
use crate::types::target::Target;

pub fn run(package: &str, from: &[String], purge: bool, out: &Output) -> Result<()> {
    let mut app_state = state::load()?;

    // 1. Lookup package
    let pkg = state::find_package(&app_state, package)?;
    let pkg = match pkg {
        Some(p) => p,
        None => bail!("Package '{}' is not installed.", package),
    };
    let source = pkg.source.clone();

    out.print(format!("Uninstalling {}...", source));

    // 2. Scope to --from targets, or all if omitted
    let target_slugs: Vec<String> = if from.is_empty() {
        pkg.installations.iter().map(|i| i.target.clone()).collect()
    } else {
        let installed_targets: Vec<&str> = pkg
            .installations
            .iter()
            .map(|i| i.target.as_str())
            .collect();
        for slug in from {
            if Target::from_slug(slug).is_none() {
                bail!(
                    "Unknown target '{}'.\n  Supported targets: claude-code, opencode, codex",
                    slug
                );
            }
            if !installed_targets.contains(&slug.as_str()) {
                bail!(
                    "{} is not installed to '{}'.\n  Installed targets: {}",
                    source,
                    slug,
                    if installed_targets.is_empty() {
                        "none".to_string()
                    } else {
                        installed_targets.join(", ")
                    }
                );
            }
        }
        from.to_vec()
    };

    // 3. Remove symlinks
    let pkg_mut = state::find_package_mut(&mut app_state, &source)?.unwrap();

    for slug in &target_slugs {
        let Some(inst) = pkg_mut.installations.iter().find(|i| &i.target == slug) else {
            out.verbose(format!("  {} is not installed to {}", source, slug));
            continue;
        };

        out.print(format!("  Removing from {}:", slug));
        for entry in &inst.symlinks {
            if let Some(dst) = expand_tilde(&entry.dst) {
                if !dst.is_symlink() && !dst.exists() {
                    out.verbose(format!("    (already gone) {}", entry.dst));
                } else {
                    remove_symlink(&dst)?;
                    out.print(format!("    - {}", entry.dst));
                }
            }
        }
    }

    // 4. Update state — remove target entries; remove package if no targets remain
    let pkg_mut = state::find_package_mut(&mut app_state, &source)?.unwrap();
    pkg_mut
        .installations
        .retain(|i| !target_slugs.contains(&i.target));

    let remove_package = pkg_mut.installations.is_empty();
    let clone_path = pkg_mut.clone_path.clone();

    if remove_package {
        app_state.packages.retain(|p| p.source != source);
    }

    state::save(&app_state)?;

    // 5. Purge clone if requested
    if purge && remove_package {
        let clone_still_needed = app_state
            .packages
            .iter()
            .any(|p| p.clone_path == clone_path);
        if clone_still_needed {
            out.print("  --purge skipped: clone is shared with other installed plugins.");
        } else {
            let root = jolene_root()?;
            let full_clone_path = root.join(&clone_path);
            if full_clone_path.exists() {
                std::fs::remove_dir_all(&full_clone_path)?;
                out.print(format!("  Purged clone at {}", full_clone_path.display()));
            }
        }
    } else if purge {
        out.print("  --purge skipped: package still installed to other targets.");
    }

    out.print(format!("Uninstalled {}", source));
    Ok(())
}
