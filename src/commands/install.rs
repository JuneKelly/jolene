use anyhow::{bail, Result};
use chrono::Utc;

use crate::config::clone_dir;
use crate::git;
use crate::output::Output;
use crate::state;
use crate::symlink::{execute_symlinks, plan_symlinks, SymlinkPlan};
use crate::types::source::Source;
use crate::types::state::{Installation, PackageState};
use crate::types::target::Target;
use crate::validation::{collect_content_items, load_manifest, validate_manifest};

pub fn run(source_str: &str, to: &[String], out: &Output) -> Result<()> {
    // 1. Parse source
    let source = Source::parse(source_str)?;

    out.print(format!("Installing {}...", source));

    // 2. Clone or pull
    let clone_root = clone_dir(&source.author, &source.repo)?;

    if clone_root.exists() {
        out.print(format!("  Updating existing clone..."));
        git::pull(&clone_root)?;
    } else {
        out.print(format!("  Cloning {}", source.github_url()));
        git::clone(&source.github_url(), &clone_root)?;
    }

    // 3. Validate
    let manifest = load_manifest(&clone_root)
        .map_err(|e| anyhow::anyhow!("Error: {}/{} {}", source.author, source.repo, e))?;

    validate_manifest(&manifest, &clone_root)
        .map_err(|e| anyhow::anyhow!("Error: {}/{} {}", source.author, source.repo, e))?;

    let items = collect_content_items(&manifest);

    out.print(format!("  Found: {}", manifest.content.summary()));

    // 4. Resolve targets
    let targets = resolve_targets(to)?;

    if targets.is_empty() {
        bail!(
            "No supported targets detected.\n  None found: ~/.claude/, ~/.config/opencode/, ~/.codex/\n  Use --to <target> to specify a target explicitly."
        );
    }

    // 5-6. Phase 1: check conflicts and collect all plans (no side effects).
    //      Abort on first conflict before any symlinks are created.
    let branch = git::current_branch(&clone_root)?;
    let commit = git::full_commit(&clone_root)?;
    let now = Utc::now();

    use crate::types::content::ContentType;

    struct TargetStage {
        plan_count: usize,
        plans: Vec<SymlinkPlan>,
    }

    let mut staged: Vec<TargetStage> = Vec::new();

    for target in &targets {
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

        let skipped_commands = items
            .iter()
            .filter(|i| i.content_type == ContentType::Command && !target.supports_commands())
            .count();
        let skipped_agents = items
            .iter()
            .filter(|i| i.content_type == ContentType::Agent && !target.supports_agents())
            .count();

        if skipped_commands > 0 {
            out.verbose(format!(
                "  Skipping {} command{} for {} (not supported)",
                skipped_commands,
                if skipped_commands == 1 { "" } else { "s" },
                target
            ));
        }
        if skipped_agents > 0 {
            out.verbose(format!(
                "  Skipping {} agent{} for {} (not supported)",
                skipped_agents,
                if skipped_agents == 1 { "" } else { "s" },
                target
            ));
        }

        let plans = plan_symlinks(&supported, &clone_root, &target_root, target.slug(), &source.display_name())
            .map_err(|e| {
                anyhow::anyhow!("Conflict installing {} to {}:\n  {}", source, target, e)
            })?;

        staged.push(TargetStage { plan_count: plans.len(), plans });
    }

    // 7. Phase 2: execute all plans atomically.
    //    Flattening into one execute_symlinks call means a failure at any point
    //    rolls back all symlinks created so far, across all targets.
    let all_plans: Vec<_> = staged.iter_mut().flat_map(|s| s.plans.drain(..)).collect();
    let all_entries = execute_symlinks(&all_plans)?;

    // Split entries back by target, print output, and build installation records.
    let mut new_installations: Vec<Installation> = Vec::new();
    let mut offset = 0;

    for (target, stage) in targets.iter().zip(staged.iter()) {
        let entries = all_entries[offset..offset + stage.plan_count].to_vec();
        offset += stage.plan_count;

        out.print(format!("\n  Installing to {}:", target));
        for entry in &entries {
            out.print(format!("    + {} -> {}", entry.src, entry.dst));
        }

        new_installations.push(Installation {
            target: target.slug().to_string(),
            symlinks: entries,
        });
    }

    // 8. Record state
    let mut app_state = state::load()?;

    let clone_path = format!("repos/{}/{}", source.author, source.repo);

    match state::find_package_mut(&mut app_state, &source.display_name())? {
        Some(existing) => {
            existing.branch = branch;
            existing.commit = commit;
            existing.updated_at = now;
            for inst in new_installations {
                if let Some(existing_inst) = existing
                    .installations
                    .iter_mut()
                    .find(|i| i.target == inst.target)
                {
                    existing_inst.symlinks = inst.symlinks;
                } else {
                    existing.installations.push(inst);
                }
            }
        }
        None => {
            app_state.packages.push(PackageState {
                source: source.display_name(),
                clone_path,
                branch,
                commit,
                installed_at: now,
                updated_at: now,
                installations: new_installations,
            });
        }
    }

    state::save(&app_state)?;

    let target_names: Vec<_> = targets.iter().map(|t| t.slug().to_string()).collect();
    out.print(format!(
        "\nInstalled {} to {}",
        source,
        target_names.join(", ")
    ));

    Ok(())
}

pub fn resolve_targets(to: &[String]) -> Result<Vec<Target>> {
    if to.is_empty() {
        return Ok(Target::detect_available());
    }

    let mut targets = Vec::new();
    for slug in to {
        match Target::from_slug(slug) {
            Some(t) => targets.push(t),
            None => bail!(
                "Unknown target '{}'.\n  Supported targets: claude-code, opencode, codex",
                slug
            ),
        }
    }
    Ok(targets)
}
