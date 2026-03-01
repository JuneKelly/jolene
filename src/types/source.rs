use std::path::PathBuf;

use anyhow::{bail, Result};

/// The source from which a package is installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A GitHub repository, addressed as `owner/repo`.
    GitHub { owner: String, repo: String },
    /// A local git repository on disk.
    Local(PathBuf),
    /// An arbitrary remote git URL.
    Url(String),
}

impl Source {
    /// Parse `owner/repo` into a [`Source::GitHub`] variant.
    pub fn from_github(s: &str) -> Result<Self> {
        let parts: Vec<&str> = s.splitn(2, '/').collect();
        match parts.as_slice() {
            [owner, repo] if !owner.is_empty() && !repo.is_empty() => Ok(Source::GitHub {
                owner: owner.to_string(),
                repo: repo.to_string(),
            }),
            _ => bail!("--github expects Owner/repo format, got '{}'", s),
        }
    }

    /// The git URL used to clone this source.
    pub fn clone_url(&self) -> String {
        match self {
            Source::GitHub { owner, repo } => {
                format!("https://github.com/{}/{}.git", owner, repo)
            }
            Source::Local(path) => path.to_string_lossy().into_owned(),
            Source::Url(url) => url.clone(),
        }
    }

    /// Path relative to `~/.jolene/repos/`, always two components:
    /// - GitHub: `{owner}/{repo}`
    /// - Local:  `local/{dirname}`
    /// - Url:    `remote/{sanitized}`
    pub fn store_key(&self) -> String {
        match self {
            Source::GitHub { owner, repo } => format!("{}/{}", owner, repo),
            Source::Local(path) => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "unnamed".to_string());
                format!("local/{}", name)
            }
            Source::Url(url) => format!("remote/{}", sanitize_url(url)),
        }
    }

    /// Human-readable display string, stored as `source` in state.toml.
    pub fn display(&self) -> String {
        match self {
            Source::GitHub { owner, repo } => format!("{}/{}", owner, repo),
            Source::Local(path) => path.to_string_lossy().into_owned(),
            Source::Url(url) => url.clone(),
        }
    }

    /// The `source_kind` value written to state.toml.
    pub fn kind(&self) -> &'static str {
        match self {
            Source::GitHub { .. } => "github",
            Source::Local(_) => "local",
            Source::Url(_) => "url",
        }
    }
}

/// Derive a filesystem-safe single-component key from a git URL.
///
/// Strips the scheme and `.git` suffix, then replaces every character that
/// isn't alphanumeric, `-`, or `.` with `-`, and collapses consecutive dashes.
///
/// Examples:
/// - `https://gitlab.com/foo/bar.git` → `gitlab.com-foo-bar`
/// - `git@github.com:alice/tools.git` → `git-github.com-alice-tools`
fn sanitize_url(url: &str) -> String {
    let without_scheme = url
        .find("://")
        .map(|i| &url[i + 3..])
        .unwrap_or(url);

    let without_git = without_scheme
        .strip_suffix(".git")
        .unwrap_or(without_scheme);

    without_git
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '.' { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- GitHub ---

    #[test]
    fn github_from_str() {
        let s = Source::from_github("junebug/review-tools").unwrap();
        assert_eq!(s, Source::GitHub { owner: "junebug".into(), repo: "review-tools".into() });
    }

    #[test]
    fn github_clone_url() {
        let s = Source::from_github("junebug/review-tools").unwrap();
        assert_eq!(s.clone_url(), "https://github.com/junebug/review-tools.git");
    }

    #[test]
    fn github_store_key() {
        let s = Source::from_github("junebug/review-tools").unwrap();
        assert_eq!(s.store_key(), "junebug/review-tools");
    }

    #[test]
    fn github_display() {
        let s = Source::from_github("junebug/review-tools").unwrap();
        assert_eq!(s.display(), "junebug/review-tools");
    }

    #[test]
    fn github_kind() {
        let s = Source::from_github("junebug/review-tools").unwrap();
        assert_eq!(s.kind(), "github");
    }

    #[test]
    fn github_missing_slash_errors() {
        assert!(Source::from_github("junebug").is_err());
    }

    #[test]
    fn github_empty_owner_errors() {
        assert!(Source::from_github("/repo").is_err());
    }

    #[test]
    fn github_empty_repo_errors() {
        assert!(Source::from_github("author/").is_err());
    }

    #[test]
    fn github_extra_slash_kept_in_repo() {
        // splitn(2) puts everything after the first slash into repo
        let s = Source::from_github("a/b/c").unwrap();
        assert_eq!(s, Source::GitHub { owner: "a".into(), repo: "b/c".into() });
    }

    // --- Local ---

    #[test]
    fn local_store_key() {
        let s = Source::Local(PathBuf::from("/Users/junebug/my-pkg"));
        assert_eq!(s.store_key(), "local/my-pkg");
    }

    #[test]
    fn local_clone_url_is_path() {
        let s = Source::Local(PathBuf::from("/Users/junebug/my-pkg"));
        assert_eq!(s.clone_url(), "/Users/junebug/my-pkg");
    }

    #[test]
    fn local_display_is_path() {
        let s = Source::Local(PathBuf::from("/Users/junebug/my-pkg"));
        assert_eq!(s.display(), "/Users/junebug/my-pkg");
    }

    #[test]
    fn local_kind() {
        let s = Source::Local(PathBuf::from("/path/to/pkg"));
        assert_eq!(s.kind(), "local");
    }

    // --- Url ---

    #[test]
    fn url_store_key_https() {
        let s = Source::Url("https://gitlab.com/foo/bar.git".to_string());
        assert_eq!(s.store_key(), "remote/gitlab.com-foo-bar");
    }

    #[test]
    fn url_store_key_ssh() {
        let s = Source::Url("git@github.com:alice/tools.git".to_string());
        assert_eq!(s.store_key(), "remote/git-github.com-alice-tools");
    }

    #[test]
    fn url_clone_url_is_identity() {
        let url = "https://gitlab.com/foo/bar.git".to_string();
        let s = Source::Url(url.clone());
        assert_eq!(s.clone_url(), url);
    }

    #[test]
    fn url_display_is_identity() {
        let url = "https://gitlab.com/foo/bar.git".to_string();
        let s = Source::Url(url.clone());
        assert_eq!(s.display(), url);
    }

    #[test]
    fn url_kind() {
        let s = Source::Url("https://example.com/repo.git".to_string());
        assert_eq!(s.kind(), "url");
    }

    // --- sanitize_url ---

    #[test]
    fn sanitize_strips_scheme_and_git() {
        assert_eq!(sanitize_url("https://gitlab.com/foo/bar.git"), "gitlab.com-foo-bar");
    }

    #[test]
    fn sanitize_no_trailing_git() {
        assert_eq!(sanitize_url("https://example.com/repo"), "example.com-repo");
    }

    #[test]
    fn sanitize_collapses_dashes() {
        // Consecutive separators (e.g. `//`) become a single dash.
        assert_eq!(sanitize_url("https://example.com//repo.git"), "example.com-repo");
    }
}
