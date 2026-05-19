use std::path::Path;

use anyhow::{Context, Result};

use crate::changelog::{generator, reader};
use crate::cli::NotesArgs;
use crate::config::Config;
use crate::error::ExitCode;
use crate::git::{commits, repo, tags};
use crate::semver_bump::{self, apply_bump};

pub fn execute(repo_path: &Path, config: &Config, args: NotesArgs) -> Result<()> {
    if args.latest {
        return extract_latest(repo_path, config);
    }
    if let Some(ref version) = args.target_version {
        return extract_version(repo_path, config, version);
    }
    generate_next(repo_path, config)
}

fn extract_latest(repo_path: &Path, config: &Config) -> Result<()> {
    let changelog_path = repo_path.join(&config.changelog_path);
    let content = std::fs::read_to_string(&changelog_path)
        .context(format!("failed to read {}", changelog_path.display()))?;
    match reader::extract_latest_section(&content) {
        Some(section) => {
            println!("{section}");
            Ok(())
        }
        None => anyhow::bail!("no version sections found in {}", config.changelog_path.display()),
    }
}

fn extract_version(repo_path: &Path, config: &Config, version: &str) -> Result<()> {
    let changelog_path = repo_path.join(&config.changelog_path);
    let content = std::fs::read_to_string(&changelog_path)
        .context(format!("failed to read {}", changelog_path.display()))?;

    // Strip tag prefix if present (e.g., "v0.1.0" → "0.1.0")
    let version_str = version
        .strip_prefix(&config.tag_prefix)
        .unwrap_or(version);

    match reader::extract_section(&content, version_str) {
        Some(section) => {
            println!("{section}");
            Ok(())
        }
        None => anyhow::bail!("version {version_str} not found in {}", config.changelog_path.display()),
    }
}

fn generate_next(repo_path: &Path, config: &Config) -> Result<()> {
    let repository = repo::open(repo_path).context("failed to open repository")?;

    let latest = tags::find_latest_release_tag(&repository, &config.tag_prefix)
        .context("failed to search for release tags")?;

    let (last_version, since_oid) = match &latest {
        Some(tag) => (tag.version.clone(), Some(tag.oid)),
        None => (semver::Version::new(0, 0, 0), None),
    };

    let collection = commits::collect_since_tag(&repository, since_oid)
        .context("failed to collect commits")?;

    if collection.conventional.is_empty() {
        return Err(ExitCode::NothingToRelease.into());
    }

    let bump = semver_bump::determine_bump(&collection.conventional, config);
    if bump == semver_bump::BumpLevel::None {
        return Err(ExitCode::NothingToRelease.into());
    }

    let next_version = apply_bump(&last_version, bump);
    let section = generator::generate_section(&next_version, &collection.conventional, config);
    print!("{section}");
    Ok(())
}
