use std::path::Path;

use anyhow::{Context, Result};

use crate::cli::ValidateArgs;
use crate::config::Config;
use crate::conventional::parser;
use crate::error::ExitCode;
use crate::git::{commits, repo, tags};

pub fn execute(repo_path: &Path, config: &Config, args: ValidateArgs) -> Result<()> {
    if let Some(ref message) = args.message {
        let oid = git2::Oid::zero();
        match parser::parse_commit(oid, message, "validate") {
            Some(c) => {
                eprintln!(
                    "Valid: {} {}",
                    c.commit_type.as_str(),
                    if c.is_breaking() { "(BREAKING)" } else { "" }
                );
                return Ok(());
            }
            None => {
                eprintln!("Invalid: not a conventional commit message");
                return Err(ExitCode::ValidationFailed.into());
            }
        }
    }

    let repository = repo::open(repo_path).context("failed to open repository")?;

    let (since_oid, to_oid, range_desc) = if let Some(ref range) = args.range {
        if let Some((from, to)) = range.split_once("..") {
            let from_oid = repo::resolve_ref(&repository, from)
                .context(format!("failed to resolve '{from}'"))?;
            let to_oid = repo::resolve_ref(&repository, to)
                .context(format!("failed to resolve '{to}'"))?;
            (Some(from_oid), Some(to_oid), range.clone())
        } else {
            let from_oid = repo::resolve_ref(&repository, range)
                .context(format!("failed to resolve '{range}'"))?;
            (Some(from_oid), None, format!("{range}..HEAD"))
        }
    } else {
        let latest = tags::find_latest_release_tag(&repository, &config.tag_prefix)?;
        match latest {
            Some(tag) => (Some(tag.oid), None, format!("{}..HEAD", tag.name)),
            None => (None, None, "all commits".to_string()),
        }
    };

    eprintln!("Validating commits in range: {range_desc}");

    let collection = commits::collect_since_tag_bounded(&repository, since_oid, to_oid, config.forge.as_ref())?;

    let total = collection.conventional.len() + collection.non_conventional_count;

    if total == 0 {
        eprintln!("No commits found in range.");
        return Ok(());
    }

    eprintln!(
        "{} / {total} commits are valid conventional commits",
        collection.conventional.len()
    );

    if collection.non_conventional_count > 0 {
        eprintln!(
            "{} commit(s) are NOT valid conventional commits",
            collection.non_conventional_count
        );
        return Err(ExitCode::ValidationFailed.into());
    }

    eprintln!("All commits are valid.");
    Ok(())
}
