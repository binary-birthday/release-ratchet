use std::path::Path;

use anyhow::{Context, Result};

use crate::changelog::{generator, reader};
use crate::cli::NotesArgs;
use crate::config::Config;
use crate::error::ExitCode;
use crate::git::{commits, repo, tags};
use crate::git::tags::TagFilter;
use crate::semver_bump::{self, apply_bump};
use super::{resolve_packages, path_prefixes_for_package};

pub fn execute(repo_path: &Path, config: &Config, args: NotesArgs, package_filter: Option<&str>) -> Result<()> {
    let packages = resolve_packages(config, package_filter)?;

    if args.latest {
        for pkg in &packages {
            let path = repo_path.join(pkg.resolved_changelog_path());
            let content = std::fs::read_to_string(&path)
                .context(format!("failed to read {}", path.display()))?;
            match reader::extract_latest_section(&content) {
                Some(section) => {
                    if packages.len() > 1 { println!("# {}\n", pkg.name); }
                    println!("{section}\n");
                }
                None => eprintln!("no version sections in {} for package '{}'", path.display(), pkg.name),
            }
        }
        return Ok(());
    }

    if let Some(ref version) = args.target_version {
        for pkg in &packages {
            let path = repo_path.join(pkg.resolved_changelog_path());
            let content = std::fs::read_to_string(&path)
                .context(format!("failed to read {}", path.display()))?;
            // Strip the package's tag prefix (not the global one)
            let v = version.strip_prefix(&pkg.tag_prefix).unwrap_or(version);
            match reader::extract_section(&content, v) {
                Some(section) => {
                    if packages.len() > 1 { println!("# {}\n", pkg.name); }
                    println!("{section}\n");
                }
                None => anyhow::bail!("version {v} not found in {} for package '{}'", path.display(), pkg.name),
            }
        }
        return Ok(());
    }

    // Generate next
    let repository = repo::open(repo_path).context("failed to open repository")?;
    let remote_url = crate::git::remote::get_remote_url(&repository);
    let mut any = false;

    for pkg in &packages {
        let latest = tags::find_latest_tag(&repository, &pkg.tag_prefix, TagFilter::StableOnly)?;
        let (last_version, since_oid) = match &latest {
            Some(tag) => (tag.version.clone(), Some(tag.oid)),
            None => (semver::Version::new(0, 0, 0), None),
        };

        let prefixes = path_prefixes_for_package(pkg, &config.shared_paths);
        let collection = commits::collect_since_tag_filtered(&repository, since_oid, &prefixes)?;

        if collection.conventional.is_empty() { continue; }

        let bump = semver_bump::determine_bump(&collection.conventional, config);
        if bump == semver_bump::BumpLevel::None { continue; }

        let next = apply_bump(&last_version, bump);
        let section = generator::generate_section(&next, &collection.conventional, config, remote_url.as_deref());

        if packages.len() > 1 { println!("# {}\n", pkg.name); }
        println!("{section}");
        any = true;
    }

    if !any {
        return Err(ExitCode::NothingToRelease.into());
    }

    Ok(())
}
