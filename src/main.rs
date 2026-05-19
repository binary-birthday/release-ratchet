mod changelog;
mod cli;
mod commands;
mod config;
mod conventional;
mod error;
mod git;
mod hooks;
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
        Commands::Completions(args) => {
            commands::completions::execute(args);
            Ok(())
        }
        Commands::Hook(args) => commands::hook::execute(&repo_path, args.action),
        Commands::Prepare(args) => {
            let config = config::load_config(&repo_path, cli.config.as_deref())?;
            if config.is_monorepo() {
                commands::monorepo::prepare::execute(&repo_path, &config, args, cli.package.as_deref())
            } else {
                commands::prepare::execute(&repo_path, &config, args)
            }
        }
        Commands::Release(args) => {
            let config = config::load_config(&repo_path, cli.config.as_deref())?;
            if config.is_monorepo() {
                commands::monorepo::release::execute(&repo_path, &config, args, cli.package.as_deref())
            } else {
                commands::release::execute(&repo_path, &config, args)
            }
        }
        Commands::Status(args) => {
            let config = config::load_config(&repo_path, cli.config.as_deref())?;
            if config.is_monorepo() {
                commands::monorepo::status::execute(&repo_path, &config, args, cli.package.as_deref())
            } else {
                commands::status::execute(&repo_path, &config, args)
            }
        }
        Commands::Validate(args) => {
            let config = config::load_config(&repo_path, cli.config.as_deref())?;
            commands::validate::execute(&repo_path, &config, args)
        }
        Commands::Notes(args) => {
            let config = config::load_config(&repo_path, cli.config.as_deref())?;
            if config.is_monorepo() {
                commands::monorepo::notes::execute(&repo_path, &config, args, cli.package.as_deref())
            } else {
                commands::notes::execute(&repo_path, &config, args)
            }
        }
        Commands::Backport(args) => {
            let config = config::load_config(&repo_path, cli.config.as_deref())?;
            commands::backport::execute(&repo_path, &config, args)
        }
        Commands::Bump(args) => {
            let config = config::load_config(&repo_path, cli.config.as_deref())?;
            if config.is_monorepo() {
                commands::monorepo::bump::execute(&repo_path, &config, args, cli.package.as_deref())
            } else {
                commands::bump::execute(&repo_path, &config, args)
            }
        }
        Commands::Check(args) => {
            let config = config::load_config(&repo_path, cli.config.as_deref())?;
            if config.is_monorepo() {
                commands::monorepo::check::execute(&repo_path, &config, args, cli.package.as_deref())
            } else {
                commands::check::execute(&repo_path, &config, args)
            }
        }
    }
}
