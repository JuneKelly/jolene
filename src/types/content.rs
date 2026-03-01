use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentType {
    Command,
    Skill,
    Agent,
}

#[allow(dead_code)]
impl ContentType {
    pub fn label(self) -> &'static str {
        match self {
            ContentType::Command => "command",
            ContentType::Skill => "skill",
            ContentType::Agent => "agent",
        }
    }

    pub fn label_plural(self) -> &'static str {
        match self {
            ContentType::Command => "commands",
            ContentType::Skill => "skills",
            ContentType::Agent => "agents",
        }
    }

    /// The directory name in both the package repo and the target config root.
    pub fn dir_name(self) -> &'static str {
        match self {
            ContentType::Command => "commands",
            ContentType::Skill => "skills",
            ContentType::Agent => "agents",
        }
    }
}

/// A single installable item from a package.
#[derive(Debug, Clone)]
pub struct ContentItem {
    pub content_type: ContentType,
    /// Name without extension (commands/agents) or directory name (skills).
    pub name: String,
}

impl ContentItem {
    pub fn new(content_type: ContentType, name: impl Into<String>) -> ContentItem {
        ContentItem {
            content_type,
            name: name.into(),
        }
    }

    /// Path relative to the package clone root (e.g. `commands/review.md`).
    pub fn relative_path(&self) -> PathBuf {
        match self.content_type {
            ContentType::Command | ContentType::Agent => {
                PathBuf::from(self.content_type.dir_name()).join(format!("{}.md", self.name))
            }
            ContentType::Skill => PathBuf::from(self.content_type.dir_name()).join(&self.name),
        }
    }

    /// Absolute source path given the clone root.
    pub fn source_path(&self, clone_root: &Path) -> PathBuf {
        clone_root.join(self.relative_path())
    }

    /// The filename or directory name to use at the destination (no extension for skills).
    pub fn dest_name(&self) -> String {
        match self.content_type {
            ContentType::Command | ContentType::Agent => format!("{}.md", self.name),
            ContentType::Skill => self.name.clone(),
        }
    }

    /// Absolute destination path given the target content directory.
    pub fn dest_path(&self, content_dir: &Path) -> PathBuf {
        content_dir.join(self.dest_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_relative_path() {
        let item = ContentItem::new(ContentType::Command, "review");
        assert_eq!(item.relative_path(), PathBuf::from("commands/review.md"));
    }

    #[test]
    fn skill_relative_path() {
        let item = ContentItem::new(ContentType::Skill, "code-analysis");
        assert_eq!(item.relative_path(), PathBuf::from("skills/code-analysis"));
    }

    #[test]
    fn agent_relative_path() {
        let item = ContentItem::new(ContentType::Agent, "reviewer");
        assert_eq!(item.relative_path(), PathBuf::from("agents/reviewer.md"));
    }

    #[test]
    fn command_dest_name_has_md_extension() {
        let item = ContentItem::new(ContentType::Command, "deploy");
        assert_eq!(item.dest_name(), "deploy.md");
    }

    #[test]
    fn skill_dest_name_has_no_extension() {
        let item = ContentItem::new(ContentType::Skill, "style-check");
        assert_eq!(item.dest_name(), "style-check");
    }

    #[test]
    fn agent_dest_name_has_md_extension() {
        let item = ContentItem::new(ContentType::Agent, "reviewer");
        assert_eq!(item.dest_name(), "reviewer.md");
    }

    #[test]
    fn source_path_joins_clone_root() {
        let item = ContentItem::new(ContentType::Command, "review");
        let root = Path::new("/home/user/.jolene/repos/junebug/review-tools");
        assert_eq!(
            item.source_path(root),
            PathBuf::from(
                "/home/user/.jolene/repos/junebug/review-tools/commands/review.md"
            )
        );
    }

    #[test]
    fn dest_path_joins_content_dir() {
        let item = ContentItem::new(ContentType::Skill, "code-analysis");
        let content_dir = Path::new("/home/user/.claude/skills");
        assert_eq!(
            item.dest_path(content_dir),
            PathBuf::from("/home/user/.claude/skills/code-analysis")
        );
    }
}
