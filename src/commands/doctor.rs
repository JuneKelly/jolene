use anyhow::Result;

use crate::config::jolene_root;
use crate::output::Output;
use crate::state;
use crate::symlink::expand_tilde;

pub fn run(out: &Output) -> Result<()> {
    let app_state = state::load()?;
    let jolene_root = jolene_root()?;
    let mut issues = 0;

    if app_state.packages.is_empty() {
        out.print("No packages installed. Nothing to check.");
        return Ok(());
    }

    out.print("Checking installations...\n");

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

    if issues == 0 {
        out.print("All installations healthy.");
    } else {
        out.print(format!("\n{} issue(s) found.", issues));
    }

    Ok(())
}
