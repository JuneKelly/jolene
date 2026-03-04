mod cli;
mod commands;
mod config;
mod discovery;
mod git;
mod marketplace;
mod output;
mod skill_check;
mod state;
mod symlink;
mod types;
mod validation;

use clap::Parser;

use cli::{Cli, Command};
use output::Output;

fn main() {
    let cli = Cli::parse();
    let out = Output::new(cli.verbose, cli.quiet);

    let result = match &cli.command {
        Command::Install(args) => commands::install::run_from_args(args, &out),
        Command::Uninstall {
            package,
            from,
            purge,
        } => commands::uninstall::run(package, from, *purge, &out),
        Command::List { target } => commands::list::run(target.as_deref(), &out),
        Command::Update { package } => commands::update::run(package.as_deref(), &out),
        Command::Info { package } => commands::info::run(package, &out),
        Command::Contents(args) => commands::contents::run(args, &out),
        Command::Doctor => commands::doctor::run(&out),
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
