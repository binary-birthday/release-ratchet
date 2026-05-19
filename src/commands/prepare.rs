use std::path::Path;

use anyhow::{Context, Result};
use semver::Version;

use crate::changelog::{generator, writer};
use crate::cli::{BumpOverride, PrepareArgs};
use crate::config::Config;
use crate::error::ExitCode;
use crate::git::{branch, commits, repo, tags};
use crate::semver_bump::{self, BumpLevel, apply_bump};
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

    if bump == BumpLevel::None {
        eprintln!("No releasable commits found (only non-bumping types like chore, docs, etc.).");
        return Err(ExitCode::NothingToRelease.into());
    }

    // 4. Compute next version
    let next_version = apply_bump(&last_version, bump);
    eprintln!("{last_version} -> {next_version} ({bump})");

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
    let section = generator::generate_section(next_version, conventional_commits, config);

    if args.dry_run {
        eprintln!("--- DRY RUN ---\n");
        println!("{section}");
        eprintln!("Files that would be modified:");
        eprintln!("  - {}", config.changelog_path.display());
        for eco in &config.ecosystems {
            let eco_impl = bumper::create_ecosystem(eco)?;
            for f in eco_impl.modified_files() {
                eprintln!("  - {}", f.display());
            }
        }
        return Ok(());
    }

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
        // Attempt to restore the original branch on failure
        if let Some(ref refname) = original_head {
            log::warn!("prepare failed, restoring original branch: {e:#}");
            if let Err(restore_err) = restore_head(repository, refname) {
                log::error!("failed to restore original branch: {restore_err}");
            }
        }
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
    let changelog_full_path = repo_path.join(&config.changelog_path);
    let updated_changelog = writer::update_changelog(&changelog_full_path, section)
        .context("failed to update changelog")?;
    writer::write_changelog(&changelog_full_path, &updated_changelog)
        .context("failed to write changelog")?;

    // 8. Bump version in ecosystem files
    let modified_files = bumper::bump_all(repo_path, &config.ecosystems, next_version)
        .context("failed to bump version files")?;

    // 9. Stage and commit
    let mut index = repository.index()?;
    index.add_path(&config.changelog_path)?;
    for f in &modified_files {
        index.add_path(f)?;
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
