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
