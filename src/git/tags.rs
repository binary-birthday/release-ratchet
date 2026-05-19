use git2::{ObjectType, Repository};
use semver::Version;

use crate::error::RatchetError;

pub struct ReleaseTag {
    pub name: String,
    pub version: Version,
    pub oid: git2::Oid,
}

pub fn find_latest_release_tag(
    repo: &Repository,
    tag_prefix: &str,
) -> Result<Option<ReleaseTag>, RatchetError> {
    let pattern = format!("{tag_prefix}*");
    let tag_names = repo.tag_names(Some(&pattern))?;

    // Build set of commits reachable from HEAD to scope tags to current branch
    let reachable = reachable_from_head(repo)?;

    let mut releases: Vec<ReleaseTag> = Vec::new();

    for tag_name_opt in tag_names.iter() {
        let tag_name = match tag_name_opt {
            Some(name) => name,
            None => continue,
        };

        let version_str = match tag_name.strip_prefix(tag_prefix) {
            Some(s) => s,
            None => tag_name,
        };

        let version = match Version::parse(version_str) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Skip pre-release tags (e.g., v1.0.0-rc.1) -- only consider stable releases
        if !version.pre.is_empty() {
            log::debug!("skipping pre-release tag: {tag_name}");
            continue;
        }

        let ref_name = format!("refs/tags/{tag_name}");
        let oid = match repo.refname_to_id(&ref_name) {
            Ok(oid) => {
                // Peel through annotated tags to the commit
                match repo
                    .find_object(oid, None)
                    .and_then(|obj| obj.peel(ObjectType::Commit))
                {
                    Ok(commit_obj) => commit_obj.id(),
                    Err(_) => oid,
                }
            }
            Err(_) => continue,
        };

        // Only consider tags reachable from HEAD (scopes to current branch)
        if !reachable.contains(&oid) {
            log::debug!("skipping tag not reachable from HEAD: {tag_name}");
            continue;
        }

        releases.push(ReleaseTag {
            name: tag_name.to_string(),
            version,
            oid,
        });
    }

    releases.sort_by(|a, b| b.version.cmp(&a.version));
    Ok(releases.into_iter().next())
}

/// Collect all commit OIDs reachable from HEAD.
fn reachable_from_head(repo: &Repository) -> Result<std::collections::HashSet<git2::Oid>, RatchetError> {
    let mut set = std::collections::HashSet::new();
    if let Ok(head) = repo.head() {
        if let Ok(commit) = head.peel_to_commit() {
            let mut revwalk = repo.revwalk()?;
            revwalk.push(commit.id())?;
            for oid in revwalk {
                if let Ok(oid) = oid {
                    set.insert(oid);
                }
            }
        }
    }
    Ok(set)
}

pub fn create_tag(
    repo: &Repository,
    name: &str,
    target_oid: git2::Oid,
    sign: bool,
) -> Result<(), RatchetError> {
    if repo.refname_to_id(&format!("refs/tags/{name}")).is_ok() {
        return Err(RatchetError::TagAlreadyExists {
            tag: name.to_string(),
        });
    }

    let target = repo.find_object(target_oid, None)?;

    if sign {
        let sig = repo.signature()?;
        repo.tag(name, &target, &sig, &format!("Release {name}"), false)?;
    } else {
        repo.tag_lightweight(name, &target, false)?;
    }

    Ok(())
}
