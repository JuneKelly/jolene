use std::collections::HashMap;

use anyhow::{bail, Result};
use chrono::Utc;

use crate::config::clone_root_for;
use crate::discovery;
use crate::git;
use crate::output::Output;
use crate::content_check;
use crate::state;
use crate::types::content::ContentType;
use crate::symlink::{execute_symlinks, expand_tilde, plan_symlinks, remove_symlink, SymlinkPlan};
use crate::types::content::ContentItem;
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

    let store_key = pkg.store_key().to_owned();
    let installations: Vec<_> = pkg.installations.clone();
    let is_marketplace = pkg.marketplace.is_some();
    let plugin_path = pkg.plugin_path.clone();
    let clone_root = clone_root_for(&pkg.clone_path)?;

    let display_names: HashMap<String, String> = app_state
        .packages
        .iter()
        .map(|p| (p.store_key().to_string(), p.source.clone()))
        .collect();

    // 1. Pull — detect no-op early.
    let old_commit = git::full_commit(&clone_root)?;
    git::pull(&clone_root)?;
    let new_commit = git::full_commit(&clone_root)?;

    if old_commit == new_commit {
        out.print(format!("  {} is already up to date.", source));
        return Ok(());
    }

    // 2. Collect content — marketplace uses discovery, native uses manifest.
    //    Relative marketplace plugins have a subdirectory within the clone.
    let content_dir = discovery::resolve_plugin_dir(&clone_root, plugin_path.as_deref())?;
    let items: Vec<ContentItem> = if is_marketplace {
        discovery::discover_content(&content_dir)?
    } else {
        let manifest = load_manifest(&clone_root)?;
        validate_manifest(&manifest, &clone_root)?;
        collect_content_items(&manifest)
    };

    // Content quality checks (advisory)
    content_check::check_and_warn_skills(&items, &content_dir, out, "  ");
    content_check::check_and_warn_agents(&items, &content_dir, out, "  ");

    let new_branch = git::current_branch(&clone_root)?;
    let now = Utc::now();

    // 3. Phase 1: plan all additions across all targets (no side effects).
    //    Collect removals too, but don't act on them yet.
    use std::collections::HashSet;

    struct TargetStage {
        target_slug: String,
        new_srcs: HashSet<String>,
        plan_count: usize,
        plans: Vec<SymlinkPlan>,
        /// dst display paths (~/...) of symlinks to remove after additions succeed.
        removals: Vec<String>,
    }

    let mut staged: Vec<TargetStage> = Vec::new();

    for inst in &installations {
        let target = crate::types::target::Target::from_slug(&inst.target);
        let Some(target) = target else {
            out.verbose(format!("  Unknown target '{}', skipping.", inst.target));
            continue;
        };

        let target_root = target
            .config_root()
            .ok_or_else(|| anyhow::anyhow!("Cannot determine config root for {}", target))?;

        let supported: Vec<_> = items
            .iter()
            .filter(|item| match item.content_type {
                ContentType::Command => target.supports_commands(),
                ContentType::Skill => target.supports_skills(),
                ContentType::Agent => target.supports_agents(),
            })
            .cloned()
            .collect();

        let new_srcs: HashSet<String> = supported
            .iter()
            .map(|i| i.relative_path().to_string_lossy().into_owned())
            .collect();

        let existing_srcs: HashSet<String> =
            inst.symlinks.iter().map(|e| e.src.clone()).collect();

        let new_items: Vec<_> = supported
            .iter()
            .filter(|i| !existing_srcs.contains(&i.relative_path().to_string_lossy().into_owned()))
            .cloned()
            .collect();

        let plans = plan_symlinks(
            &new_items,
            &content_dir,
            &target_root,
            inst.target.as_str(),
            &store_key,
            &display_names,
        )?;

        let removals: Vec<String> = inst
            .symlinks
            .iter()
            .filter(|e| !new_srcs.contains(&e.src))
            .map(|e| e.dst.clone())
            .collect();

        staged.push(TargetStage {
            target_slug: inst.target.clone(),
            new_srcs,
            plan_count: plans.len(),
            plans,
            removals,
        });
    }

    // 4. Phase 2: execute all additions atomically.
    //    A failure here rolls back all created symlinks; no removals have happened.
    let all_plans: Vec<_> = staged.iter_mut().flat_map(|s| s.plans.drain(..)).collect();
    let all_entries = execute_symlinks(&all_plans)?;

    // 5. Phase 3: removals and state update per target.
    //    Additions are on disk; removing old symlinks now is safe.
    let pkg_mut = state::find_package_mut(app_state, source)?.unwrap();
    let mut offset = 0;
    for stage in &staged {
        let new_entries = all_entries[offset..offset + stage.plan_count].to_vec();
        offset += stage.plan_count;

        out.print(format!("\n  Updating {}:", stage.target_slug));
        for entry in &new_entries {
            out.print(format!("    + {} -> {}", entry.src, entry.dst));
        }
        for dst_str in &stage.removals {
            if let Some(dst) = expand_tilde(dst_str) {
                remove_symlink(&dst)?;
                out.print(format!("    - {} (removed)", dst_str));
            }
        }

        if let Some(inst_mut) = pkg_mut
            .installations
            .iter_mut()
            .find(|i| i.target == stage.target_slug)
        {
            inst_mut.symlinks.retain(|e| stage.new_srcs.contains(&e.src));
            inst_mut.symlinks.extend(new_entries);
        }
    }

    // 6. Update commit and timestamp, then persist once.
    pkg_mut.commit = new_commit;
    pkg_mut.branch = new_branch;
    pkg_mut.updated_at = now;

    state::save(app_state)?;

    out.print(format!("Updated {}", source));
    Ok(())
}
