use std::path::Path;

use anyhow::{Context, Result};

use crate::changelog::reader;
use crate::cli::CheckArgs;
use crate::config::Config;
use crate::error::ExitCode;
use crate::git::{repo, tags};
use crate::version::bumper;

pub fn execute(repo_path: &Path, config: &Config, args: CheckArgs) -> Result<()> {
    let repository = repo::open(repo_path).context("failed to open repository")?;
    let mut errors: Vec<String> = Vec::new();

    // 1. Find latest tag
    let latest = tags::find_latest_release_tag(&repository, &config.tag_prefix)
        .context("failed to search for release tags")?;

    let tag_version = latest.as_ref().map(|tag| tag.version.to_string());

    // 2. Read ecosystem file versions
    let mut file_versions = serde_json::Map::new();
    for eco_config in &config.ecosystems {
        let eco = bumper::create_ecosystem(eco_config)?;
        let files = eco.modified_files();
        let label = files.first().map(|f| f.display().to_string()).unwrap_or_default();
        match eco.read_version(repo_path) {
            Ok(v) => { file_versions.insert(label, serde_json::Value::String(v.to_string())); }
            Err(e) => { errors.push(format!("failed to read version from {label}: {e}")); }
        }
    }

    // 3. Check tag-file version match
    if let Some(ref tv) = tag_version {
        for (file, version) in &file_versions {
            if let Some(v) = version.as_str() {
                if v != tv {
                    errors.push(format!("{file} has version {v} but latest tag is {tv}"));
                }
            }
        }
    }

    // 4. Check no version drift between ecosystem files
    let unique_versions: std::collections::HashSet<&str> = file_versions
        .values()
        .filter_map(|v| v.as_str())
        .collect();
    if unique_versions.len() > 1 {
        let versions: Vec<String> = file_versions
            .iter()
            .map(|(f, v)| format!("{f}={}", v.as_str().unwrap_or("?")))
            .collect();
        errors.push(format!("version drift between ecosystem files: {}", versions.join(", ")));
    }

    // 5. Check changelog has section for latest tag
    let changelog_ok = if let Some(ref tv) = tag_version {
        let changelog_path = repo_path.join(&config.changelog_path);
        if let Ok(content) = std::fs::read_to_string(&changelog_path) {
            if reader::extract_section(&content, tv).is_none() {
                errors.push(format!("CHANGELOG.md missing section for {tv}"));
                false
            } else {
                true
            }
        } else {
            errors.push("CHANGELOG.md not found".to_string());
            false
        }
    } else {
        true // no tag = nothing to check
    };

    let consistent = errors.is_empty();

    if args.json {
        let json = serde_json::json!({
            "consistent": consistent,
            "tag_version": tag_version,
            "file_versions": file_versions,
            "changelog_has_section": changelog_ok,
            "errors": errors,
        });
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else {
        if let Some(ref tv) = tag_version {
            eprintln!("Latest tag:     {tv}");
        } else {
            eprintln!("Latest tag:     (none)");
        }
        for (file, version) in &file_versions {
            eprintln!("  {file}: {}", version.as_str().unwrap_or("?"));
        }
        if errors.is_empty() {
            eprintln!("All checks passed.");
        } else {
            for e in &errors {
                eprintln!("FAIL: {e}");
            }
        }
    }

    if !consistent {
        return Err(ExitCode::ValidationFailed.into());
    }

    Ok(())
}
