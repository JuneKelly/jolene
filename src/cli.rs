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

A package is a git repo with a jolene.toml manifest containing
commands/, skills/, and/or agents/ that get symlinked into your
coding tool's config directory.

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
}
