use std::path::Path;

use anyhow::{Context, Result};
use regex::Regex;
use semver::Version;

use crate::cli::ReleaseArgs;
use crate::config::Config;
use crate::git::{repo, tags};
use super::resolve_packages;

pub fn execute(repo_path: &Path, config: &Config, args: ReleaseArgs, package_filter: Option<&str>) -> Result<()> {
    let repository = repo::open(repo_path).context("failed to open repository")?;
    let packages = resolve_packages(config, package_filter)?;

    let target_oid = if let Some(ref commitish) = args.commit {
        repo::resolve_ref(&repository, commitish).context(format!("failed to resolve '{commitish}'"))?
    } else {
        repo::resolve_ref(&repository, "HEAD").context("failed to resolve HEAD")?
    };
    let short = repo::short_oid(target_oid);

    let commit = repository.find_commit(target_oid)?;
    let message = commit.message().unwrap_or("");

    if args.dry_run {
        eprintln!("--- DRY RUN ---");
    }

    let mut created_tags: Vec<String> = Vec::new();
    for pkg in &packages {
        let version = if let Some(ref v) = args.release_version {
            if package_filter.is_none() {
                anyhow::bail!("--release-version requires --package in monorepo mode");
            }
            Version::parse(v).context(format!("invalid version: {v}"))?
        } else {
            match detect_package_version(message, &pkg.tag_prefix, &repository, &commit, pkg) {
                Ok(v) => v,
                Err(_) => {
                    log::debug!("no version detected for package '{}', skipping", pkg.name);
                    continue;
                }
            }
        };

        let tag_name = format!("{}{version}", pkg.tag_prefix);

        if args.dry_run {
            eprintln!("Would create tag '{tag_name}' at {short}");
            continue;
        }

        tags::create_tag(&repository, &tag_name, target_oid, config.sign_tags)
            .context(format!("failed to create tag '{tag_name}'"))?;
        eprintln!("Created tag '{tag_name}' at {short}");
        created_tags.push(tag_name);
    }

    if !args.dry_run {
        if !created_tags.is_empty() {
            eprintln!("Run `git push origin --tags` to publish.");
        }

        if args.cleanup || config.cleanup_branch {
            let on_release = repository.head().ok()
                .and_then(|h| h.shorthand().map(|s| s == config.release_branch))
                .unwrap_or(false);
            if on_release {
                eprintln!("warning: cannot delete '{}' — currently checked out", config.release_branch);
            } else {
                let _ = crate::git::branch::delete_branch(&repository, &config.release_branch);
            }
        }

        if !config.hooks.post_release.is_empty() {
            let versions = created_tags.join(",");
            crate::hooks::run_hooks(&config.hooks.post_release, repo_path, &versions);
        }
    }

    Ok(())
}

fn detect_package_version(
    message: &str,
    tag_prefix: &str,
    repo: &git2::Repository,
    commit: &git2::Commit,
    pkg: &crate::config::PackageConfig,
) -> Result<Version> {
    // Try commit message: "chore: release core-v1.2.3, cli-v0.5.0"
    // Use regex with word boundary to avoid prefix substring ambiguity
    // (e.g., "core-v" matching inside "ui-core-v")
    let prefix_re = Regex::new(&format!(
        r"(?:^|[\s,]){}(\d+\.\d+\.\d+(?:-[\w.\-]+)?)",
        regex::escape(tag_prefix)
    )).unwrap();
    if let Some(caps) = prefix_re.captures(message) {
        let v = caps.get(1).unwrap().as_str();
        return Version::parse(v).context(format!("invalid version in commit: {v}"));
    }

    // Fallback: read CHANGELOG.md from commit tree
    let tree = commit.tree()?;
    let changelog_path = pkg.resolved_changelog_path();
    if let Ok(entry) = tree.get_path(&changelog_path) {
        if let Ok(blob) = repo.find_blob(entry.id()) {
            if let Ok(content) = std::str::from_utf8(blob.content()) {
                let version_re = Regex::new(r"## \[(\d+\.\d+\.\d+(?:-[\w.\-]+)?)\]").unwrap();
                if let Some(caps) = version_re.captures(content) {
                    let v = caps.get(1).unwrap().as_str();
                    return Version::parse(v).context(format!("invalid version in changelog: {v}"));
                }
            }
        }
    }

    anyhow::bail!("could not detect version for package '{}'. Use --release-version.", pkg.name)
}
