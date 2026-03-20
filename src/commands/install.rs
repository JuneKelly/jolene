use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::Utc;

use crate::cli::InstallArgs;
use crate::config::{self, clone_root_for};
use crate::content_check;
use crate::discovery;
use crate::git;
use crate::marketplace::{self, PluginSource};
use crate::output::Output;
use crate::state;
use crate::symlink::{SymlinkContext, SymlinkPlan, execute_symlinks, plan_symlinks};
use crate::template;
use crate::types::content::{ContentItem, ContentType};
use crate::types::source::Source;
use crate::types::state::{Installation, PackageState, SourceKind};
use crate::types::target::Target;
use crate::types::var_value::VarValue;
use crate::validation::{collect_content_items, load_manifest, resolve_prefix, validate_manifest};

pub fn run_from_args(args: &InstallArgs, out: &Output) -> Result<()> {
    let source = if let Some(ref s) = args.github {
        Source::from_github(s)?
    } else if let Some(ref path) = args.local {
        let abs = path
            .canonicalize()
            .with_context(|| format!("Cannot access local path: {}", path.display()))?;
        Source::Local(abs)
    } else if let Some(ref url) = args.url {
        if url.starts_with('/') || url.starts_with('.') {
            bail!(
                "--url does not accept local filesystem paths.\n  Use --local instead: jolene install --local {}",
                url
            );
        }
        Source::Url(url.clone())
    } else {
        unreachable!("clap ArgGroup ensures one of --github/--local/--url is set")
    };

    let cli_prefix = args.prefix.as_deref();
    let cli_no_prefix = args.no_prefix;

    if args.marketplace {
        run_marketplace(
            &source,
            &args.to,
            &args.pick,
            cli_prefix,
            cli_no_prefix,
            out,
        )
    } else {
        if !args.pick.is_empty() {
            out.print("Warning: --pick is ignored without --marketplace");
        }
        run(
            &source,
            &args.to,
            cli_prefix,
            cli_no_prefix,
            &args.var,
            &args.vars_json,
            out,
        )
    }
}

/// Native install flow — expects `jolene.toml`.
pub fn run(
    source: &Source,
    to: &[String],
    cli_prefix: Option<&str>,
    cli_no_prefix: bool,
    var_flags: &[String],
    vars_json_flags: &[String],
    out: &Output,
) -> Result<()> {
    out.print(format!("Installing {}...", source.display()));

    // Clone or pull
    let clone_root = clone_root_for(&format!("repos/{}", source.store_key()))?;

    if clone_root.exists() {
        out.print("  Updating existing clone...".to_string());
        git::pull(&clone_root)?;
    } else {
        out.print(format!("  Cloning {}", source.clone_url()));
        git::clone(&source.clone_url(), &clone_root)?;
    }

    // Validate
    let manifest = load_manifest(&clone_root)
        .map_err(|e| anyhow::anyhow!("Error: {} {}", source.display(), e))?;

    validate_manifest(&manifest, &clone_root)
        .map_err(|e| anyhow::anyhow!("Error: {} {}", source.display(), e))?;

    let mut items = collect_content_items(&manifest);
    let prefix = resolve_prefix(
        cli_prefix,
        cli_no_prefix,
        manifest.package.prefix.as_deref(),
    )?;

    out.print(format!("  Found: {}", manifest.content.summary()));
    if let Some(ref p) = prefix {
        out.print(format!("  Prefix: {}", p));
    }

    // 3c. Validate CLI overrides
    let declared_vars = manifest.template_vars()?;
    let (merged_vars, var_overrides) =
        template::parse_and_validate_var_overrides(var_flags, vars_json_flags, &declared_vars)?;

    // 3d. Scan for templates
    let exclude: std::collections::HashSet<&str> =
        manifest.template_exclude().iter().map(String::as_str).collect();
    template::scan_content_items(&mut items, &clone_root, &exclude)?;

    // Content quality checks (advisory)
    content_check::check_and_warn_skills(&items, &clone_root, out, "  ");
    content_check::check_and_warn_agents(&items, &clone_root, out, "  ");

    // Resolve targets
    let targets = resolve_targets(to)?;

    if targets.is_empty() {
        bail!(
            "No supported targets detected.\n  None found: ~/.claude/, ~/.config/opencode/, ~/.codex/\n  Use --to <target> to specify a target explicitly."
        );
    }

    // Acquire lock and load state together to prevent concurrent modifications
    // and ensure state is never read without holding the lock.
    let (_lock, mut app_state) = state::StateLock::acquire_and_load()?;
    check_prefix_mismatch(&app_state, &source.display(), prefix.as_deref())?;

    let display_names: HashMap<String, String> = app_state
        .packages
        .iter()
        .map(|p| (p.store_key().to_string(), p.source.clone()))
        .collect();

    let branch = git::current_branch(&clone_root)?;
    let commit = git::full_commit(&clone_root)?;
    let now = Utc::now();
    let store_key = source.store_key();

    // 5b. Render templates (per target)
    for target in &targets {
        template::render_content_items(
            &items,
            &clone_root,
            &store_key,
            target.slug(),
            prefix.as_deref(),
            &manifest,
            &merged_vars,
        )?;
    }

    // Phase 1: check conflicts and collect all plans (no side effects).
    let (mut staged, targets) = plan_all_targets(&PlanAllTargetsContext {
        items: &items,
        clone_root: &clone_root,
        targets: &targets,
        store_key: &store_key,
        display_names: &display_names,
        source,
        out,
        prefix: prefix.as_deref(),
    })?;

    // Phase 2: execute all plans atomically.
    let (new_installations, target_names) = execute_and_record(&mut staged, &targets, out)?;

    // Record state
    let clone_path = format!("repos/{}", store_key);

    record_state(
        &mut app_state,
        source,
        new_installations,
        StateRecord {
            clone_path,
            branch,
            commit,
            now,
            marketplace: None,
            plugin_name: None,
            plugin_path: None,
            display_override: None,
            prefix,
            var_overrides,
        },
    )?;
    state::save(&app_state)?;

    out.print(format!(
        "\nInstalled {} to {}",
        source.display(),
        target_names.join(", ")
    ));

    Ok(())
}

/// Marketplace install flow — expects `.claude-plugin/marketplace.json`.
fn run_marketplace(
    source: &Source,
    to: &[String],
    pick: &[String],
    cli_prefix: Option<&str>,
    cli_no_prefix: bool,
    out: &Output,
) -> Result<()> {
    if pick.is_empty() {
        bail!(
            "--pick is required with --marketplace\n  Use `jolene contents --marketplace --github {}` to see available plugins",
            source.display()
        );
    }

    out.print(format!(
        "Installing from marketplace {}...",
        source.display()
    ));

    // Clone or pull the marketplace repo
    let mp_clone_root = clone_root_for(&format!("repos/{}", source.store_key()))?;

    if mp_clone_root.exists() {
        out.print("  Updating marketplace clone...".to_string());
        git::pull(&mp_clone_root)?;
    } else {
        out.print(format!("  Cloning {}", source.clone_url()));
        git::clone(&source.clone_url(), &mp_clone_root)?;
    }

    // Parse marketplace.json
    let mp_path = mp_clone_root
        .join(".claude-plugin")
        .join("marketplace.json");
    if !mp_path.exists() {
        bail!(
            "No .claude-plugin/marketplace.json found in {}\n  Are you sure this is a marketplace repo?",
            source.display()
        );
    }

    let mp = marketplace::load_marketplace(&mp_clone_root)?;
    out.print(format!("  Marketplace: {}", mp.name));

    // Resolve targets
    let targets = resolve_targets(to)?;
    if targets.is_empty() {
        bail!(
            "No supported targets detected.\n  None found: ~/.claude/, ~/.config/opencode/, ~/.codex/\n  Use --to <target> to specify a target explicitly."
        );
    }

    // Resolve prefix from CLI flags (marketplace has no manifest prefix).
    let prefix = resolve_prefix(cli_prefix, cli_no_prefix, None)?;
    if let Some(ref p) = prefix {
        out.print(format!("  Prefix: {}", p));
    }

    // Acquire lock and load state together.
    let (_lock, mut app_state) = state::StateLock::acquire_and_load()?;

    // Process each picked plugin
    for plugin_name in pick {
        let entry = mp
            .plugins
            .iter()
            .find(|p| p.name == *plugin_name)
            .ok_or_else(|| {
                let available: Vec<_> = mp.plugins.iter().map(|p| p.name.as_str()).collect();
                anyhow::anyhow!(
                    "Plugin '{}' not found in marketplace.\n  Available: {}",
                    plugin_name,
                    available.join(", ")
                )
            })?;

        out.print(format!("\n  Plugin: {}", entry.name));
        if let Some(ref desc) = entry.description {
            out.print(format!("    {}", desc));
        }

        // Resolve plugin source to a local directory
        let resolved =
            resolve_plugin_source(&entry.source, &mp_clone_root, &entry.name, source, out)?;

        check_prefix_mismatch(&app_state, &resolved.display_name, prefix.as_deref())?;

        // Check for ignored features
        let ignored = marketplace::detect_ignored_features(&resolved.dir);
        if ignored.any() {
            out.print(format!(
                "  Warning: this plugin has {} that jolene does not install",
                ignored.labels().join(", ")
            ));
        }

        // Discover content
        let items = discovery::discover_content(&resolved.dir)?;
        if items.is_empty() {
            out.print(format!(
                "  Warning: plugin '{}' has no installable content (no commands, skills, or agents)",
                entry.name
            ));
            continue;
        }

        out.print(format!("    Found: {}", discovery::content_summary(&items)));

        // Content quality checks (advisory)
        content_check::check_and_warn_skills(&items, &resolved.dir, out, "    ");
        content_check::check_and_warn_agents(&items, &resolved.dir, out, "    ");

        // Rebuild display_names each iteration so cross-plugin conflicts are caught.
        let display_names: HashMap<String, String> = app_state
            .packages
            .iter()
            .map(|p| (p.store_key().to_string(), p.source.clone()))
            .collect();

        let branch = git::current_branch(&resolved.dir)?;
        let commit = git::full_commit(&resolved.dir)?;
        let now = Utc::now();

        let (mut staged, targets_used) = plan_all_targets(&PlanAllTargetsContext {
            items: &items,
            clone_root: &resolved.dir,
            targets: &targets,
            store_key: &resolved.store_key,
            display_names: &display_names,
            source: &resolved.source,
            out,
            prefix: prefix.as_deref(),
        })?;

        let (new_installations, target_names) =
            execute_and_record(&mut staged, &targets_used, out)?;

        let clone_path = format!("repos/{}", resolved.store_key);

        record_state(
            &mut app_state,
            &resolved.source,
            new_installations,
            StateRecord {
                clone_path,
                branch: branch.clone(),
                commit: commit.clone(),
                now,
                marketplace: Some(source.display()),
                plugin_name: Some(entry.name.clone()),
                plugin_path: resolved.plugin_path,
                display_override: Some(resolved.display_name),
                prefix: prefix.clone(),
                var_overrides: None,
            },
        )?;

        state::save(&app_state)?;

        out.print(format!(
            "\n  Installed plugin '{}' to {}",
            entry.name,
            target_names.join(", ")
        ));
    }
    Ok(())
}

/// Result of resolving a marketplace plugin's source.
struct ResolvedPlugin {
    /// Absolute path to the plugin's content on disk.
    dir: PathBuf,
    /// The source identity for state recording.
    source: Source,
    /// Store key (SHA256 hash) for the clone directory.
    store_key: String,
    /// For relative plugins, the subdirectory within the marketplace clone.
    /// None for external plugins (they have their own clone).
    plugin_path: Option<String>,
    /// Display name used as the `source` field in PackageState.
    /// For relative plugins: "org/marketplace::plugin-name".
    /// For external plugins: the plugin's own source display.
    display_name: String,
}

/// Resolve a marketplace plugin's source to a local directory, cloning if needed.
fn resolve_plugin_source(
    ps: &PluginSource,
    mp_clone_root: &Path,
    plugin_name: &str,
    mp_source: &Source,
    out: &Output,
) -> Result<ResolvedPlugin> {
    match ps {
        PluginSource::Relative { path } => {
            let cleaned = path.strip_prefix("./").unwrap_or(path);
            let plugin_dir = mp_clone_root.join(cleaned);
            if !plugin_dir.exists() {
                bail!(
                    "Plugin '{}' declares relative path '{}' but it does not exist in the marketplace repo",
                    plugin_name,
                    path
                );
            }
            let plugin_dir = discovery::resolve_plugin_dir(mp_clone_root, Some(cleaned))
                .with_context(|| {
                    format!(
                        "Plugin '{}' path '{}' escapes the marketplace repository",
                        plugin_name, path
                    )
                })?;
            // Relative plugins live inside the marketplace clone.
            // Use a composite display name so each gets a distinct PackageState entry.
            let display_name = format!("{}::{}", mp_source.display(), plugin_name);
            Ok(ResolvedPlugin {
                dir: plugin_dir,
                source: mp_source.clone(),
                store_key: mp_source.store_key(),
                plugin_path: Some(cleaned.to_string()),
                display_name,
            })
        }
        PluginSource::GitHub { repo, git_ref: _ } => {
            let plugin_source = Source::from_github(repo)?;
            let clone_root = clone_root_for(&format!("repos/{}", plugin_source.store_key()))?;

            if clone_root.exists() {
                out.verbose(format!("    Updating clone of {}...", repo));
                git::pull(&clone_root)?;
            } else {
                out.print(format!("    Cloning {}...", plugin_source.clone_url()));
                git::clone(&plugin_source.clone_url(), &clone_root)?;
            }

            let display_name = plugin_source.display();
            let key = plugin_source.store_key();
            Ok(ResolvedPlugin {
                dir: clone_root,
                source: plugin_source,
                store_key: key,
                plugin_path: None,
                display_name,
            })
        }
        PluginSource::Url { url, git_ref: _ } => {
            let plugin_source = Source::Url(url.clone());
            let clone_root = clone_root_for(&format!("repos/{}", plugin_source.store_key()))?;

            if clone_root.exists() {
                out.verbose(format!("    Updating clone of {}...", url));
                git::pull(&clone_root)?;
            } else {
                out.print(format!("    Cloning {}...", url));
                git::clone(url, &clone_root)?;
            }

            let display_name = plugin_source.display();
            let key = plugin_source.store_key();
            Ok(ResolvedPlugin {
                dir: clone_root,
                source: plugin_source,
                store_key: key,
                plugin_path: None,
                display_name,
            })
        }
        PluginSource::Unsupported => {
            bail!(
                "Plugin '{}' uses an unsupported source type (npm/pip are not yet supported by jolene)",
                plugin_name
            );
        }
    }
}

struct TargetStage {
    plan_count: usize,
    plans: Vec<SymlinkPlan>,
}

/// Context for planning symlinks across all targets.
struct PlanAllTargetsContext<'a> {
    items: &'a [ContentItem],
    clone_root: &'a Path,
    targets: &'a [Target],
    store_key: &'a str,
    display_names: &'a HashMap<String, String>,
    source: &'a Source,
    out: &'a Output,
    prefix: Option<&'a str>,
}

/// Plan symlinks for all targets. Returns the staged plans and the filtered target list.
fn plan_all_targets(ctx: &PlanAllTargetsContext<'_>) -> Result<(Vec<TargetStage>, Vec<Target>)> {
    let has_templated = ctx.items.iter().any(|i| i.templated);
    let mut staged: Vec<TargetStage> = Vec::new();
    let mut used_targets: Vec<Target> = Vec::new();

    for target in ctx.targets {
        let target_root = target
            .config_root()
            .ok_or_else(|| anyhow::anyhow!("Cannot determine config root for {}", target))?;

        let supported: Vec<_> = ctx
            .items
            .iter()
            .filter(|item| match item.content_type {
                ContentType::Command => target.supports_commands(),
                ContentType::Skill => target.supports_skills(),
                ContentType::Agent => target.supports_agents(),
            })
            .cloned()
            .collect();

        let skipped_commands = ctx
            .items
            .iter()
            .filter(|i| i.content_type == ContentType::Command && !target.supports_commands())
            .count();
        let skipped_agents = ctx
            .items
            .iter()
            .filter(|i| i.content_type == ContentType::Agent && !target.supports_agents())
            .count();

        if skipped_commands > 0 {
            ctx.out.print(format!(
                "  Warning: skipping {} command{} for {} (not supported by this target)",
                skipped_commands,
                if skipped_commands == 1 { "" } else { "s" },
                target
            ));
        }
        if skipped_agents > 0 {
            ctx.out.print(format!(
                "  Warning: skipping {} agent{} for {} (not supported by this target)",
                skipped_agents,
                if skipped_agents == 1 { "" } else { "s" },
                target
            ));
        }

        let rendered_root = if has_templated {
            Some(config::rendered_path_for(ctx.store_key, target.slug())?)
        } else {
            None
        };

        let plans = plan_symlinks(&SymlinkContext {
            items: &supported,
            clone_root: ctx.clone_root,
            target_root: &target_root,
            target_slug: target.slug(),
            package_source: ctx.store_key,
            display_names: ctx.display_names,
            prefix: ctx.prefix,
            rendered_item_root: rendered_root.as_deref(),
        })
        .map_err(|e| {
            anyhow::anyhow!(
                "Conflict installing {} to {}:\n  {}",
                ctx.source.display(),
                target,
                e
            )
        })?;

        staged.push(TargetStage {
            plan_count: plans.len(),
            plans,
        });
        used_targets.push(*target);
    }

    Ok((staged, used_targets))
}

/// Execute all staged symlink plans atomically and return installation records.
fn execute_and_record(
    staged: &mut [TargetStage],
    targets: &[Target],
    out: &Output,
) -> Result<(Vec<Installation>, Vec<String>)> {
    let all_plans: Vec<_> = staged.iter_mut().flat_map(|s| s.plans.drain(..)).collect();
    let all_entries = execute_symlinks(&all_plans)?;

    let mut new_installations: Vec<Installation> = Vec::new();
    let mut target_names: Vec<String> = Vec::new();
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
        target_names.push(target.slug().to_string());
    }

    Ok((new_installations, target_names))
}

/// Record package state, creating or updating as needed.
struct StateRecord {
    clone_path: String,
    branch: String,
    commit: String,
    now: chrono::DateTime<chrono::Utc>,
    marketplace: Option<String>,
    plugin_name: Option<String>,
    plugin_path: Option<String>,
    /// Override the display name used as the `source` field in PackageState.
    /// When None, uses `source.display()`.
    display_override: Option<String>,
    prefix: Option<String>,
    /// Template variable overrides from `--var` / `--vars-json`.
    var_overrides: Option<BTreeMap<String, VarValue>>,
}

fn record_state(
    app_state: &mut crate::types::state::State,
    source: &Source,
    new_installations: Vec<Installation>,
    record: StateRecord,
) -> Result<()> {
    let display_name = record
        .display_override
        .as_deref()
        .unwrap_or(&source.display())
        .to_string();

    // Look up by the display name we'll store (important for relative plugins
    // which use "org/marketplace::plugin-name" instead of just "org/marketplace").
    match state::find_package_mut(app_state, &display_name)? {
        Some(existing) => {
            existing.branch = record.branch;
            existing.commit = record.commit;
            existing.updated_at = record.now;
            if record.marketplace.is_some() {
                existing.marketplace = record.marketplace;
            }
            if record.plugin_name.is_some() {
                existing.plugin_name = record.plugin_name;
            }
            if record.plugin_path.is_some() {
                existing.plugin_path = record.plugin_path;
            }
            existing.prefix = record.prefix;
            existing.var_overrides = record.var_overrides;
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
                source_kind: match source {
                    Source::GitHub { .. } => SourceKind::GitHub,
                    Source::Local(_) => SourceKind::Local,
                    Source::Url(_) => SourceKind::Url,
                },
                source: display_name,
                clone_url: Some(source.clone_url()),
                clone_path: record.clone_path,
                branch: record.branch,
                commit: record.commit,
                installed_at: record.now,
                updated_at: record.now,
                installations: new_installations,
                marketplace: record.marketplace,
                plugin_name: record.plugin_name,
                plugin_path: record.plugin_path,
                prefix: record.prefix,
                var_overrides: record.var_overrides,
            });
        }
    }

    Ok(())
}

/// Error if the package is already installed with a different prefix.
fn check_prefix_mismatch(
    app_state: &crate::types::state::State,
    display_name: &str,
    new_prefix: Option<&str>,
) -> Result<()> {
    if let Some(existing) = state::find_package(app_state, display_name)? {
        let old = existing.prefix.as_deref();
        if old != new_prefix {
            let fmt = |p: Option<&str>| match p {
                Some(v) => format!("'{}'", v),
                None => "none".to_string(),
            };
            bail!(
                "Package '{}' is already installed with prefix {}.\n  To change prefix, uninstall first:\n    jolene uninstall {}",
                display_name,
                fmt(old),
                display_name,
            );
        }
    }
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
