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
use error::ExitCode;

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
        // Check if the error is a typed ExitCode (non-error exit)
        if let Some(exit_code) = err.downcast_ref::<ExitCode>() {
            eprintln!("{exit_code}");
            std::process::exit(exit_code.code());
        }
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    let repo_path = cli.repo.canonicalize()?;

    match cli.command {
        Commands::Init(args) => commands::init::execute(&repo_path, args),
        Commands::Prepare(args) => {
            let config = config::load_config(&repo_path, cli.config.as_deref())?;
            commands::prepare::execute(&repo_path, &config, args)
        }
        Commands::Release(args) => {
            let config = config::load_config(&repo_path, cli.config.as_deref())?;
            commands::release::execute(&repo_path, &config, args)
        }
        Commands::Status(args) => {
            let config = config::load_config(&repo_path, cli.config.as_deref())?;
            commands::status::execute(&repo_path, &config, args)
        }
        Commands::Validate(args) => {
            let config = config::load_config(&repo_path, cli.config.as_deref())?;
            commands::validate::execute(&repo_path, &config, args)
        }
        Commands::Backport(args) => {
            let config = config::load_config(&repo_path, cli.config.as_deref())?;
            commands::backport::execute(&repo_path, &config, args)
        }
    }
}
