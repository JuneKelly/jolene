use std::fmt::Write as FmtWrite;
use std::path::PathBuf;

use anyhow::{Result, bail};
use sha2::{Digest, Sha256};

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
            [owner, repo] if !owner.is_empty() && !repo.is_empty() => {
                if s.chars().any(|c| c.is_whitespace()) {
                    bail!(
                        "--github owner/repo must not contain whitespace, got '{}'",
                        s
                    );
                }
                if repo.contains('/') {
                    bail!("--github repo must not contain '/', got '{}'", s);
                }
                Ok(Source::GitHub {
                    owner: owner.to_string(),
                    repo: repo.to_string(),
                })
            }
            _ => bail!("--github expects owner/repo format, got '{}'", s),
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

    /// The canonical string used as SHA256 input for the store key.
    ///
    /// Format:
    /// - GitHub: `github||owner/repo`
    /// - Local:  `local||/absolute/path`
    /// - Url:    `url||https://...`
    pub fn canonical_key(&self) -> String {
        match self {
            Source::GitHub { owner, repo } => format!("github||{}/{}", owner, repo),
            Source::Local(path) => format!("local||{}", path.to_string_lossy()),
            Source::Url(url) => format!("url||{}", url),
        }
    }

    /// 64-character lowercase hex SHA256 of `canonical_key()`.
    /// Used as the directory name under `~/.jolene/repos/`.
    pub fn store_key(&self) -> String {
        sha256_hex(&self.canonical_key())
    }

    /// Human-readable display string, stored as `source` in state.json.
    pub fn display(&self) -> String {
        match self {
            Source::GitHub { owner, repo } => format!("{}/{}", owner, repo),
            Source::Local(path) => path.to_string_lossy().into_owned(),
            Source::Url(url) => url.clone(),
        }
    }
}

fn sha256_hex(input: &str) -> String {
    let hash = Sha256::digest(input.as_bytes());
    hash.iter().fold(String::with_capacity(64), |mut s, b| {
        write!(s, "{:02x}", b).unwrap();
        s
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- GitHub ---

    #[test]
    fn github_from_str() {
        let s = Source::from_github("junebug/review-tools").unwrap();
        assert_eq!(
            s,
            Source::GitHub {
                owner: "junebug".into(),
                repo: "review-tools".into()
            }
        );
    }

    #[test]
    fn github_clone_url() {
        let s = Source::from_github("junebug/review-tools").unwrap();
        assert_eq!(s.clone_url(), "https://github.com/junebug/review-tools.git");
    }

    #[test]
    fn github_canonical_key() {
        let s = Source::from_github("junebug/review-tools").unwrap();
        assert_eq!(s.canonical_key(), "github||junebug/review-tools");
    }

    #[test]
    fn github_store_key_is_64_hex() {
        let s = Source::from_github("junebug/review-tools").unwrap();
        let key = s.store_key();
        assert_eq!(key.len(), 64);
        assert!(key.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn github_store_key_is_deterministic() {
        let a = Source::from_github("junebug/review-tools").unwrap();
        let b = Source::from_github("junebug/review-tools").unwrap();
        assert_eq!(a.store_key(), b.store_key());
    }

    #[test]
    fn github_display() {
        let s = Source::from_github("junebug/review-tools").unwrap();
        assert_eq!(s.display(), "junebug/review-tools");
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
    fn github_extra_slash_errors() {
        assert!(Source::from_github("a/b/c").is_err());
    }

    #[test]
    fn github_whitespace_in_owner_errors() {
        assert!(Source::from_github("june bug/repo").is_err());
    }

    #[test]
    fn github_whitespace_in_repo_errors() {
        assert!(Source::from_github("owner/my repo").is_err());
    }

    // --- Local ---

    #[test]
    fn local_canonical_key() {
        let s = Source::Local(PathBuf::from("/Users/junebug/my-pkg"));
        assert_eq!(s.canonical_key(), "local||/Users/junebug/my-pkg");
    }

    #[test]
    fn local_store_key_is_64_hex() {
        let s = Source::Local(PathBuf::from("/Users/junebug/my-pkg"));
        let key = s.store_key();
        assert_eq!(key.len(), 64);
        assert!(key.chars().all(|c| c.is_ascii_hexdigit()));
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

    // --- Url ---

    #[test]
    fn url_canonical_key() {
        let s = Source::Url("https://gitlab.com/foo/bar.git".to_string());
        assert_eq!(s.canonical_key(), "url||https://gitlab.com/foo/bar.git");
    }

    #[test]
    fn url_store_key_is_64_hex() {
        let s = Source::Url("https://gitlab.com/foo/bar.git".to_string());
        let key = s.store_key();
        assert_eq!(key.len(), 64);
        assert!(key.chars().all(|c| c.is_ascii_hexdigit()));
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
        assert_eq!(s.display(), "nope");
    }

    // --- Collision resistance ---

    #[test]
    fn different_sources_produce_different_keys() {
        let github = Source::from_github("alice/tools").unwrap();
        let local = Source::Local(PathBuf::from("/alice/tools"));
        let url = Source::Url("https://github.com/alice/tools.git".to_string());

        let keys = [github.store_key(), local.store_key(), url.store_key()];
        // All three must be distinct.
        assert_ne!(keys[0], keys[1]);
        assert_ne!(keys[0], keys[2]);
        assert_ne!(keys[1], keys[2]);
    }
}
