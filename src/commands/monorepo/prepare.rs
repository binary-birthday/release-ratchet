use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use semver::Version;

use crate::changelog::{generator, writer};
use crate::cli::{BumpOverride, PrepareArgs};
use crate::config::{Config, PackageConfig};
use crate::error::ExitCode;
use crate::git::{branch, commits, repo, tags};
use crate::git::tags::TagFilter;
use crate::semver_bump::{self, BumpLevel, apply_bump, base_version, compute_prerelease_version};
use crate::version::bumper;
use crate::commands::prepare::check_dirty_files;
use super::{resolve_packages, path_prefixes_for_package};

struct PackageRelease<'a> {
    package: &'a PackageConfig,
    last_version: Version,
    next_version: Version,
    bump: BumpLevel,
    section: String,
    modified_files: Vec<PathBuf>,
}

pub fn execute(repo_path: &Path, config: &Config, args: PrepareArgs, package_filter: Option<&str>) -> Result<()> {
    // Check if HEAD is already a release commit
    let repository_check = repo::open(repo_path).context("failed to open repository")?;
    if let Ok(head) = repository_check.head() {
        if let Ok(commit) = head.peel_to_commit() {
            let msg = commit.message().unwrap_or("");
            if msg.starts_with("chore: release ") {
                anyhow::bail!(
                    "HEAD is already a release commit ({}).\n\
                     Run `release-ratchet release` to tag it, or make new commits first.",
                    msg.trim()
                );
            }
        }
    }
    drop(repository_check);

    // Override flags require --package in monorepo
    if package_filter.is_none() && (args.bump.is_some() || args.release_version.is_some() || args.prerelease.is_some()) {
        anyhow::bail!("--bump, --release-version, and --prerelease require --package in monorepo mode");
    }

    // Validate prerelease ID
    if let Some(ref id) = args.prerelease {
        if id.is_empty()
            || id.starts_with('.')
            || id.ends_with('.')
            || id.contains("..")
            || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
        {
            anyhow::bail!("invalid pre-release identifier '{id}'");
        }
    }

    let repository = repo::open(repo_path).context("failed to open repository")?;
    let packages = resolve_packages(config, package_filter)?;
    let remote_url = crate::git::remote::get_remote_url(&repository);

    let mut releases: Vec<PackageRelease> = Vec::new();

    for pkg in &packages {
        let latest = tags::find_latest_tag(&repository, &pkg.tag_prefix, TagFilter::StableOnly)
            .context(format!("failed to search tags for '{}'", pkg.name))?;

        let (last_version, since_oid) = match &latest {
            Some(tag) => (tag.version.clone(), Some(tag.oid)),
            None => (Version::new(0, 0, 0), None),
        };

        let prefixes = path_prefixes_for_package(pkg, &config.shared_paths);
        let collection = commits::collect_since_tag_filtered(&repository, since_oid, &prefixes)
            .context(format!("failed to collect commits for '{}'", pkg.name))?;

        if collection.conventional.is_empty() && args.bump.is_none() && args.release_version.is_none() {
            log::info!("no conventional commits for package '{}', skipping", pkg.name);
            continue;
        }

        let bump = if let Some(ref v) = args.release_version {
            let next = Version::parse(v).context(format!("invalid version: {v}"))?;
            releases.push(PackageRelease {
                package: pkg,
                last_version: last_version.clone(),
                next_version: next,
                bump: BumpLevel::None,
                section: String::new(),
                modified_files: Vec::new(),
            });
            continue;
        } else if let Some(ref b) = args.bump {
            match b {
                BumpOverride::Major => BumpLevel::Major,
                BumpOverride::Minor => BumpLevel::Minor,
                BumpOverride::Patch => BumpLevel::Patch,
            }
        } else {
            semver_bump::determine_bump(&collection.conventional, config)
        };

        if bump == BumpLevel::None && args.prerelease.is_none() {
            // Check for stable promotion from pre-release
            let any_tag = tags::find_latest_tag(&repository, &pkg.tag_prefix, TagFilter::Any)?;
            if let Some(ref tag) = any_tag {
                if !tag.version.pre.is_empty() {
                    let next = base_version(&tag.version);
                    let section = generator::generate_section(&next, &collection.conventional, config, remote_url.as_deref());
                    eprintln!("[{}] {} -> {} (stable promotion)", pkg.name, tag.version, next);
                    releases.push(PackageRelease {
                        package: pkg,
                        last_version: tag.version.clone(),
                        next_version: next,
                        bump: BumpLevel::None,
                        section,
                        modified_files: Vec::new(),
                    });
                    continue;
                }
            }
            log::info!("no releasable commits for package '{}', skipping", pkg.name);
            continue;
        }

        let next_version = if let Some(ref prerelease_id) = args.prerelease {
            if bump == BumpLevel::None {
                log::info!("no releasable commits for package '{}', skipping", pkg.name);
                continue;
            }
            let tentative_base = apply_bump(&last_version, bump);
            let latest_pre = tags::find_latest_tag(
                &repository, &pkg.tag_prefix,
                TagFilter::PrereleasesOf(tentative_base.clone()),
            )?;
            let pre_version = latest_pre.as_ref().map(|t| &t.version);
            compute_prerelease_version(&last_version, pre_version, bump, prerelease_id)
        } else {
            // Check for stable promotion: if a pre-release exists, promote it
            let any_tag = tags::find_latest_tag(&repository, &pkg.tag_prefix, TagFilter::Any)?;
            if let Some(ref tag) = any_tag {
                if !tag.version.pre.is_empty() {
                    base_version(&tag.version)
                } else {
                    apply_bump(&last_version, bump)
                }
            } else {
                apply_bump(&last_version, bump)
            }
        };

        let section = generator::generate_section(&next_version, &collection.conventional, config, remote_url.as_deref());

        releases.push(PackageRelease {
            package: pkg,
            last_version,
            next_version,
            bump,
            section,
            modified_files: Vec::new(),
        });
    }

    if releases.is_empty() {
        eprintln!("No releasable changes found in any package.");
        return Err(ExitCode::NothingToRelease.into());
    }

    // Generate sections for version-override releases that skipped section generation
    for rel in &mut releases {
        if rel.section.is_empty() {
            // Re-collect commits for changelog generation
            let latest = tags::find_latest_tag(&repository, &rel.package.tag_prefix, TagFilter::StableOnly)?;
            let since_oid = latest.map(|t| t.oid);
            let prefixes = path_prefixes_for_package(rel.package, &config.shared_paths);
            let collection = commits::collect_since_tag_filtered(&repository, since_oid, &prefixes)?;
            rel.section = generator::generate_section(&rel.next_version, &collection.conventional, config, remote_url.as_deref());
        }
    }

    // Collect all files that will be modified for dirty-check
    let mut files_to_modify: Vec<PathBuf> = Vec::new();
    for rel in &releases {
        files_to_modify.push(rel.package.resolved_changelog_path());
        for eco in &rel.package.ecosystems {
            let eco_impl = bumper::create_ecosystem(eco)?;
            files_to_modify.extend(eco_impl.modified_files());
        }
    }

    if args.dry_run {
        eprintln!("--- DRY RUN ---\n");
        for rel in &releases {
            eprintln!("[{}] {} -> {} ({})", rel.package.name, rel.last_version, rel.next_version, rel.bump);
            println!("{}", rel.section);
        }
        return Ok(());
    }

    // Check for dirty files
    let existing: Vec<PathBuf> = files_to_modify.iter().filter(|f| repo_path.join(f).exists()).cloned().collect();
    check_dirty_files(&repository, &existing)?;

    // Create release branch with rollback on failure
    let branch_name = args.branch.as_deref().unwrap_or(&config.release_branch);
    let original_head = if !args.no_branch {
        let head = repository.head().context("failed to read HEAD")?;
        let refname = head.name().map(String::from);
        branch::create_and_checkout(&repository, branch_name)
            .context(format!("failed to create branch '{branch_name}'"))?;
        refname
    } else {
        None
    };

    let result = apply_monorepo_changes(
        repo_path, config, &repository, &mut releases, &remote_url, branch_name, args.no_branch,
    );

    if let Err(ref e) = result {
        if let Some(ref refname) = original_head {
            log::warn!("prepare failed, restoring original branch: {e:#}");
            if let Err(re) = restore_head(&repository, refname) {
                log::error!("failed to restore original branch: {re}");
            }
        }
    }

    if result.is_ok() && !config.hooks.post_prepare.is_empty() {
        let release_tags: Vec<String> = releases.iter()
            .map(|r| format!("{}{}", r.package.tag_prefix, r.next_version))
            .collect();
        crate::hooks::run_hooks(&config.hooks.post_prepare, repo_path, &release_tags.join(","));
    }

    result
}

fn apply_monorepo_changes(
    repo_path: &Path,
    config: &Config,
    repository: &git2::Repository,
    releases: &mut Vec<PackageRelease>,
    remote_url: &Option<String>,
    branch_name: &str,
    no_branch: bool,
) -> Result<()> {
    let mut all_modified = Vec::new();
    for rel in releases.iter_mut() {
        let changelog_path = repo_path.join(rel.package.resolved_changelog_path());
        let updated = writer::update_changelog(&changelog_path, &rel.section, remote_url.as_deref(), &rel.package.tag_prefix)
            .context(format!("failed to update changelog for '{}'", rel.package.name))?;
        writer::write_changelog(&changelog_path, &updated)
            .context(format!("failed to write changelog for '{}'", rel.package.name))?;
        all_modified.push(rel.package.resolved_changelog_path());

        let modified = bumper::bump_all(repo_path, &rel.package.ecosystems, &rel.next_version)
            .context(format!("failed to bump versions for '{}'", rel.package.name))?;
        all_modified.extend(modified.clone());
        rel.modified_files = modified;
    }

    let mut index = repository.index()?;
    for f in &all_modified {
        if repo_path.join(f).exists() {
            index.add_path(f)?;
        }
    }
    index.write()?;

    let tree_oid = index.write_tree()?;
    let tree = repository.find_tree(tree_oid)?;
    let head = repository.head()?.peel_to_commit()?;
    let sig = repository.signature().context("git user.name and user.email must be configured")?;

    let release_tags: Vec<String> = releases.iter()
        .map(|r| format!("{}{}", r.package.tag_prefix, r.next_version))
        .collect();
    let message = format!("chore: release {}", release_tags.join(", "));

    repository.commit(Some("HEAD"), &sig, &sig, &message, &tree, &[&head])?;

    eprintln!("Created release commit: {message}");
    if !no_branch {
        eprintln!("Release branch '{}' is ready. Create a PR/MR to merge it into '{}'.", branch_name, config.main_branch);
    }
    eprintln!("After merging, run `release-ratchet release` to tag the release.");
    Ok(())
}

fn restore_head(repo: &git2::Repository, refname: &str) -> Result<(), git2::Error> {
    let obj = repo.revparse_single(refname)?;
    repo.checkout_tree(&obj, Some(git2::build::CheckoutBuilder::new().force()))?;
    repo.set_head(refname)?;
    Ok(())
}
