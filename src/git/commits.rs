use git2::Repository;

use crate::conventional::parser;
use crate::conventional::types::ConventionalCommit;
use crate::error::RatchetError;

pub struct CommitCollection {
    pub conventional: Vec<ConventionalCommit>,
    pub non_conventional_count: usize,
}

pub fn collect_since_tag(
    repo: &Repository,
    since_oid: Option<git2::Oid>,
) -> Result<CommitCollection, RatchetError> {
    // Handle empty repo (no HEAD)
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
        let message = commit.message().unwrap_or("").to_string();
        let author = commit
            .author()
            .name()
            .unwrap_or("Unknown")
            .to_string();

        match parser::parse_commit(oid, &message, &author) {
            Some(cc) => conventional.push(cc),
            None => {
                log::debug!("non-conventional commit: {} {}", &oid.to_string()[..7], message.lines().next().unwrap_or(""));
                non_conventional_count += 1;
            }
        }
    }

    Ok(CommitCollection {
        conventional,
        non_conventional_count,
    })
}
