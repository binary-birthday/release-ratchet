use std::collections::HashSet;

use git2::Repository;

use crate::config::Forge;
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
    forge: Option<&Forge>,
) -> Result<CommitCollection, RatchetError> {
    collect_since_tag_bounded(repo, since_oid, None, forge)
}

/// Collect commits in range (`since_oid`..`until_oid`].
/// If `until_oid` is None, walks from HEAD.
/// If `since_oid` is None, walks to the root.
pub fn collect_since_tag_bounded(
    repo: &Repository,
    since_oid: Option<git2::Oid>,
    until_oid: Option<git2::Oid>,
    forge: Option<&Forge>,
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

        match parser::parse_commit_with_forge(oid, &message, &author, forge) {
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

/// Collect commits filtered to those that touch files under any of the given path prefixes.
/// Each prefix should end with `/` (e.g., "packages/core/").
pub fn collect_since_tag_filtered(
    repo: &Repository,
    since_oid: Option<git2::Oid>,
    path_prefixes: &[String],
    forge: Option<&Forge>,
) -> Result<CommitCollection, RatchetError> {
    if repo.head().is_err() {
        return Ok(CommitCollection {
            conventional: Vec::new(),
            non_conventional_count: 0,
        });
    }

    let mut revwalk = repo.revwalk()?;
    revwalk.push_head()?;
    revwalk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME)?;

    if let Some(oid) = since_oid {
        revwalk.hide(oid)?;
    }

    let mut conventional = Vec::new();
    let mut non_conventional_count = 0;

    for oid_result in revwalk {
        let oid = oid_result?;
        let commit = repo.find_commit(oid)?;

        if !commit_touches_any_path(repo, &commit, path_prefixes)? {
            continue;
        }

        let message = commit.message().unwrap_or("").to_string();
        let author = commit.author().name().unwrap_or("Unknown").to_string();

        match parser::parse_commit_with_forge(oid, &message, &author, forge) {
            Some(cc) => conventional.push(cc),
            None => non_conventional_count += 1,
        }
    }

    Ok(CommitCollection {
        conventional,
        non_conventional_count,
    })
}

/// Check if a commit's diff touches any file under any of the given path prefixes.
fn commit_touches_any_path(
    repo: &Repository,
    commit: &git2::Commit,
    path_prefixes: &[String],
) -> Result<bool, RatchetError> {
    let tree = commit.tree()?;

    if commit.parent_count() == 0 {
        return Ok(path_prefixes.iter().any(|prefix| {
            tree.get_path(std::path::Path::new(prefix.trim_end_matches('/'))).is_ok()
        }));
    }

    let parent_tree = commit.parent(0)?.tree()?;
    let diff = repo.diff_tree_to_tree(Some(&parent_tree), Some(&tree), None)?;

    for delta in diff.deltas() {
        for prefix in path_prefixes {
            if let Some(path) = delta.new_file().path().and_then(|p| p.to_str()) {
                if path.starts_with(prefix.as_str()) {
                    return Ok(true);
                }
            }
            if let Some(path) = delta.old_file().path().and_then(|p| p.to_str()) {
                if path.starts_with(prefix.as_str()) {
                    return Ok(true);
                }
            }
        }
    }

    Ok(false)
}
