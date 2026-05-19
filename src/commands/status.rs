use std::path::Path;

use anyhow::{Context, Result};

use crate::cli::StatusArgs;
use crate::config::Config;
use crate::git::{commits, repo, tags};
use crate::semver_bump::{self, apply_bump};
use crate::version::bumper;

pub fn execute(repo_path: &Path, config: &Config, args: StatusArgs) -> Result<()> {
    let repository = repo::open(repo_path).context("failed to open repository")?;

    let latest = tags::find_latest_release_tag(&repository, &config.tag_prefix)
        .context("failed to search for release tags")?;

    let (last_version, since_oid, tag_name) = match &latest {
        Some(tag) => (tag.version.clone(), Some(tag.oid), Some(tag.name.clone())),
        None => (semver::Version::new(0, 0, 0), None, None),
    };

    let collection = commits::collect_since_tag(&repository, since_oid)
        .context("failed to collect commits")?;

    let bump = semver_bump::determine_bump(&collection.conventional, config);
    let next_version = apply_bump(&last_version, bump);

    let breaking_count = collection
        .conventional
        .iter()
        .filter(|c| c.is_breaking())
        .count();

    // Try to read current version from ecosystem files
    let current_file_version = config
        .ecosystems
        .first()
        .and_then(|eco| {
            let eco_impl = bumper::create_ecosystem(eco);
            eco_impl.read_version(repo_path).ok()
        });

    if args.json {
        let json = serde_json::json!({
            "last_tag": tag_name,
            "last_version": last_version.to_string(),
            "current_file_version": current_file_version.as_ref().map(|v| v.to_string()),
            "commits_since": collection.conventional.len() + collection.non_conventional_count,
            "conventional_commits": collection.conventional.len(),
            "non_conventional_commits": collection.non_conventional_count,
            "bump_level": bump.to_string(),
            "next_version": next_version.to_string(),
            "breaking_changes": breaking_count,
        });
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else {
        match &tag_name {
            Some(name) => eprintln!("Last release:    {name} ({last_version})"),
            None => eprintln!("Last release:    (none)"),
        }
        if let Some(ref fv) = current_file_version {
            eprintln!("File version:    {fv}");
        }
        let total = collection.conventional.len() + collection.non_conventional_count;
        eprintln!(
            "Commits since:   {total} ({} conventional, {} other)",
            collection.conventional.len(),
            collection.non_conventional_count,
        );
        eprintln!("Bump level:      {bump}");
        eprintln!("Next version:    {next_version}");
        if breaking_count > 0 {
            eprintln!("Breaking changes: {breaking_count}");
        }
    }

    Ok(())
}
