mod changelog;
mod cli;
mod commands;
mod config;
mod conventional;
mod error;
mod git;
mod semver_bump;
mod version;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Commands};

fn main() {
    let cli = Cli::parse();

    let log_level = match (cli.quiet, cli.verbose) {
        (true, _) => log::LevelFilter::Error,
        (_, 0) => log::LevelFilter::Warn,
        (_, 1) => log::LevelFilter::Info,
        (_, 2) => log::LevelFilter::Debug,
        _ => log::LevelFilter::Trace,
    };
    env_logger::Builder::new().filter_level(log_level).init();

    if let Err(err) = run(cli) {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    let repo_path = cli.repo.canonicalize()?;

    match cli.command {
        Commands::Init(args) => commands::init::execute(&repo_path, args),
        _ => {
            let config = config::load_config(&repo_path, cli.config.as_deref())?;
            match cli.command {
                Commands::Prepare(args) => commands::prepare::execute(&repo_path, &config, args),
                Commands::Release(args) => commands::release::execute(&repo_path, &config, args),
                Commands::Status(args) => commands::status::execute(&repo_path, &config, args),
                Commands::Validate(args) => commands::validate::execute(&repo_path, &config, args),
                Commands::Init(_) => unreachable!(),
            }
        }
    }
}
