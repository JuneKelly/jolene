use anyhow::{bail, Result};
use chrono::Utc;

use crate::config::clone_dir;
use crate::git;
use crate::output::Output;
use crate::state;
use crate::symlink::{execute_symlinks, expand_tilde, plan_symlinks, remove_symlink};
use crate::types::source::Source;
use crate::types::state::State;
use crate::validation::{collect_content_items, load_manifest, validate_manifest};

pub fn run(package: Option<&str>, out: &Output) -> Result<()> {
    let mut app_state = state::load()?;

    let sources: Vec<String> = match package {
        Some(name) => {
            let pkg = state::find_package(&app_state, name)?;
            match pkg {
                Some(p) => vec![p.source.clone()],
                None => bail!("Package '{}' is not installed.", name),
            }
        }
        None => app_state.packages.iter().map(|p| p.source.clone()).collect(),
    };

    if sources.is_empty() {
        out.print("No packages installed.");
        return Ok(());
    }

    for source in &sources {
        out.print(format!("Updating {}...", source));
        update_one(source, &mut app_state, out)?;
    }

    Ok(())
}

fn update_one(source: &str, app_state: &mut State, out: &Output) -> Result<()> {
    let pkg = state::find_package(app_state, source)?
        .ok_or_else(|| anyhow::anyhow!("Package '{}' not found in state.", source))?;

    let src = Source::parse(&pkg.source)?;
    let pkg_source = pkg.source.clone();

    let clone_root = clone_dir(&src.author, &src.repo)?;

    // 1. Pull — detect no-op early.
    let old_commit = git::full_commit(&clone_root)?;
    git::pull(&clone_root)?;
    let new_commit = git::full_commit(&clone_root)?;

    if old_commit == new_commit {
        out.print(format!("  {} is already up to date.", source));
        return Ok(());
    }

    // 2. Validate updated manifest.
    let manifest = load_manifest(&clone_root)?;
    validate_manifest(&manifest, &clone_root)?;
    let items = collect_content_items(&manifest);

    let new_branch = git::current_branch(&clone_root)?;
    let now = Utc::now();

    // 3. Per installation: sync symlinks.
    //    Order: create new symlinks first, then remove old ones.
    //    This way a failure during creation leaves existing symlinks intact.
    let pkg = state::find_package(app_state, source)?.unwrap();
    let installations: Vec<_> = pkg.installations.clone();

    for inst in &installations {
        let target = crate::types::target::Target::from_slug(&inst.target);
        let Some(target) = target else {
            out.verbose(format!("  Unknown target '{}', skipping.", inst.target));
            continue;
        };

        let target_root = target
            .config_root()
            .ok_or_else(|| anyhow::anyhow!("Cannot determine config root for {}", target))?;

        // Supported items for this target.
        let supported: Vec<_> = items
            .iter()
            .filter(|item| match item.content_type {
                crate::types::content::ContentType::Command => target.supports_commands(),
                crate::types::content::ContentType::Skill => target.supports_skills(),
                crate::types::content::ContentType::Agent => target.supports_agents(),
            })
            .cloned()
            .collect();

        let new_srcs: std::collections::HashSet<String> = supported
            .iter()
            .map(|i| i.relative_path().to_string_lossy().into_owned())
            .collect();

        let existing_srcs: std::collections::HashSet<String> =
            inst.symlinks.iter().map(|e| e.src.clone()).collect();

        // Create symlinks for new content first — bail on failure before any removals.
        let new_items: Vec<_> = supported
            .iter()
            .filter(|i| !existing_srcs.contains(&i.relative_path().to_string_lossy().into_owned()))
            .cloned()
            .collect();

        let plans = plan_symlinks(&new_items, &clone_root, &target_root, inst.target.as_str(), &pkg_source)?;
        let new_entries = execute_symlinks(&plans)?;

        for entry in &new_entries {
            out.print(format!("    + {} -> {}", entry.src, entry.dst));
        }

        // Remove symlinks for content that no longer exists.
        for entry in &inst.symlinks {
            if !new_srcs.contains(&entry.src) {
                if let Some(dst) = expand_tilde(&entry.dst) {
                    remove_symlink(&dst)?;
                    out.print(format!("    - {} (removed)", entry.dst));
                }
            }
        }

        // Update in-memory state for this installation.
        let pkg_mut = state::find_package_mut(app_state, source)?.unwrap();
        if let Some(inst_mut) = pkg_mut.installations.iter_mut().find(|i| i.target == inst.target)
        {
            inst_mut.symlinks.retain(|e| new_srcs.contains(&e.src));
            inst_mut.symlinks.extend(new_entries);
        }
    }

    // 4. Update commit and timestamp, then persist.
    let pkg_mut = state::find_package_mut(app_state, source)?.unwrap();
    pkg_mut.commit = new_commit;
    pkg_mut.branch = new_branch;
    pkg_mut.updated_at = now;

    state::save(app_state)?;

    out.print(format!("Updated {}", source));
    Ok(())
}
