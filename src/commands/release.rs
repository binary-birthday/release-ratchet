use std::path::Path;

use anyhow::{Context, Result};
use regex::Regex;
use semver::Version;

use crate::cli::ReleaseArgs;
use crate::config::Config;
use crate::git::{repo, tags};

pub fn execute(repo_path: &Path, config: &Config, args: ReleaseArgs) -> Result<()> {
    let repository = repo::open(repo_path).context("failed to open repository")?;

    // 1. Determine target commit (HEAD of current branch, not necessarily main)
    let target_oid = if let Some(ref commitish) = args.commit {
        repo::resolve_ref(&repository, commitish)
            .context(format!("failed to resolve '{commitish}'"))?
    } else {
        repo::resolve_ref(&repository, "HEAD")
            .context("failed to resolve HEAD")?
    };

    let oid_hex = target_oid.to_string();
    let short_oid = oid_hex.get(..7).unwrap_or(&oid_hex);

    // 2. Determine version to tag
    let version = if let Some(ref v) = args.release_version {
        Version::parse(v).context(format!("invalid version: {v}"))?
    } else {
        detect_version_from_commit(&repository, target_oid, config)?
    };

    let tag_name = format!("{}{version}", config.tag_prefix);

    // 3. Validate tag doesn't exist
    if args.dry_run {
        eprintln!("--- DRY RUN ---");
        eprintln!("Would create tag '{tag_name}' at commit {short_oid}");
        if args.cleanup || config.cleanup_branch {
            eprintln!("Would delete branch '{}'", config.release_branch);
        }
        if !config.hooks.post_release.is_empty() {
            eprintln!("Would run {} post-release hook(s)", config.hooks.post_release.len());
        }
        return Ok(());
    }

    // 4. Create the tag
    tags::create_tag(&repository, &tag_name, target_oid, config.sign_tags)
        .context(format!("failed to create tag '{tag_name}'"))?;

    eprintln!("Created tag '{tag_name}' at {short_oid}");
    eprintln!("Run `git push origin {tag_name}` to publish.");

    // Branch cleanup (explicit dry-run guard for safety under refactoring)
    if !args.dry_run && (args.cleanup || config.cleanup_branch) {
        // Check if we're on the release branch — can't delete the checked-out branch
        let on_release_branch = repository.head()
            .ok()
            .and_then(|h| h.shorthand().map(|s| s == config.release_branch))
            .unwrap_or(false);
        if on_release_branch {
            eprintln!(
                "warning: cannot delete '{}' — it is currently checked out. Switch to '{}' first.",
                config.release_branch, config.main_branch
            );
        } else if let Err(e) = crate::git::branch::delete_branch(&repository, &config.release_branch) {
            log::warn!("failed to delete release branch: {e}");
        } else {
            eprintln!("Deleted branch '{}'", config.release_branch);
        }
    }

    // Post-release hooks (explicit dry-run guard)
    if !args.dry_run && !config.hooks.post_release.is_empty() {
        crate::hooks::run_hooks(&config.hooks.post_release, repo_path, &version.to_string());
    }

    Ok(())
}

fn detect_version_from_commit(
    repo: &git2::Repository,
    oid: git2::Oid,
    config: &Config,
) -> Result<Version> {
    let commit = repo.find_commit(oid)?;
    let message = commit.message().unwrap_or("");

    // Try to extract from "chore: release vX.Y.Z" or "chore: release X.Y.Z"
    let release_re = Regex::new(&format!(
        r"chore:\s+release\s+(?:{})?(\d+\.\d+\.\d+(?:-[\w.]+)?)",
        regex::escape(&config.tag_prefix)
    ))
    .unwrap();

    if let Some(caps) = release_re.captures(message) {
        let version_str = caps.get(1).unwrap().as_str();
        return Version::parse(version_str)
            .context(format!("invalid version in commit message: {version_str}"));
    }

    // If it's a merge commit, check the merged branch's commits
    if commit.parent_count() > 1 {
        if let Ok(parent) = commit.parent(1) {
            let parent_msg = parent.message().unwrap_or("");
            if let Some(caps) = release_re.captures(parent_msg) {
                let version_str = caps.get(1).unwrap().as_str();
                return Version::parse(version_str)
                    .context(format!("invalid version in merged commit: {version_str}"));
            }
        }
    }

    // Fall back to reading the CHANGELOG.md from the commit's tree.
    // This handles squash merges where the commit message is the PR title.
    if let Ok(version) = detect_version_from_changelog(repo, &commit, config) {
        return Ok(version);
    }

    anyhow::bail!(
        "Could not detect version from commit {}. \
         Use --release-version to specify it explicitly.",
        crate::git::repo::short_oid(oid),
    )
}

fn detect_version_from_changelog(
    repo: &git2::Repository,
    commit: &git2::Commit,
    config: &Config,
) -> Result<Version> {
    let tree = commit.tree()?;
    let changelog_path = config.changelog_path.to_str().unwrap_or("CHANGELOG.md");
    let entry = tree.get_path(Path::new(changelog_path))
        .context("CHANGELOG.md not found in commit tree")?;
    let blob = repo.find_blob(entry.id())
        .context("failed to read CHANGELOG.md blob")?;
    let content = std::str::from_utf8(blob.content())
        .context("CHANGELOG.md is not valid UTF-8")?;

    // Parse the first "## [X.Y.Z]" heading
    let version_re = Regex::new(r"## \[(\d+\.\d+\.\d+(?:-[\w.]+)?)\]").unwrap();
    let caps = version_re.captures(content)
        .context("no version heading found in CHANGELOG.md")?;
    let version_str = caps.get(1).unwrap().as_str();
    Version::parse(version_str)
        .context(format!("invalid version in CHANGELOG.md: {version_str}"))
}
