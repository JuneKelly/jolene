use std::collections::HashSet;

use anyhow::Result;

use crate::config::{self, jolene_root};
use crate::output::Output;
use crate::state;
use crate::symlink::expand_tilde;

pub fn run(out: &Output) -> Result<()> {
    let app_state = state::load()?;
    let jolene_root = jolene_root()?;
    let mut issues = 0;

    out.print("Checking installations...\n");

    if app_state.packages.is_empty() {
        out.verbose("No packages installed.");
    }

    for pkg in &app_state.packages {
        let clone_root = jolene_root.join(&pkg.clone_path);

        // Check clone exists
        if !clone_root.exists() {
            out.print(format!(
                "  [MISSING CLONE] {} — clone not found at {}",
                pkg.source,
                clone_root.display()
            ));
            issues += 1;
            continue;
        }

        for inst in &pkg.installations {
            for entry in &inst.symlinks {
                let Some(dst) = expand_tilde(&entry.dst) else {
                    out.print(format!(
                        "  [BAD PATH] {} — cannot expand path: {}",
                        pkg.source, entry.dst
                    ));
                    issues += 1;
                    continue;
                };

                if !dst.is_symlink() {
                    out.print(format!(
                        "  [MISSING SYMLINK] {} ({}) — {}",
                        pkg.source, inst.target, entry.dst
                    ));
                    issues += 1;
                    continue;
                }

                // Symlink exists — check it resolves
                match dst.read_link() {
                    Ok(target) if !target.exists() => {
                        out.print(format!(
                            "  [BROKEN SYMLINK] {} ({}) — {} -> {} (target missing)",
                            pkg.source,
                            inst.target,
                            entry.dst,
                            target.display()
                        ));
                        issues += 1;
                    }
                    Err(e) => {
                        out.print(format!(
                            "  [ERROR] {} ({}) — {}: {}",
                            pkg.source, inst.target, entry.dst, e
                        ));
                        issues += 1;
                    }
                    Ok(_) => {
                        out.verbose(format!("  [OK] {} ({})", entry.dst, inst.target));
                    }
                }
            }
        }
    }

    // Check for orphaned rendered/ directories.
    let rendered_root = config::rendered_root()?;
    if rendered_root.exists() {
        let known_keys: HashSet<String> = app_state
            .packages
            .iter()
            .map(|p| p.store_key().to_string())
            .collect();

        if let Ok(entries) = std::fs::read_dir(&rendered_root) {
            for entry in entries.flatten() {
                if entry.path().is_dir()
                    && let Some(name) = entry.file_name().to_str()
                    && !known_keys.contains(name)
                {
                    out.print(format!(
                        "  [ORPHANED RENDERED] {} — not referenced by any installed package",
                        entry.path().display()
                    ));
                    issues += 1;
                }
            }
        }
    }

    if issues == 0 {
        out.print("All installations healthy.");
    } else {
        out.print(format!("\n{} issue(s) found.", issues));
    }

    Ok(())
}
