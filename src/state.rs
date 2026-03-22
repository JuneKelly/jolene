use std::fs::{File, Permissions};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;

use anyhow::{Context, Result, bail};
use tempfile::NamedTempFile;

use crate::config::{jolene_root, legacy_state_file, state_file};
use crate::types::state::{BundleState, State};

/// Advisory file lock for serializing concurrent jolene processes.
///
/// Acquires an exclusive lock on `~/.jolene/.lock` via `File::lock()`.
/// The lock is released when this value is dropped (file close releases it).
pub struct StateLock {
    _file: File,
}

impl StateLock {
    pub fn acquire() -> Result<Self> {
        let root = jolene_root()?;
        std::fs::create_dir_all(&root)
            .with_context(|| format!("Failed to create jolene directory {}", root.display()))?;

        let lock_path = root.join(".lock");
        let file = File::create(&lock_path)
            .with_context(|| format!("Failed to create lock file {}", lock_path.display()))?;
        file.set_permissions(Permissions::from_mode(0o600))
            .with_context(|| format!("Failed to set lock file permissions {}", lock_path.display()))?;

        file.lock()
            .context("Failed to acquire state lock")?;

        Ok(StateLock { _file: file })
    }

    /// Acquire the lock and load state atomically.
    ///
    /// Prefer this over separate `acquire()` + `load()` calls for mutating
    /// commands to ensure state is never loaded without holding the lock.
    /// Automatically migrates state.json from the old `packages` key to `bundles`
    /// if needed, printing a message to stderr.
    pub fn acquire_and_load() -> Result<(Self, State)> {
        let lock = Self::acquire()?;
        let (state, needs_migration) = load_with_migration_flag()?;
        if needs_migration {
            eprintln!("Migrating state.json: \"packages\" → \"bundles\"");
            save(&state)?;
        }
        Ok((lock, state))
    }
}

pub fn load() -> Result<State> {
    let (state, _) = load_with_migration_flag()?;
    Ok(state)
}

/// Load state.json, also returning whether the file used the old `"packages"` key.
/// This avoids a second read of the file in `acquire_and_load`.
fn load_with_migration_flag() -> Result<(State, bool)> {
    let path = state_file()?;

    if !path.exists() {
        // Migrate from legacy state.toml if it exists.
        let legacy = legacy_state_file()?;
        if legacy.exists() {
            eprintln!("Migrating state.toml → state.json");
            let text = std::fs::read_to_string(&legacy).with_context(|| {
                format!("Failed to read legacy state file {}", legacy.display())
            })?;
            let state: State = toml::from_str(&text).with_context(|| {
                format!("Failed to parse legacy state file {}", legacy.display())
            })?;
            save(&state)?;
            let old = legacy.with_file_name("_old_state.toml");
            std::fs::rename(&legacy, &old).with_context(|| {
                format!("Failed to rename legacy state file to {}", old.display())
            })?;
            return Ok((state, false));
        }

        return Ok((State::default(), false));
    }

    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read state file {}", path.display()))?;

    // Check for the old "packages" key before deserializing, so we can report
    // migration without a second file read.
    let raw: serde_json::Value = serde_json::from_str(&text)
        .with_context(|| format!("Failed to parse state file {}", path.display()))?;
    let needs_migration = raw.get("packages").is_some() && raw.get("bundles").is_none();

    let state: State = serde_json::from_value(raw)
        .with_context(|| format!("Failed to parse state file {}", path.display()))?;

    Ok((state, needs_migration))
}

pub fn save(state: &State) -> Result<()> {
    let root = jolene_root()?;
    std::fs::create_dir_all(&root)
        .with_context(|| format!("Failed to create jolene directory {}", root.display()))?;

    let path = state_file()?;
    let text = serde_json::to_string_pretty(state).context("Failed to serialize state")?;

    // Atomic write: write to a temp file in the same directory, then rename.
    let dir = path.parent().unwrap_or(&root);
    let mut tmp = NamedTempFile::new_in(dir).context("Failed to create temp file for state")?;
    tmp.write_all(text.as_bytes())
        .context("Failed to write state to temp file")?;
    tmp.as_file()
        .set_permissions(Permissions::from_mode(0o600))
        .context("Failed to set state file permissions")?;
    tmp.persist(&path)
        .with_context(|| format!("Failed to persist state file to {}", path.display()))?;

    Ok(())
}

/// Find a bundle by its source identifier (exact match on `pkg.source`).
///
/// For GitHub bundles, also supports a bare repo name (e.g. `"tools"` matches
/// `"alice/tools"`). This short-name lookup is GitHub-specific: local paths and
/// URLs always contain `/` and are matched exactly by the first branch, but their
/// components have no meaningful short form.
///
/// For marketplace plugins, also matches by `plugin_name` (e.g. `"review-plugin"`
/// matches a bundle with `plugin_name: Some("review-plugin")`).
pub fn find_bundle<'a>(state: &'a State, name: &str) -> Result<Option<&'a BundleState>> {
    if name.contains('/') {
        return Ok(state.bundles.iter().find(|p| p.source == name));
    }

    // Short-name lookup: matches GitHub repo component or marketplace plugin_name.
    let matches: Vec<_> = state
        .bundles
        .iter()
        .filter(|p| {
            p.source.split('/').nth(1) == Some(name) || p.plugin_name.as_deref() == Some(name)
        })
        .collect();

    match matches.as_slice() {
        [] => Ok(None),
        [one] => Ok(Some(one)),
        _ => {
            let names: Vec<_> = matches.iter().map(|p| format!("  {}", p.source)).collect();
            bail!(
                "Ambiguous name '{}'. Multiple matches:\n{}\n\n  Use the full identifier:\n    owner/repo (native bundles)\n    org/marketplace::plugin-name (marketplace plugins)",
                name,
                names.join("\n")
            );
        }
    }
}

/// Mutable variant of find_bundle.
pub fn find_bundle_mut<'a>(
    state: &'a mut State,
    name: &str,
) -> Result<Option<&'a mut BundleState>> {
    if name.contains('/') {
        return Ok(state.bundles.iter_mut().find(|p| p.source == name));
    }

    // Short-name lookup: matches GitHub repo component or marketplace plugin_name.
    let matches: Vec<_> = state
        .bundles
        .iter()
        .filter(|p| {
            p.source.split('/').nth(1) == Some(name) || p.plugin_name.as_deref() == Some(name)
        })
        .map(|p| p.source.clone())
        .collect();

    match matches.as_slice() {
        [] => Ok(None),
        [source] => {
            let source = source.clone();
            Ok(state.bundles.iter_mut().find(|p| p.source == source))
        }
        _ => {
            let names: Vec<_> = matches.iter().map(|s| format!("  {}", s)).collect();
            bail!(
                "Ambiguous name '{}'. Multiple matches:\n{}\n\n  Use the full identifier:\n    owner/repo (native bundles)\n    org/marketplace::plugin-name (marketplace plugins)",
                name,
                names.join("\n")
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use crate::types::source::Source;
    use crate::types::state::{BundleState, SourceKind, State};

    use super::find_bundle;

    fn make_bundle(source: &str) -> BundleState {
        let src = Source::from_github(source).unwrap();
        BundleState {
            source_kind: SourceKind::GitHub,
            source: source.to_string(),
            clone_url: Some(format!("https://github.com/{}.git", source)),
            clone_path: format!("repos/{}", src.store_key()),
            branch: "main".to_string(),
            commit: "abc123".to_string(),
            installed_at: Utc::now(),
            updated_at: Utc::now(),
            installations: vec![],
            marketplace: None,
            plugin_name: None,
            plugin_path: None,
            prefix: None,
            var_overrides: None,
        }
    }

    fn make_state(sources: &[&str]) -> State {
        State {
            bundles: sources.iter().map(|s| make_bundle(s)).collect(),
        }
    }

    #[test]
    fn find_by_full_name() {
        let state = make_state(&["alice/tools", "bob/utils"]);
        let pkg = find_bundle(&state, "alice/tools").unwrap().unwrap();
        assert_eq!(pkg.source, "alice/tools");
    }

    #[test]
    fn find_by_repo_name_unambiguous() {
        let state = make_state(&["alice/tools", "bob/utils"]);
        let pkg = find_bundle(&state, "tools").unwrap().unwrap();
        assert_eq!(pkg.source, "alice/tools");
    }

    #[test]
    fn find_by_repo_name_ambiguous_errors() {
        let state = make_state(&["alice/tools", "bob/tools"]);
        assert!(find_bundle(&state, "tools").is_err());
    }

    #[test]
    fn find_missing_full_name_returns_none() {
        let state = make_state(&["alice/tools"]);
        assert!(find_bundle(&state, "alice/other").unwrap().is_none());
    }

    #[test]
    fn find_missing_repo_name_returns_none() {
        let state = make_state(&["alice/tools"]);
        assert!(find_bundle(&state, "other").unwrap().is_none());
    }

    #[test]
    fn find_in_empty_state_returns_none() {
        let state = make_state(&[]);
        assert!(find_bundle(&state, "alice/tools").unwrap().is_none());
    }

    fn make_marketplace_bundle(source: &str, plugin_name: &str) -> BundleState {
        let src = Source::from_github("acme/marketplace").unwrap();
        BundleState {
            source_kind: SourceKind::GitHub,
            source: source.to_string(),
            clone_url: Some("https://github.com/acme/marketplace.git".to_string()),
            clone_path: format!("repos/{}", src.store_key()),
            branch: "main".to_string(),
            commit: "abc123".to_string(),
            installed_at: Utc::now(),
            updated_at: Utc::now(),
            installations: vec![],
            marketplace: Some("acme/marketplace".to_string()),
            plugin_name: Some(plugin_name.to_string()),
            plugin_path: Some(format!("plugins/{}", plugin_name)),
            prefix: None,
            var_overrides: None,
        }
    }

    #[test]
    fn find_marketplace_plugin_by_plugin_name() {
        let state = State {
            bundles: vec![make_marketplace_bundle("acme/marketplace::review", "review")],
        };
        let pkg = find_bundle(&state, "review").unwrap().unwrap();
        assert_eq!(pkg.source, "acme/marketplace::review");
    }

    #[test]
    fn find_marketplace_plugin_by_full_source() {
        let state = State {
            bundles: vec![make_marketplace_bundle("acme/marketplace::review", "review")],
        };
        let pkg = find_bundle(&state, "acme/marketplace::review")
            .unwrap()
            .unwrap();
        assert_eq!(pkg.plugin_name.as_deref(), Some("review"));
    }

    #[test]
    fn find_marketplace_plugin_ambiguous_with_repo() {
        let state = State {
            bundles: vec![
                make_bundle("alice/review"),
                make_marketplace_bundle("acme/marketplace::review", "review"),
            ],
        };
        assert!(find_bundle(&state, "review").is_err());
    }

    #[test]
    fn find_marketplace_plugin_missing_returns_none() {
        let state = State {
            bundles: vec![make_marketplace_bundle("acme/marketplace::review", "review")],
        };
        assert!(find_bundle(&state, "deploy").unwrap().is_none());
    }
}
