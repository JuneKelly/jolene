use std::fmt;
use std::str::FromStr;

use anyhow::{bail, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub author: String,
    pub repo: String,
}

impl Source {
    pub fn parse(s: &str) -> Result<Source> {
        let parts: Vec<&str> = s.splitn(2, '/').collect();
        match parts.as_slice() {
            [author, repo] if !author.is_empty() && !repo.is_empty() => Ok(Source {
                author: author.to_string(),
                repo: repo.to_string(),
            }),
            _ => bail!("Invalid source '{}'. Expected Author/repo format.", s),
        }
    }

    pub fn github_url(&self) -> String {
        format!("https://github.com/{}/{}.git", self.author, self.repo)
    }

    pub fn display_name(&self) -> String {
        format!("{}/{}", self.author, self.repo)
    }

}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

impl FromStr for Source {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        Source::parse(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid() {
        let s = Source::parse("junebug/review-tools").unwrap();
        assert_eq!(s.author, "junebug");
        assert_eq!(s.repo, "review-tools");
    }

    #[test]
    fn parse_missing_slash_errors() {
        assert!(Source::parse("junebug").is_err());
    }

    #[test]
    fn parse_empty_author_errors() {
        assert!(Source::parse("/repo").is_err());
    }

    #[test]
    fn parse_empty_repo_errors() {
        assert!(Source::parse("author/").is_err());
    }

    #[test]
    fn parse_empty_string_errors() {
        assert!(Source::parse("").is_err());
    }

    #[test]
    fn parse_extra_slash_kept_in_repo() {
        // splitn(2, '/') puts everything after the first slash into repo
        let s = Source::parse("a/b/c").unwrap();
        assert_eq!(s.author, "a");
        assert_eq!(s.repo, "b/c");
    }

    #[test]
    fn github_url() {
        let s = Source::parse("junebug/review-tools").unwrap();
        assert_eq!(
            s.github_url(),
            "https://github.com/junebug/review-tools.git"
        );
    }

    #[test]
    fn display_name_roundtrips() {
        let s = Source::parse("junebug/review-tools").unwrap();
        assert_eq!(s.display_name(), "junebug/review-tools");
    }

    #[test]
    fn display_impl_matches_display_name() {
        let s = Source::parse("junebug/review-tools").unwrap();
        assert_eq!(format!("{s}"), s.display_name());
    }

    #[test]
    fn from_str_parses_correctly() {
        let s: Source = "junebug/review-tools".parse().unwrap();
        assert_eq!(s.author, "junebug");
        assert_eq!(s.repo, "review-tools");
    }
}
