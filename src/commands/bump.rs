use std::path::Path;

use anyhow::{Context, Result};
use semver::Version;

use crate::cli::{BumpArgs, BumpOverride};
use crate::config::Config;
use crate::error::ExitCode;
use crate::git::{commits, repo, tags};
use crate::semver_bump::{self, BumpLevel, apply_bump};
use crate::version::bumper;

pub fn execute(repo_path: &Path, config: &Config, args: BumpArgs) -> Result<()> {
    let repository = repo::open(repo_path).context("failed to open repository")?;

    let latest = tags::find_latest_release_tag(&repository, &config.tag_prefix)
        .context("failed to search for release tags")?;

    let (last_version, since_oid) = match &latest {
        Some(tag) => (tag.version.clone(), Some(tag.oid)),
        None => (Version::new(0, 0, 0), None),
    };

    let next_version = if let Some(ref v) = args.release_version {
        Version::parse(v).context(format!("invalid version: {v}"))?
    } else {
        let bump = if let Some(ref b) = args.bump {
            match b {
                BumpOverride::Major => BumpLevel::Major,
                BumpOverride::Minor => BumpLevel::Minor,
                BumpOverride::Patch => BumpLevel::Patch,
            }
        } else {
            let collection = commits::collect_since_tag(&repository, since_oid, config.forge.as_ref())
                .context("failed to collect commits")?;
            let level = semver_bump::determine_bump(&collection.conventional, config);
            if level == BumpLevel::None {
                return Err(ExitCode::NothingToRelease.into());
            }
            level
        };
        apply_bump(&last_version, bump)
    };

    if args.dry_run {
        eprintln!("{last_version} -> {next_version}");
        eprintln!("Files that would be modified:");
        for eco in &config.ecosystems {
            let eco_impl = bumper::create_ecosystem(eco)?;
            for f in eco_impl.modified_files() {
                if repo_path.join(&f).exists() {
                    eprintln!("  - {}", f.display());
                }
            }
        }
        return Ok(());
    }

    let modified = bumper::bump_all(repo_path, &config.ecosystems, &next_version)
        .context("failed to bump version files")?;

    eprintln!("{last_version} -> {next_version}");
    for f in &modified {
        if repo_path.join(f).exists() {
            eprintln!("  bumped {}", f.display());
        }
    }

    Ok(())
}
