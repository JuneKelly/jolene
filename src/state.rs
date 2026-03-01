use std::io::Write;

use anyhow::{bail, Context, Result};
use tempfile::NamedTempFile;

use crate::config::{jolene_root, state_file};
use crate::types::state::{PackageState, State};

pub fn load() -> Result<State> {
    let path = state_file()?;

    if !path.exists() {
        return Ok(State::default());
    }

    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read state file {}", path.display()))?;

    toml::from_str(&text).with_context(|| format!("Failed to parse state file {}", path.display()))
}

pub fn save(state: &State) -> Result<()> {
    let root = jolene_root()?;
    std::fs::create_dir_all(&root)
        .with_context(|| format!("Failed to create jolene directory {}", root.display()))?;

    let path = state_file()?;
    let text = toml::to_string_pretty(state).context("Failed to serialize state")?;

    // Atomic write: write to a temp file in the same directory, then rename.
    let dir = path.parent().unwrap_or(&root);
    let mut tmp = NamedTempFile::new_in(dir).context("Failed to create temp file for state")?;
    tmp.write_all(text.as_bytes())
        .context("Failed to write state to temp file")?;
    tmp.persist(&path)
        .with_context(|| format!("Failed to persist state file to {}", path.display()))?;

    Ok(())
}

/// Find a package by "Author/repo" or bare "repo" (errors if ambiguous).
pub fn find_package<'a>(state: &'a State, name: &str) -> Result<Option<&'a PackageState>> {
    if name.contains('/') {
        return Ok(state.packages.iter().find(|p| p.source == name));
    }

    let matches: Vec<_> = state
        .packages
        .iter()
        .filter(|p| p.source.split('/').nth(1) == Some(name))
        .collect();

    match matches.as_slice() {
        [] => Ok(None),
        [one] => Ok(Some(one)),
        _ => {
            let names: Vec<_> = matches.iter().map(|p| format!("  {}", p.source)).collect();
            bail!(
                "Ambiguous name '{}'. Multiple matches:\n{}\n\n  Use the full Author/repo format.",
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

    let matches: Vec<_> = state
        .packages
        .iter()
        .filter(|p| p.source.split('/').nth(1) == Some(name))
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
                "Ambiguous name '{}'. Multiple matches:\n{}\n\n  Use the full Author/repo format.",
                name,
                names.join("\n")
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use crate::types::state::{PackageState, State};

    use super::find_package;

    fn make_pkg(source: &str) -> PackageState {
        PackageState {
            source: source.to_string(),
            clone_path: format!("repos/{}", source),
            branch: "main".to_string(),
            commit: "abc123".to_string(),
            installed_at: Utc::now(),
            updated_at: Utc::now(),
            installations: vec![],
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
}
