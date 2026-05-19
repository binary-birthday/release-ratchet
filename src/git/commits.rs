use std::collections::HashSet;

use git2::Repository;

use crate::conventional::parser;
use crate::conventional::types::ConventionalCommit;
use crate::error::RatchetError;

pub struct CommitCollection {
    pub conventional: Vec<ConventionalCommit>,
    pub non_conventional_count: usize,
}

/// Collect commits from HEAD back to `since_oid` (exclusive).
pub fn collect_since_tag(
    repo: &Repository,
    since_oid: Option<git2::Oid>,
) -> Result<CommitCollection, RatchetError> {
    collect_since_tag_bounded(repo, since_oid, None)
}

/// Collect commits in range (`since_oid`..`until_oid`].
/// If `until_oid` is None, walks from HEAD.
/// If `since_oid` is None, walks to the root.
pub fn collect_since_tag_bounded(
    repo: &Repository,
    since_oid: Option<git2::Oid>,
    until_oid: Option<git2::Oid>,
) -> Result<CommitCollection, RatchetError> {
    // Handle empty repo (no HEAD)
    if until_oid.is_none() && repo.head().is_err() {
        return Ok(CommitCollection {
            conventional: Vec::new(),
            non_conventional_count: 0,
        });
    }

    let mut revwalk = repo.revwalk()?;

    match until_oid {
        Some(oid) => revwalk.push(oid)?,
        None => revwalk.push_head()?,
    }

    revwalk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME)?;

    if let Some(oid) = since_oid {
        revwalk.hide(oid)?;
    }

    // If we have a bounded range with the same since and until ancestors,
    // we need to also exclude `since_oid` itself (hide does this).
    // But we also need to exclude `until_oid` if it's the same as since_oid.
    let excluded: HashSet<git2::Oid> = since_oid.into_iter().collect();

    let mut conventional = Vec::new();
    let mut non_conventional_count = 0;

    for oid_result in revwalk {
        let oid = oid_result?;
        if excluded.contains(&oid) {
            continue;
        }
        let commit = repo.find_commit(oid)?;
        let message = commit.message().unwrap_or("").to_string();
        let author = commit
            .author()
            .name()
            .unwrap_or("Unknown")
            .to_string();

        match parser::parse_commit(oid, &message, &author) {
            Some(cc) => conventional.push(cc),
            None => {
                log::debug!(
                    "non-conventional commit: {} {}",
                    &oid.to_string().get(..7).unwrap_or(&oid.to_string()),
                    message.lines().next().unwrap_or("")
                );
                non_conventional_count += 1;
            }
        }
    }

    Ok(CommitCollection {
        conventional,
        non_conventional_count,
    })
}
