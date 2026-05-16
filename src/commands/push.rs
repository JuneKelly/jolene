use anyhow::{Result, bail};
use chrono::Utc;

use crate::config;
use crate::git;
use crate::output::Output;
use crate::state;

const DEFAULT_MESSAGE: &str = "jolene push: update bundle content";

pub fn run(bundle: &str, message: Option<&str>, dry_run: bool, out: &Output) -> Result<()> {
    let (_lock, mut app_state) = state::StateLock::acquire_and_load()?;

    let pkg = state::find_bundle(&app_state, bundle)?;
    let pkg = match pkg {
        Some(p) => p,
        None => bail!("Bundle '{}' is not installed.", bundle),
    };

    // Block marketplace plugins: shared clones, user doesn't own the upstream.
    if pkg.marketplace.is_some() {
        bail!(
            "Cannot push marketplace plugin '{}'.\n  \
             Marketplace plugins share clones and jolene does not own their upstream.\n  \
             Fork the plugin into its own repo and install it natively to use push.",
            pkg.source
        );
    }

    let source = pkg.source.clone();
    let clone_root = config::clone_root_for(&pkg.clone_path)?;

    // Warn about templated items (edits in rendered/ won't be pushed).
    let templated_srcs: Vec<String> = pkg
        .installations
        .iter()
        .flat_map(|inst| &inst.symlinks)
        .filter(|entry| entry.templated)
        .map(|entry| entry.src.clone())
        .collect();

    if !templated_srcs.is_empty() {
        out.print("Warning: some items are templated (symlinked from rendered/, not repos/).");
        out.print("  Edits to these files will NOT be included in the push:");
        for src in &templated_srcs {
            out.print(format!("    {}", src));
        }
        out.print("");
    }

    // Check for changes in the working tree.
    let status = git::status_short(&clone_root)?;
    if status.is_empty() {
        out.print(format!("Nothing to push: working tree is clean for {}.", source));
        return Ok(());
    }

    out.print(format!("Changes in {}:", source));
    // Print each line of git status output indented.
    for line in status.lines() {
        out.print(format!("  {}", line));
    }

    if dry_run {
        out.print("\nDry run: would commit and push the above changes.");
        return Ok(());
    }

    let final_message = message.unwrap_or(DEFAULT_MESSAGE);

    git::add_all(&clone_root)?;
    git::commit(&clone_root, final_message)?;

    // Push, but handle failure gracefully: commit is local, don't update state.
    if let Err(e) = git::push(&clone_root) {
        let display = config::display_path(&clone_root);
        bail!(
            "{}\n  Commit was created locally but push failed.\n  \
             Resolve manually in: {}",
            e,
            display
        );
    }

    // Update state with new commit hash.
    let new_commit = git::full_commit(&clone_root)?;
    let branch = git::current_branch(&clone_root)?;
    let short_commit = new_commit[..new_commit.len().min(7)].to_string();

    let pkg_mut = state::find_bundle_mut(&mut app_state, &source)?.unwrap();
    pkg_mut.commit = new_commit;
    pkg_mut.updated_at = Utc::now();

    state::save(&app_state)?;

    out.print(format!("\nPushed {} ({}@{})", source, branch, short_commit));
    Ok(())
}
