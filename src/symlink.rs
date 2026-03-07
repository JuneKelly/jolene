use std::collections::HashMap;
use std::os::unix::fs as unix_fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::config::{display_path, jolene_root};
use crate::types::content::ContentItem;
use crate::types::state::SymlinkEntry;

pub enum ConflictCheck {
    /// No destination exists — safe to create.
    Clear,
    /// Destination is a jolene symlink from the same package — skip.
    AlreadyInstalled,
    /// Destination is a jolene symlink from a different package.
    PackageConflict { store_key: String },
    /// Destination exists but is not a jolene-managed symlink.
    UserConflict,
}

/// Check the destination path for conflicts before creating a symlink.
pub fn check_conflict(dst: &Path, current_source: &str) -> Result<ConflictCheck> {
    if !dst.exists() && !dst.is_symlink() {
        return Ok(ConflictCheck::Clear);
    }

    if dst.is_symlink() {
        let target = std::fs::read_link(dst)
            .with_context(|| format!("Failed to read symlink {}", dst.display()))?;

        if is_jolene_symlink(&target)? {
            match package_from_symlink(&target) {
                Some(key) if key == current_source => {
                    return Ok(ConflictCheck::AlreadyInstalled);
                }
                Some(key) => {
                    return Ok(ConflictCheck::PackageConflict { store_key: key });
                }
                None => {}
            }
        }
    }

    Ok(ConflictCheck::UserConflict)
}

/// True if the symlink target points into ~/.jolene/.
pub fn is_jolene_symlink(target: &Path) -> Result<bool> {
    let root = jolene_root()?;
    Ok(target.starts_with(&root))
}

/// Extract the 64-char store-key hash from a path like ~/.jolene/repos/{hash}/...
pub fn package_from_symlink(target: &Path) -> Option<String> {
    let root = jolene_root().ok()?;
    let repos = root.join("repos");
    let rel = target.strip_prefix(&repos).ok()?;
    let hash = rel.components().next()?.as_os_str().to_str()?;
    Some(hash.to_string())
}

/// A planned symlink operation.
pub struct SymlinkPlan {
    /// Absolute source path, used to create the symlink.
    pub src: PathBuf,
    pub dst: PathBuf,
    /// Source path relative to the clone root, stored in state.
    pub relative_src: String,
}

/// Build the symlink plan for a set of content items, checking for conflicts.
/// Returns an error on the first conflict encountered.
///
/// `display_names` maps store-key hashes to human-readable package names
/// for use in conflict error messages.
pub fn plan_symlinks(
    items: &[ContentItem],
    clone_root: &Path,
    target_root: &Path,
    target_slug: &str,
    package_source: &str,
    display_names: &HashMap<String, String>,
    prefix: Option<&str>,
) -> Result<Vec<SymlinkPlan>> {
    let mut plans = Vec::new();

    for item in items {
        let src = item.source_path(clone_root);
        let content_dir = target_root.join(item.content_type.dir_name());
        let dst = item.dest_path(&content_dir, prefix);
        let relative_src = item.relative_path().to_string_lossy().into_owned();

        match check_conflict(&dst, package_source)? {
            ConflictCheck::Clear => {
                plans.push(SymlinkPlan {
                    src,
                    dst,
                    relative_src,
                });
            }
            ConflictCheck::AlreadyInstalled => {
                // Already correct — skip silently.
            }
            ConflictCheck::PackageConflict { store_key } => {
                let name = display_names
                    .get(&store_key)
                    .map(|s| s.as_str())
                    .unwrap_or(&store_key);
                bail!(
                    "{} is already provided by {}\n\n  To resolve: jolene uninstall {} --from {}",
                    display_path(&dst),
                    name,
                    name,
                    target_slug
                );
            }
            ConflictCheck::UserConflict => {
                bail!(
                    "{} already exists and is not managed by jolene.\n  Remove or rename {}, then retry.",
                    display_path(&dst),
                    display_path(&dst)
                );
            }
        }
    }

    Ok(plans)
}

/// Execute symlink plans, rolling back all created symlinks on failure.
pub fn execute_symlinks(plans: &[SymlinkPlan]) -> Result<Vec<SymlinkEntry>> {
    let mut created: Vec<PathBuf> = Vec::new();
    let mut entries: Vec<SymlinkEntry> = Vec::new();

    for plan in plans {
        if let Some(parent) = plan.dst.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {}", parent.display()))?;

            // Verify the parent directory wasn't replaced with a symlink
            // between creation and use (TOCTOU mitigation).
            if parent.is_symlink() {
                bail!(
                    "Directory {} is a symlink, which is unexpected. Refusing to create symlink inside it.",
                    parent.display()
                );
            }
        }

        if let Err(e) = unix_fs::symlink(&plan.src, &plan.dst) {
            // Rollback everything created so far.
            let rolled_back = created.len();
            for path in &created {
                let _ = std::fs::remove_file(path);
            }
            bail!(
                "Failed to create symlink {} -> {}: {}\n  Rolled back {} symlink(s) that were created before the error.\n  No changes were made.",
                display_path(&plan.dst),
                plan.src.display(),
                e,
                rolled_back
            );
        }

        created.push(plan.dst.clone());
        entries.push(SymlinkEntry {
            src: plan.relative_src.clone(),
            dst: display_path(&plan.dst),
        });
    }

    Ok(entries)
}

/// Remove a symlink. Warns (returns Ok) if already gone.
pub fn remove_symlink(dst: &Path) -> Result<()> {
    if dst.is_symlink() || dst.exists() {
        std::fs::remove_file(dst)
            .with_context(|| format!("Failed to remove symlink {}", dst.display()))?;
    }
    // Already gone — silently ok per spec.
    Ok(())
}

/// Expand a `~/...` path to an absolute path.
pub fn expand_tilde(path: &str) -> Option<PathBuf> {
    if let Some(rest) = path.strip_prefix("~/") {
        crate::config::effective_home().map(|h| h.join(rest))
    } else if path == "~" {
        crate::config::effective_home()
    } else {
        Some(PathBuf::from(path))
    }
}
