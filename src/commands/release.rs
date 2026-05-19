use std::path::Path;

use anyhow::{Context, Result};
use regex::Regex;
use semver::Version;

use crate::cli::ReleaseArgs;
use crate::config::Config;
use crate::git::{repo, tags};

pub fn execute(repo_path: &Path, config: &Config, args: ReleaseArgs) -> Result<()> {
    let repository = repo::open(repo_path).context("failed to open repository")?;

    // 1. Determine target commit
    let target_oid = if let Some(ref commitish) = args.commit {
        repo::resolve_ref(&repository, commitish)
            .context(format!("failed to resolve '{commitish}'"))?
    } else {
        let main_ref = format!("refs/heads/{}", config.main_branch);
        repo::resolve_ref(&repository, &main_ref)
            .context(format!("failed to resolve '{main_ref}'"))?
    };

    let short_oid = &target_oid.to_string()[..7];

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
        return Ok(());
    }

    // 4. Create the tag
    tags::create_tag(&repository, &tag_name, target_oid, config.sign_tags)
        .context(format!("failed to create tag '{tag_name}'"))?;

    eprintln!("Created tag '{tag_name}' at {short_oid}");
    eprintln!("Run `git push origin {tag_name}` to publish.");

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
        r"chore:\s+release\s+{}?(\d+\.\d+\.\d+(?:-[\w.]+)?)",
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
        // Check second parent (the merged branch)
        if let Ok(parent) = commit.parent(1) {
            let parent_msg = parent.message().unwrap_or("");
            if let Some(caps) = release_re.captures(parent_msg) {
                let version_str = caps.get(1).unwrap().as_str();
                return Version::parse(version_str)
                    .context(format!("invalid version in merged commit: {version_str}"));
            }
        }
    }

    anyhow::bail!(
        "Could not detect version from commit {oid}. \
         Use --version to specify it explicitly."
    )
}
