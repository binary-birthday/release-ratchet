use std::path::Path;

use anyhow::{Context, Result};

use crate::changelog::reader;
use crate::cli::CheckArgs;
use crate::config::Config;
use crate::error::ExitCode;
use crate::git::{repo, tags};
use crate::git::tags::TagFilter;
use crate::version::bumper;
use super::{resolve_packages};

pub fn execute(repo_path: &Path, config: &Config, args: CheckArgs, package_filter: Option<&str>) -> Result<()> {
    let repository = repo::open(repo_path).context("failed to open repository")?;
    let packages = resolve_packages(config, package_filter)?;

    let mut all_errors: Vec<String> = Vec::new();
    let mut pkg_results = Vec::new();

    for pkg in &packages {
        let mut errors: Vec<String> = Vec::new();

        let latest = tags::find_latest_tag(&repository, &pkg.tag_prefix, TagFilter::StableOnly)?;
        let tag_version = latest.as_ref().map(|t| t.version.to_string());

        // Check ecosystem file versions match tag
        let mut file_versions = serde_json::Map::new();
        for eco_config in &pkg.ecosystems {
            let eco = bumper::create_ecosystem(eco_config)?;
            let files = eco.modified_files();
            let label = files.first().map(|f| f.display().to_string()).unwrap_or_default();
            match eco.read_version(repo_path) {
                Ok(v) => { file_versions.insert(label.clone(), serde_json::Value::String(v.to_string())); }
                Err(e) => { errors.push(format!("{}: {e}", label)); }
            }
        }

        if let Some(ref tv) = tag_version {
            for (file, version) in &file_versions {
                if let Some(v) = version.as_str() {
                    if v != tv {
                        errors.push(format!("{file} has version {v} but latest tag is {tv}"));
                    }
                }
            }
        }

        // Check changelog
        let changelog_path = repo_path.join(pkg.resolved_changelog_path());
        let changelog_ok = if let Some(ref tv) = tag_version {
            if let Ok(content) = std::fs::read_to_string(&changelog_path) {
                if reader::extract_section(&content, tv).is_none() {
                    errors.push(format!("{} missing section for {tv}", pkg.resolved_changelog_path().display()));
                    false
                } else {
                    true
                }
            } else {
                errors.push(format!("{} not found", pkg.resolved_changelog_path().display()));
                false
            }
        } else {
            true
        };

        pkg_results.push(serde_json::json!({
            "package": pkg.name,
            "consistent": errors.is_empty(),
            "tag_version": tag_version,
            "file_versions": file_versions,
            "changelog_has_section": changelog_ok,
            "errors": errors,
        }));

        if !args.json && !errors.is_empty() {
            for e in &errors {
                eprintln!("FAIL [{}]: {e}", pkg.name);
            }
        }
        all_errors.extend(errors);
    }

    let consistent = all_errors.is_empty();

    if args.json {
        println!("{}", serde_json::to_string_pretty(&pkg_results)?);
    } else if consistent {
        eprintln!("All checks passed.");
    }

    if !consistent {
        return Err(ExitCode::ValidationFailed.into());
    }

    Ok(())
}
