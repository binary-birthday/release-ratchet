use git2::{ObjectType, Repository};
use semver::Version;

use crate::error::RatchetError;

pub struct ReleaseTag {
    pub name: String,
    pub version: Version,
    pub oid: git2::Oid,
}

pub enum TagFilter {
    /// Only stable releases (no pre-release segment).
    StableOnly,
    /// Only pre-releases whose base version (major.minor.patch) matches.
    PrereleasesOf(Version),
    /// Any valid semver tag (stable or pre-release), highest wins.
    Any,
}

/// Find the latest release tag matching the filter, reachable from HEAD.
pub fn find_latest_tag(
    repo: &Repository,
    tag_prefix: &str,
    filter: TagFilter,
) -> Result<Option<ReleaseTag>, RatchetError> {
    let pattern = format!("{tag_prefix}*");
    let tag_names = repo.tag_names(Some(&pattern))?;

    let head_oid = match repo.head() {
        Ok(head) => match head.peel_to_commit() {
            Ok(c) => Some(c.id()),
            Err(_) => None,
        },
        Err(_) => None,
    };

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

        // Apply filter
        match &filter {
            TagFilter::StableOnly => {
                if !version.pre.is_empty() {
                    continue;
                }
            }
            TagFilter::PrereleasesOf(base) => {
                if version.pre.is_empty() {
                    continue;
                }
                if version.major != base.major
                    || version.minor != base.minor
                    || version.patch != base.patch
                {
                    continue;
                }
            }
            TagFilter::Any => {}
        }

        let ref_name = format!("refs/tags/{tag_name}");
        let oid = match repo.refname_to_id(&ref_name) {
            Ok(oid) => {
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

        if let Some(head) = head_oid {
            match repo.graph_descendant_of(head, oid) {
                Ok(true) => {}
                Ok(false) => {
                    if head != oid {
                        continue;
                    }
                }
                Err(_) => continue,
            }
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

/// Convenience wrapper: find latest stable release tag.
pub fn find_latest_release_tag(
    repo: &Repository,
    tag_prefix: &str,
) -> Result<Option<ReleaseTag>, RatchetError> {
    find_latest_tag(repo, tag_prefix, TagFilter::StableOnly)
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
