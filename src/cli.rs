use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "jolene", version, about = "A package manager for coding agent commands, skills, and agents.")]
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
    /// Install a package from GitHub
    Install {
        /// GitHub repository in Author/repo format
        source: String,

        /// Target(s) to install to (repeatable). Defaults to all detected targets.
        #[arg(long = "to", value_name = "TARGET")]
        to: Vec<String>,
    },

    /// Uninstall a package
    Uninstall {
        /// Package name in Author/repo or repo format
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
        /// Package name in Author/repo or repo format
        package: String,
    },

    /// Verify health of all installations
    Doctor,
}
