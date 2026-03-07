use std::fs::File;
use std::io::Write;

use anyhow::{Context, Result, bail};
use tempfile::NamedTempFile;

use crate::config::{jolene_root, legacy_state_file, state_file};
use crate::types::state::{PackageState, State};

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

        file.lock()
            .context("Failed to acquire state lock")?;

        Ok(StateLock { _file: file })
    }
}

pub fn load() -> Result<State> {
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
            return Ok(state);
        }

        return Ok(State::default());
    }

    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read state file {}", path.display()))?;

    serde_json::from_str(&text)
        .with_context(|| format!("Failed to parse state file {}", path.display()))
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
    tmp.persist(&path)
        .with_context(|| format!("Failed to persist state file to {}", path.display()))?;

    Ok(())
}

/// Find a package by its source identifier (exact match on `pkg.source`).
///
/// For GitHub packages, also supports a bare repo name (e.g. `"tools"` matches
/// `"alice/tools"`). This short-name lookup is GitHub-specific: local paths and
/// URLs always contain `/` and are matched exactly by the first branch, but their
/// components have no meaningful short form.
///
/// For marketplace plugins, also matches by `plugin_name` (e.g. `"review-plugin"`
/// matches a package with `plugin_name: Some("review-plugin")`).
pub fn find_package<'a>(state: &'a State, name: &str) -> Result<Option<&'a PackageState>> {
    if name.contains('/') {
        return Ok(state.packages.iter().find(|p| p.source == name));
    }

    // Short-name lookup: matches GitHub repo component or marketplace plugin_name.
    let matches: Vec<_> = state
        .packages
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
                "Ambiguous name '{}'. Multiple matches:\n{}\n\n  Use the full owner/repo format.",
                name,
                names.join("\n")
            );
        }
    }
}

/// Mutable variant of find_package.
pub fn find_package_mut<'a>(
    state: &'a mut State,
    name: &str,
) -> Result<Option<&'a mut PackageState>> {
    if name.contains('/') {
        return Ok(state.packages.iter_mut().find(|p| p.source == name));
    }

    // Short-name lookup: matches GitHub repo component or marketplace plugin_name.
    let matches: Vec<_> = state
        .packages
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
            Ok(state.packages.iter_mut().find(|p| p.source == source))
        }
        _ => {
            let names: Vec<_> = matches.iter().map(|s| format!("  {}", s)).collect();
            bail!(
                "Ambiguous name '{}'. Multiple matches:\n{}\n\n  Use the full owner/repo format.",
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
    use crate::types::state::{PackageState, SourceKind, State};

    use super::find_package;

    fn make_pkg(source: &str) -> PackageState {
        let src = Source::from_github(source).unwrap();
        PackageState {
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
        }
    }

    fn make_state(sources: &[&str]) -> State {
        State {
            packages: sources.iter().map(|s| make_pkg(s)).collect(),
        }
    }

    #[test]
    fn find_by_full_name() {
        let state = make_state(&["alice/tools", "bob/utils"]);
        let pkg = find_package(&state, "alice/tools").unwrap().unwrap();
        assert_eq!(pkg.source, "alice/tools");
    }

    #[test]
    fn find_by_repo_name_unambiguous() {
        let state = make_state(&["alice/tools", "bob/utils"]);
        let pkg = find_package(&state, "tools").unwrap().unwrap();
        assert_eq!(pkg.source, "alice/tools");
    }

    #[test]
    fn find_by_repo_name_ambiguous_errors() {
        let state = make_state(&["alice/tools", "bob/tools"]);
        assert!(find_package(&state, "tools").is_err());
    }

    #[test]
    fn find_missing_full_name_returns_none() {
        let state = make_state(&["alice/tools"]);
        assert!(find_package(&state, "alice/other").unwrap().is_none());
    }

    #[test]
    fn find_missing_repo_name_returns_none() {
        let state = make_state(&["alice/tools"]);
        assert!(find_package(&state, "other").unwrap().is_none());
    }

    #[test]
    fn find_in_empty_state_returns_none() {
        let state = make_state(&[]);
        assert!(find_package(&state, "alice/tools").unwrap().is_none());
    }

    fn make_marketplace_pkg(source: &str, plugin_name: &str) -> PackageState {
        let src = Source::from_github("acme/marketplace").unwrap();
        PackageState {
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
        }
    }

    #[test]
    fn find_marketplace_plugin_by_plugin_name() {
        let state = State {
            packages: vec![make_marketplace_pkg("acme/marketplace::review", "review")],
        };
        let pkg = find_package(&state, "review").unwrap().unwrap();
        assert_eq!(pkg.source, "acme/marketplace::review");
    }

    #[test]
    fn find_marketplace_plugin_by_full_source() {
        let state = State {
            packages: vec![make_marketplace_pkg("acme/marketplace::review", "review")],
        };
        let pkg = find_package(&state, "acme/marketplace::review")
            .unwrap()
            .unwrap();
        assert_eq!(pkg.plugin_name.as_deref(), Some("review"));
    }

    #[test]
    fn find_marketplace_plugin_ambiguous_with_repo() {
        let state = State {
            packages: vec![
                make_pkg("alice/review"),
                make_marketplace_pkg("acme/marketplace::review", "review"),
            ],
        };
        assert!(find_package(&state, "review").is_err());
    }

    #[test]
    fn find_marketplace_plugin_missing_returns_none() {
        let state = State {
            packages: vec![make_marketplace_pkg("acme/marketplace::review", "review")],
        };
        assert!(find_package(&state, "deploy").unwrap().is_none());
    }
}
