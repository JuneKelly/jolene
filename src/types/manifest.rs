use serde::Deserialize;

/// Top-level jolene.toml structure.
#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub package: Package,
    pub content: ContentDecl,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Package {
    #[allow(dead_code)]
    pub name: String,
    pub description: String,
    pub version: String,
    pub authors: Vec<String>,
    pub license: String,
    pub urls: Option<Urls>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Urls {
    pub repository: Option<String>,
    pub homepage: Option<String>,
}

/// Declares which content items the package provides.
/// At least one field must be non-empty.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ContentDecl {
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub agents: Vec<String>,
}

impl ContentDecl {
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty() && self.skills.is_empty() && self.agents.is_empty()
    }

    /// Human-readable summary, e.g. "2 commands, 1 skill".
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if !self.commands.is_empty() {
            let n = self.commands.len();
            parts.push(format!("{} {}", n, if n == 1 { "command" } else { "commands" }));
        }
        if !self.skills.is_empty() {
            let n = self.skills.len();
            parts.push(format!("{} {}", n, if n == 1 { "skill" } else { "skills" }));
        }
        if !self.agents.is_empty() {
            let n = self.agents.len();
            parts.push(format!("{} {}", n, if n == 1 { "agent" } else { "agents" }));
        }
        if parts.is_empty() {
            "no content".to_string()
        } else {
            parts.join(", ")
        }
    }
}
