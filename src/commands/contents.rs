use anyhow::{bail, Context, Result};

use crate::cli::ContentsArgs;
use crate::config::clone_root_for;
use crate::discovery;
use crate::git;
use crate::marketplace;
use crate::output::Output;
use crate::state;
use crate::types::content::ContentType;
use crate::types::source::Source;
use crate::validation::{collect_content_items, load_manifest};

pub fn run(args: &ContentsArgs, out: &Output) -> Result<()> {
    if let Some(ref pkg_name) = args.package {
        return show_installed_package(pkg_name, out);
    }

    let source = if let Some(ref s) = args.github {
        Some(Source::from_github(s)?)
    } else if let Some(ref path) = args.local {
        let abs = path
            .canonicalize()
            .with_context(|| format!("Cannot access local path: {}", path.display()))?;
        Some(Source::Local(abs))
    } else {
        args.url.as_ref().map(|url| Source::Url(url.clone()))
    };

    let Some(source) = source else {
        bail!("Specify a source (--github, --local, --url) or an installed package name.");
    };

    // Clone or pull
    let clone_root = clone_root_for(&format!("repos/{}", source.store_key()))?;

    if clone_root.exists() {
        git::pull(&clone_root)?;
    } else {
        out.print(format!("Cloning {}...", source.clone_url()));
        git::clone(&source.clone_url(), &clone_root)?;
    }

    if args.marketplace {
        show_marketplace(&source, &clone_root, out)
    } else {
        show_native_package(&source, &clone_root, out)
    }
}

fn show_marketplace(
    source: &Source,
    clone_root: &std::path::Path,
    out: &Output,
) -> Result<()> {
    let mp_path = clone_root.join(".claude-plugin").join("marketplace.json");
    if !mp_path.exists() {
        bail!(
            "No .claude-plugin/marketplace.json found in {}\n  Are you sure this is a marketplace repo?",
            source.display()
        );
    }

    let mp = marketplace::load_marketplace(clone_root)?;

    // Header
    out.print(mp.name.to_string());
    if let Some(ref meta) = mp.metadata
        && let Some(ref desc) = meta.description
    {
        out.print(format!("  {}", desc));
    }
    if let Some(ref owner) = mp.owner
        && let Some(ref name) = owner.name
    {
        out.print(format!("  Maintained by: {}", name));
    }

    out.print(format!("\nAvailable plugins ({}):\n", mp.plugins.len()));

    for entry in &mp.plugins {
        let plugin_dir = resolve_plugin_dir_for_display(&entry.source, clone_root);

        // Check if this plugin has any installable content
        let has_content = plugin_dir
            .as_ref()
            .and_then(|d| discovery::discover_content(d).ok())
            .is_some_and(|items| !items.is_empty());

        // Check for hooks-only plugins
        let has_only_ignored = plugin_dir
            .as_ref()
            .map(|d| {
                let ignored = marketplace::detect_ignored_features(d);
                ignored.any() && !has_content
            })
            .unwrap_or(false);

        let desc = entry.description.as_deref().unwrap_or("");
        if has_only_ignored {
            out.print(format!(
                "  {:<24} {} (hooks only — not installable by jolene)",
                entry.name, desc
            ));
        } else {
            out.print(format!("  {:<24} {}", entry.name, desc));
        }
    }

    out.print(format!(
        "\nInstall with: jolene install --marketplace --github {} --pick <plugin>",
        source.display()
    ));

    Ok(())
}

fn show_native_package(
    source: &Source,
    clone_root: &std::path::Path,
    out: &Output,
) -> Result<()> {
    let manifest = load_manifest(clone_root)
        .map_err(|e| anyhow::anyhow!("{} {}", source.display(), e))?;

    let items = collect_content_items(&manifest);

    out.print(format!(
        "{} — {}",
        manifest.package.name, manifest.package.description
    ));
    out.print(format!("Version: {}", manifest.package.version));

    print_content_list(&items, out);

    Ok(())
}

fn show_installed_package(pkg_name: &str, out: &Output) -> Result<()> {
    let app_state = state::load()?;
    let pkg = state::find_package(&app_state, pkg_name)?
        .ok_or_else(|| anyhow::anyhow!("Package '{}' is not installed.", pkg_name))?;

    out.print(pkg.source.to_string());
    if let Some(ref mp) = pkg.marketplace {
        out.print(format!("  From marketplace: {}", mp));
    }
    if let Some(ref pn) = pkg.plugin_name {
        out.print(format!("  Plugin: {}", pn));
    }

    // Re-discover content from the clone
    let clone_root = clone_root_for(&pkg.clone_path)?;
    if clone_root.exists() {
        if pkg.marketplace.is_some() {
            // For marketplace plugins, resolve the content directory.
            // Relative plugins have a plugin_path subdirectory within the clone.
            let content_dir =
                discovery::resolve_plugin_dir(&clone_root, pkg.plugin_path.as_deref())?;
            let items = discovery::discover_content(&content_dir)?;
            print_content_list(&items, out);
        } else {
            let manifest = load_manifest(&clone_root);
            if let Ok(manifest) = manifest {
                let items = collect_content_items(&manifest);
                print_content_list(&items, out);
            }
        }
    }

    Ok(())
}

fn print_content_list(items: &[crate::types::content::ContentItem], out: &Output) {
    let commands: Vec<_> = items
        .iter()
        .filter(|i| i.content_type == ContentType::Command)
        .collect();
    let skills: Vec<_> = items
        .iter()
        .filter(|i| i.content_type == ContentType::Skill)
        .collect();
    let agents: Vec<_> = items
        .iter()
        .filter(|i| i.content_type == ContentType::Agent)
        .collect();

    if !commands.is_empty() {
        out.print("\n  Commands:");
        for item in &commands {
            out.print(format!("    {}", item.name));
        }
    }
    if !skills.is_empty() {
        out.print("\n  Skills:");
        for item in &skills {
            out.print(format!("    {}", item.name));
        }
    }
    if !agents.is_empty() {
        out.print("\n  Agents:");
        for item in &agents {
            out.print(format!("    {}", item.name));
        }
    }
}

/// For display purposes only: resolve a relative plugin source to its directory.
/// Returns None for external sources (we don't want to clone just to display).
fn resolve_plugin_dir_for_display(
    ps: &marketplace::PluginSource,
    mp_clone_root: &std::path::Path,
) -> Option<std::path::PathBuf> {
    match ps {
        marketplace::PluginSource::Relative { path } => {
            discovery::resolve_plugin_dir_lossy(mp_clone_root, path)
        }
        _ => None,
    }
}
