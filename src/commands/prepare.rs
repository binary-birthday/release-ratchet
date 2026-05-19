use std::path::Path;

use anyhow::{Context, Result};
use semver::Version;

use crate::changelog::{generator, writer};
use crate::cli::{BumpOverride, PrepareArgs};
use crate::config::Config;
use crate::error::ExitCode;
use crate::git::{branch, commits, repo, tags};
use crate::git::tags::TagFilter;
use crate::semver_bump::{self, BumpLevel, apply_bump, base_version, compute_prerelease_version};
use crate::version::bumper;

pub fn execute(repo_path: &Path, config: &Config, args: PrepareArgs) -> Result<()> {
    let repository = repo::open(repo_path).context("failed to open repository")?;

    // 1. Find latest release tag
    let latest = tags::find_latest_release_tag(&repository, &config.tag_prefix)
        .context("failed to search for release tags")?;

    let (last_version, since_oid) = match &latest {
        Some(tag) => {
            log::info!("found latest release: {} ({})", tag.name, tag.version);
            (tag.version.clone(), Some(tag.oid))
        }
        None => {
            log::info!("no release tags found, starting from 0.0.0");
            (Version::new(0, 0, 0), None)
        }
    };

    // 2. Collect commits
    let collection = commits::collect_since_tag(&repository, since_oid)
        .context("failed to collect commits")?;

    if collection.non_conventional_count > 0 {
        log::warn!(
            "{} non-conventional commit(s) skipped",
            collection.non_conventional_count
        );
    }

    if collection.conventional.is_empty() && args.bump.is_none() && args.release_version.is_none() {
        eprintln!("No conventional commits found since last release.");
        return Err(ExitCode::NothingToRelease.into());
    }

    // 3. Determine bump level
    let bump = if let Some(override_version) = &args.release_version {
        let next = Version::parse(override_version)
            .context(format!("invalid version override: {override_version}"))?;
        eprintln!("{last_version} -> {next} (manual override)");
        return execute_with_version(
            repo_path,
            config,
            &args,
            &repository,
            &collection.conventional,
            &next,
        );
    } else if let Some(bump_override) = &args.bump {
        match bump_override {
            BumpOverride::Major => BumpLevel::Major,
            BumpOverride::Minor => BumpLevel::Minor,
            BumpOverride::Patch => BumpLevel::Patch,
        }
    } else {
        semver_bump::determine_bump(&collection.conventional, config)
    };

    if bump == BumpLevel::None && args.prerelease.is_none() {
        // Check for stable promotion: is the latest reachable tag a pre-release?
        let any_tag = tags::find_latest_tag(&repository, &config.tag_prefix, TagFilter::Any)
            .context("failed to search tags")?;
        if let Some(ref tag) = any_tag {
            if !tag.version.pre.is_empty() {
                // Promote pre-release to stable
                let next_version = base_version(&tag.version);
                eprintln!("{} -> {next_version} (stable promotion)", tag.version);
                return execute_with_version(
                    repo_path, config, &args, &repository,
                    &collection.conventional, &next_version,
                );
            }
        }
        eprintln!("No releasable commits found (only non-bumping types like chore, docs, etc.).");
        return Err(ExitCode::NothingToRelease.into());
    }

    // Validate pre-release identifier early
    if let Some(ref id) = args.prerelease {
        if id.is_empty()
            || id.starts_with('.')
            || id.ends_with('.')
            || id.contains("..")
            || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
        {
            anyhow::bail!("invalid pre-release identifier '{id}': must be non-empty alphanumeric segments separated by dots or hyphens");
        }
    }

    // 4. Compute next version
    let next_version = if let Some(ref prerelease_id) = args.prerelease {
        if bump == BumpLevel::None {
            eprintln!("No releasable commits found (only non-bumping types like chore, docs, etc.).");
            return Err(ExitCode::NothingToRelease.into());
        }
        let tentative_base = apply_bump(&last_version, bump);
        let latest_pre = tags::find_latest_tag(
            &repository, &config.tag_prefix,
            TagFilter::PrereleasesOf(tentative_base.clone()),
        ).context("failed to search pre-release tags")?;
        let pre_version = latest_pre.as_ref().map(|t| &t.version);
        let next = compute_prerelease_version(&last_version, pre_version, bump, prerelease_id);
        eprintln!("{last_version} -> {next} (pre-release)");
        next
    } else {
        // Check for stable promotion from pre-release
        let any_tag = tags::find_latest_tag(&repository, &config.tag_prefix, TagFilter::Any)
            .context("failed to search tags")?;
        if let Some(ref tag) = any_tag {
            if !tag.version.pre.is_empty() {
                let next = base_version(&tag.version);
                eprintln!("{} -> {next} (stable promotion)", tag.version);
                next
            } else {
                let next = apply_bump(&last_version, bump);
                eprintln!("{last_version} -> {next} ({bump})");
                next
            }
        } else {
            let next = apply_bump(&last_version, bump);
            eprintln!("{last_version} -> {next} ({bump})");
            next
        }
    };

    execute_with_version(
        repo_path,
        config,
        &args,
        &repository,
        &collection.conventional,
        &next_version,
    )
}

fn execute_with_version(
    repo_path: &Path,
    config: &Config,
    args: &PrepareArgs,
    repository: &git2::Repository,
    conventional_commits: &[crate::conventional::types::ConventionalCommit],
    next_version: &Version,
) -> Result<()> {
    // 5. Generate changelog section
    let remote_url = crate::git::remote::get_remote_url(repository);
    let section = generator::generate_section(next_version, conventional_commits, config, remote_url.as_deref());

    // Collect all files that will be modified (filter to files that exist on disk)
    let mut files_to_modify = vec![config.changelog_path.clone()];
    for eco in &config.ecosystems {
        let eco_impl = bumper::create_ecosystem(eco)?;
        files_to_modify.extend(eco_impl.modified_files());
    }
    let existing_files: Vec<_> = files_to_modify
        .iter()
        .filter(|f| repo_path.join(f).exists())
        .cloned()
        .collect();

    if args.dry_run {
        eprintln!("--- DRY RUN ---\n");
        println!("{section}");
        eprintln!("Files that would be modified:");
        for f in &files_to_modify {
            eprintln!("  - {}", f.display());
        }
        return Ok(());
    }

    // Check for uncommitted changes to files we're about to overwrite
    check_dirty_files(repository, &existing_files)?;

    // 6. Create release branch FIRST (before modifying files), unless --no-branch.
    //    Save original ref so we can restore on failure.
    let branch_name = args
        .branch
        .as_deref()
        .unwrap_or(&config.release_branch);

    let original_head = if !args.no_branch {
        let head = repository.head().context("failed to read HEAD")?;
        let refname = head.name().map(String::from);
        branch::create_and_checkout(repository, branch_name)
            .context(format!("failed to create branch '{branch_name}'"))?;
        refname
    } else {
        None
    };

    // Run the rest in a closure so we can catch errors and restore the branch
    let result = apply_release_changes(
        repo_path,
        config,
        repository,
        &section,
        next_version,
        branch_name,
        args.no_branch,
    );

    if let Err(ref e) = result {
        if let Some(ref refname) = original_head {
            log::warn!("prepare failed, restoring original branch: {e:#}");
            if let Err(restore_err) = restore_head(repository, refname) {
                log::error!("failed to restore original branch: {restore_err}");
            }
        }
    }

    if result.is_ok() && !args.dry_run && !config.hooks.post_prepare.is_empty() {
        crate::hooks::run_hooks(&config.hooks.post_prepare, repo_path, &next_version.to_string());
    }

    result
}

fn apply_release_changes(
    repo_path: &Path,
    config: &Config,
    repository: &git2::Repository,
    section: &str,
    next_version: &Version,
    branch_name: &str,
    no_branch: bool,
) -> Result<()> {
    // 7. Update CHANGELOG.md (now on the release branch)
    let remote_url = crate::git::remote::get_remote_url(repository);
    let changelog_full_path = repo_path.join(&config.changelog_path);
    let updated_changelog = writer::update_changelog(
        &changelog_full_path, section, remote_url.as_deref(), &config.tag_prefix,
    ).context("failed to update changelog")?;
    writer::write_changelog(&changelog_full_path, &updated_changelog)
        .context("failed to write changelog")?;

    // 8. Bump version in ecosystem files
    let modified_files = bumper::bump_all(repo_path, &config.ecosystems, next_version)
        .context("failed to bump version files")?;

    // 9. Stage and commit (skip files that don't exist, e.g. Cargo.lock in lib crates)
    let mut index = repository.index()?;
    index.add_path(&config.changelog_path)?;
    for f in &modified_files {
        if repo_path.join(f).exists() {
            index.add_path(f)?;
        }
    }
    index.write()?;

    let tree_oid = index.write_tree()?;
    let tree = repository.find_tree(tree_oid)?;
    let head = repository.head()?.peel_to_commit()?;
    let sig = repository.signature().context(
        "git user.name and user.email must be configured (set via git config)"
    )?;
    let tag_name = format!("{}{next_version}", config.tag_prefix);
    let message = format!("chore: release {tag_name}");

    repository.commit(Some("HEAD"), &sig, &sig, &message, &tree, &[&head])?;

    eprintln!("Created release commit: {message}");
    if !no_branch {
        eprintln!(
            "Release branch '{}' is ready. Create a PR/MR to merge it into '{}'.",
            branch_name, config.main_branch
        );
    }
    eprintln!(
        "After merging, run `release-ratchet release` to tag the release."
    );

    Ok(())
}

fn restore_head(repo: &git2::Repository, refname: &str) -> Result<(), git2::Error> {
    let obj = repo.revparse_single(refname)?;
    repo.checkout_tree(&obj, Some(git2::build::CheckoutBuilder::new().force()))?;
    repo.set_head(refname)?;
    Ok(())
}

fn check_dirty_files(
    repo: &git2::Repository,
    files: &[std::path::PathBuf],
) -> Result<()> {
    let statuses = repo.statuses(None)?;
    let mut dirty = Vec::new();

    for entry in statuses.iter() {
        let status = entry.status();
        // Only flag files with unstaged working-tree modifications.
        // Staged (index) changes are fine — they'll be included in the commit.
        // New untracked files are fine — prepare creates new files (CHANGELOG.md).
        if status.is_wt_modified() {
            if let Some(path) = entry.path() {
                let entry_path = std::path::Path::new(path);
                if files.iter().any(|f| f == entry_path) {
                    dirty.push(path.to_string());
                }
            }
        }
    }

    if !dirty.is_empty() {
        anyhow::bail!(
            "refusing to overwrite uncommitted changes in: {}\n\
             Commit or stash your changes first.",
            dirty.join(", ")
        );
    }

    Ok(())
}
