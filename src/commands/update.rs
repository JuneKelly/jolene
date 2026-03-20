use std::collections::HashMap;

use anyhow::{Context, Result, bail};
use chrono::Utc;

use crate::config::{self, clone_root_for};
use crate::content_check;
use crate::discovery;
use crate::git;
use crate::output::Output;
use crate::state;
use crate::symlink::{SymlinkContext, SymlinkPlan, execute_symlinks, expand_tilde, plan_symlinks, remove_symlink};
use crate::template;
use crate::types::content::ContentItem;
use crate::types::content::ContentType;
use crate::types::state::State;
use crate::validation::{collect_content_items, load_manifest, validate_manifest, validate_prefix};

pub fn run(package: Option<&str>, out: &Output) -> Result<()> {
    let (_lock, mut app_state) = state::StateLock::acquire_and_load()?;

    let sources: Vec<String> = match package {
        Some(name) => {
            let pkg = state::find_package(&app_state, name)?;
            match pkg {
                Some(p) => vec![p.source.clone()],
                None => bail!("Package '{}' is not installed.", name),
            }
        }
        None => app_state
            .packages
            .iter()
            .map(|p| p.source.clone())
            .collect(),
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
    let prefix = pkg.prefix.clone();

    // Re-validate stored prefix against current rules.  A prefix that was
    // valid at install time will always remain valid under the current rules,
    // so this only fires if the state file was manually edited.
    if let Some(ref p) = prefix {
        validate_prefix(p).with_context(|| {
            format!(
                "Stored prefix '{}' for package '{}' is no longer valid.\n  Uninstall and reinstall to reset the prefix:\n    jolene uninstall {}",
                p, source, source
            )
        })?;
    }

    let stored_overrides = pkg.var_overrides.clone();
    let pkg_source = pkg.source.clone();
    let source_kind = pkg.source_kind.clone();
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
    let manifest = if !is_marketplace {
        let m = load_manifest(&clone_root)?;
        validate_manifest(&m, &clone_root)?;
        Some(m)
    } else {
        None
    };
    let mut items: Vec<ContentItem> = if let Some(ref m) = manifest {
        collect_content_items(m)
    } else {
        discovery::discover_content(&content_dir)?
    };

    // Content quality checks (advisory)
    content_check::check_and_warn_skills(&items, &content_dir, out, "  ");
    content_check::check_and_warn_agents(&items, &content_dir, out, "  ");

    // 2b. Re-scan and re-render (native packages only)
    if let Some(ref manifest) = manifest {
        let exclude: std::collections::HashSet<&str> =
            manifest.template_exclude().iter().map(String::as_str).collect();
        template::scan_content_items(&mut items, &content_dir, &exclude)?;

        let declared_vars = manifest.template_vars()?;

        // Validate stored overrides against the updated manifest.
        if let Some(ref overrides) = stored_overrides {
            let source_flag = match source_kind {
                crate::types::state::SourceKind::GitHub => {
                    format!("--github {}", pkg_source)
                }
                crate::types::state::SourceKind::Local => {
                    format!("--local {}", pkg_source)
                }
                crate::types::state::SourceKind::Url => {
                    format!("--url {}", pkg_source)
                }
            };
            template::validate_stored_overrides(
                overrides,
                &declared_vars,
                &source_flag,
            )?;
        }

        let merged_vars = match stored_overrides {
            Some(ref overrides) => template::merge_stored_overrides(&declared_vars, overrides),
            None => declared_vars,
        };

        // Render for each target.
        for inst in &installations {
            template::render_content_items(
                &items,
                &content_dir,
                &store_key,
                &inst.target,
                prefix.as_deref(),
                manifest,
                &merged_vars,
            )?;
        }
    }

    let new_branch = git::current_branch(&clone_root)?;
    let now = Utc::now();

    // 3. Phase 1: plan all additions across all targets (no side effects).
    //    Collect removals and recreations too, but don't act on them yet.
    use std::collections::HashSet;

    struct TargetStage {
        target_slug: String,
        new_srcs: HashSet<String>,
        plan_count: usize,
        plans: Vec<SymlinkPlan>,
        /// dst display paths (~/...) of symlinks to remove after additions succeed.
        removals: Vec<String>,
        /// Plans for symlinks that need recreation (templated status changed).
        /// These are executed separately after new additions succeed.
        recreation_plans: Vec<SymlinkPlan>,
    }

    let has_templated = items.iter().any(|i| i.templated);
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

        let existing_srcs: HashSet<String> = inst.symlinks.iter().map(|e| e.src.clone()).collect();

        let rendered_root = if has_templated {
            Some(config::rendered_path_for(&store_key, target.slug())?)
        } else {
            None
        };

        // Detect items whose templated status changed — need symlink recreation.
        // Build recreation plans directly (no conflict check needed since we'll
        // remove the old symlink before creating the new one in the execution phase).
        let mut recreation_plans: Vec<SymlinkPlan> = Vec::new();
        let mut recreation_srcs: HashSet<String> = HashSet::new();
        for entry in &inst.symlinks {
            if let Some(item) = supported.iter().find(|i| {
                i.relative_path().to_string_lossy().as_ref() == entry.src
            })
                && item.templated != entry.templated
            {
                let src = if item.templated {
                    if let Some(ref rendered) = rendered_root {
                        item.rendered_path(rendered)
                    } else {
                        item.source_path(&content_dir)
                    }
                } else {
                    item.source_path(&content_dir)
                };
                let content_dir_target = target_root.join(item.content_type.dir_name());
                let dst = item.dest_path(&content_dir_target, prefix.as_deref());
                recreation_plans.push(SymlinkPlan {
                    src,
                    dst,
                    relative_src: item.relative_path().to_string_lossy().into_owned(),
                    templated: item.templated,
                });
                recreation_srcs.insert(entry.src.clone());
            }
        }

        // Only truly new items go through plan_symlinks (with conflict checking).
        let new_items: Vec<_> = supported
            .iter()
            .filter(|i| {
                let src = i.relative_path().to_string_lossy().into_owned();
                !existing_srcs.contains(&src) && !recreation_srcs.contains(&src)
            })
            .cloned()
            .collect();

        let plans = plan_symlinks(&SymlinkContext {
            items: &new_items,
            clone_root: &content_dir,
            target_root: &target_root,
            target_slug: inst.target.as_str(),
            package_source: &store_key,
            display_names: &display_names,
            prefix: prefix.as_deref(),
            rendered_item_root: rendered_root.as_deref(),
        })?;

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
            recreation_plans,
        });
    }

    // 4. Execute all new additions atomically.
    //    A failure here rolls back all created symlinks; no removals or
    //    recreations have happened yet.
    let all_plans: Vec<_> = staged.iter_mut().flat_map(|s| s.plans.drain(..)).collect();
    let all_entries = execute_symlinks(&all_plans)?;

    // 5. Execute recreations and removals, then update state per target.
    //    New additions are on disk; it's now safe to touch existing symlinks.
    let pkg_mut = state::find_package_mut(app_state, source)?.unwrap();
    let mut offset = 0;
    for stage in &staged {
        let new_entries = all_entries[offset..offset + stage.plan_count].to_vec();
        offset += stage.plan_count;

        out.print(format!("\n  Updating {}:", stage.target_slug));
        for entry in &new_entries {
            out.print(format!("    + {} -> {}", entry.src, entry.dst));
        }

        // Execute recreation: remove old symlink, create new one.
        let mut recreation_entries = Vec::new();
        for plan in &stage.recreation_plans {
            remove_symlink(&plan.dst)?;
            if let Some(parent) = plan.dst.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::os::unix::fs::symlink(&plan.src, &plan.dst)
                .with_context(|| format!("Failed to create symlink {}", plan.dst.display()))?;
            recreation_entries.push(crate::types::state::SymlinkEntry {
                src: plan.relative_src.clone(),
                dst: crate::config::display_path(&plan.dst),
                templated: plan.templated,
            });
            out.verbose(format!(
                "    ~ {} (recreated)",
                crate::config::display_path(&plan.dst)
            ));
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
            // Remove old entries for removed items and recreated items.
            let recreation_srcs: HashSet<&str> = stage
                .recreation_plans
                .iter()
                .map(|p| p.relative_src.as_str())
                .collect();
            inst_mut
                .symlinks
                .retain(|e| stage.new_srcs.contains(&e.src) && !recreation_srcs.contains(e.src.as_str()));
            inst_mut.symlinks.extend(new_entries);
            inst_mut.symlinks.extend(recreation_entries);
        }
    }

    // 7. Update commit and timestamp, then persist once.
    pkg_mut.commit = new_commit;
    pkg_mut.branch = new_branch;
    pkg_mut.updated_at = now;

    state::save(app_state)?;

    out.print(format!("Updated {}", source));
    Ok(())
}
