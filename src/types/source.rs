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

    pub fn clone_subpath(&self) -> String {
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
