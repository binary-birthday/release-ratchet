use std::path::Path;

use anyhow::{Context, Result};

use crate::cli::StatusArgs;
use crate::config::Config;
use crate::git::{commits, repo, tags};
use crate::git::tags::TagFilter;
use crate::semver_bump::{self, apply_bump};
use crate::version::bumper;
use super::{resolve_packages, path_prefixes_for_package};

pub fn execute(repo_path: &Path, config: &Config, args: StatusArgs, package_filter: Option<&str>) -> Result<()> {
    let repository = repo::open(repo_path).context("failed to open repository")?;
    let packages = resolve_packages(config, package_filter)?;

    let mut results = Vec::new();

    for pkg in &packages {
        let latest = tags::find_latest_tag(&repository, &pkg.tag_prefix, TagFilter::StableOnly)
            .context(format!("failed to search tags for package '{}'", pkg.name))?;

        let (last_version, since_oid) = match &latest {
            Some(tag) => (tag.version.clone(), Some(tag.oid)),
            None => (semver::Version::new(0, 0, 0), None),
        };

        let prefixes = path_prefixes_for_package(pkg, &config.shared_paths);
        let collection = commits::collect_since_tag_filtered(&repository, since_oid, &prefixes, config.forge.as_ref())
            .context(format!("failed to collect commits for package '{}'", pkg.name))?;

        let bump = semver_bump::determine_bump(&collection.conventional, config);
        let next_version = apply_bump(&last_version, bump);

        let breaking_count = collection.conventional.iter().filter(|c| c.is_breaking()).count();

        let file_version = pkg.ecosystems.first().and_then(|eco| {
            bumper::create_ecosystem(eco).ok().and_then(|e| e.read_version(repo_path).ok())
        });

        results.push(serde_json::json!({
            "package": pkg.name,
            "last_tag": latest.as_ref().map(|t| &t.name),
            "last_version": last_version.to_string(),
            "current_file_version": file_version.as_ref().map(|v| v.to_string()),
            "commits_since": collection.conventional.len() + collection.non_conventional_count,
            "conventional_commits": collection.conventional.len(),
            "non_conventional_commits": collection.non_conventional_count,
            "bump_level": bump.to_string(),
            "next_version": next_version.to_string(),
            "breaking_changes": breaking_count,
        }));

        if !args.json {
            let tag_display = latest.as_ref().map(|t| format!("{} ({})", t.name, t.version)).unwrap_or_else(|| "(none)".into());
            eprintln!("[{}]", pkg.name);
            eprintln!("  Last release:    {tag_display}");
            if let Some(ref fv) = file_version {
                eprintln!("  File version:    {fv}");
            }
            let total = collection.conventional.len() + collection.non_conventional_count;
            eprintln!("  Commits since:   {total} ({} conventional, {} other)", collection.conventional.len(), collection.non_conventional_count);
            eprintln!("  Bump level:      {bump}");
            eprintln!("  Next version:    {next_version}");
            if breaking_count > 0 {
                eprintln!("  Breaking changes: {breaking_count}");
            }
            eprintln!();
        }
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&results)?);
    }

    Ok(())
}
