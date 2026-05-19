use std::path::Path;

use anyhow::{Context, Result};
use semver::Version;

use crate::cli::{BumpArgs, BumpOverride};
use crate::config::Config;
use crate::error::ExitCode;
use crate::git::{commits, repo, tags};
use crate::git::tags::TagFilter;
use crate::semver_bump::{self, BumpLevel, apply_bump};
use crate::version::bumper;
use super::{resolve_packages, path_prefixes_for_package};

pub fn execute(repo_path: &Path, config: &Config, args: BumpArgs, package_filter: Option<&str>) -> Result<()> {
    if package_filter.is_none() && (args.bump.is_some() || args.release_version.is_some()) {
        anyhow::bail!("--bump and --release-version require --package in monorepo mode");
    }

    let repository = repo::open(repo_path).context("failed to open repository")?;
    let packages = resolve_packages(config, package_filter)?;
    let mut any = false;

    for pkg in &packages {
        let latest = tags::find_latest_tag(&repository, &pkg.tag_prefix, TagFilter::StableOnly)?;
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
                let prefixes = path_prefixes_for_package(pkg, &config.shared_paths);
                let collection = commits::collect_since_tag_filtered(&repository, since_oid, &prefixes)?;
                let level = semver_bump::determine_bump(&collection.conventional, config);
                if level == BumpLevel::None {
                    log::info!("no releasable commits for '{}', skipping", pkg.name);
                    continue;
                }
                level
            };
            apply_bump(&last_version, bump)
        };

        if args.dry_run {
            eprintln!("[{}] {} -> {}", pkg.name, last_version, next_version);
            any = true;
            continue;
        }

        let modified = bumper::bump_all(repo_path, &pkg.ecosystems, &next_version)
            .context(format!("failed to bump versions for '{}'", pkg.name))?;
        eprintln!("[{}] {} -> {}", pkg.name, last_version, next_version);
        for f in &modified {
            if repo_path.join(f).exists() {
                eprintln!("  bumped {}", f.display());
            }
        }
        any = true;
    }

    if !any {
        return Err(ExitCode::NothingToRelease.into());
    }

    Ok(())
}
