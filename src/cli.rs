use std::path::PathBuf;

use clap::{ArgGroup, Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "jolene",
    version,
    about = "A package manager for coding agent commands, skills, and agents.",
    after_help = "\
Quick start:

  Install from GitHub:  jolene install --github owner/repo
  Install from URL:     jolene install --url https://example.com/repo.git
  Install from path:    jolene install --local /path/to/package
  List installed:       jolene list
  Update all:           jolene update
  Uninstall:            jolene uninstall owner/repo

Marketplace (Claude Code plugin repos):

  Browse plugins:       jolene contents --marketplace --github org/tools
  Install a plugin:     jolene install --marketplace --github org/tools --pick review

A package is a git repo with a jolene.toml manifest containing
commands/, skills/, and/or agents/ that get symlinked into your
coding tool's config directory. Marketplace repos use
.claude-plugin/marketplace.json instead.

Supported targets:

  claude-code   ~/.claude/
  opencode      ~/.config/opencode/
  codex         ~/.codex/

Targets are auto-detected by checking whether their config directory exists.
Use --to / --from to specify targets explicitly."
)]
pub struct Cli {
    /// Print detailed output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Suppress non-error output
    #[arg(short, long, global = true)]
    pub quiet: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Install a package
    Install(InstallArgs),

    /// Uninstall a package
    Uninstall {
        /// Package identifier: owner/repo (GitHub), absolute path (local), or URL
        package: String,

        /// Remove from specific targets only (repeatable). Defaults to all.
        #[arg(long = "from", value_name = "TARGET")]
        from: Vec<String>,

        /// Also delete the cloned repo from the local store
        #[arg(long)]
        purge: bool,
    },

    /// List installed packages
    List {
        /// Filter by target
        #[arg(long, value_name = "TARGET")]
        target: Option<String>,
    },

    /// Update one or all packages
    Update {
        /// Package to update. Omit to update all.
        package: Option<String>,
    },

    /// Show detailed info about an installed package
    Info {
        /// Package identifier: owner/repo (GitHub), absolute path (local), or URL
        package: String,
    },

    /// Browse contents of a marketplace or installed package
    Contents(ContentsArgs),

    /// Verify health of all installations
    Doctor,
}

#[derive(Debug, Args)]
#[command(
    about = "Install a package",
    group(
        ArgGroup::new("source")
            .required(true)
            .args(["github", "local", "url"])
    )
)]
pub struct InstallArgs {
    /// GitHub repository in Owner/repo format
    #[arg(long, value_name = "OWNER/REPO")]
    pub github: Option<String>,

    /// Local git repository path
    #[arg(long, value_name = "PATH")]
    pub local: Option<PathBuf>,

    /// Remote git URL
    #[arg(long, value_name = "URL")]
    pub url: Option<String>,

    /// Target(s) to install to (repeatable). Defaults to all detected targets.
    #[arg(long = "to", value_name = "TARGET")]
    pub to: Vec<String>,

    /// Treat the source as a Claude Code marketplace repository
    #[arg(long)]
    pub marketplace: bool,

    /// Select specific plugins from a marketplace catalog (comma-separated)
    #[arg(long, value_name = "NAME", value_delimiter = ',')]
    pub pick: Vec<String>,

    /// Prefix for installed content names (e.g. --prefix jb → jb--review.md)
    #[arg(long, value_name = "PREFIX")]
    pub prefix: Option<String>,

    /// Suppress any manifest-defined prefix; install flat
    #[arg(long, conflicts_with = "prefix")]
    pub no_prefix: bool,
}

#[derive(Debug, Args)]
#[command(
    about = "Browse contents of a marketplace or installed package",
    group(
        ArgGroup::new("contents_source")
            .args(["github", "local", "url", "package"])
    )
)]
pub struct ContentsArgs {
    /// GitHub repository in Owner/repo format
    #[arg(long, value_name = "OWNER/REPO")]
    pub github: Option<String>,

    /// Local git repository path
    #[arg(long, value_name = "PATH")]
    pub local: Option<PathBuf>,

    /// Remote git URL
    #[arg(long, value_name = "URL")]
    pub url: Option<String>,

    /// Name of an installed package
    #[arg(value_name = "PACKAGE")]
    pub package: Option<String>,

    /// Treat the source as a Claude Code marketplace repository
    #[arg(long)]
    pub marketplace: bool,
}
