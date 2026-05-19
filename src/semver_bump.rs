use semver::Version;

use crate::config::Config;
use crate::conventional::types::ConventionalCommit;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BumpLevel {
    None,
    Patch,
    Minor,
    Major,
}

impl std::fmt::Display for BumpLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::Patch => write!(f, "patch"),
            Self::Minor => write!(f, "minor"),
            Self::Major => write!(f, "major"),
        }
    }
}

pub fn determine_bump(commits: &[ConventionalCommit], config: &Config) -> BumpLevel {
    let mut level = BumpLevel::None;
    for commit in commits {
        if commit.is_breaking() {
            return BumpLevel::Major;
        }
        let commit_level = config.bump_for_type(&commit.commit_type);
        if commit_level > level {
            level = commit_level;
        }
    }
    level
}

pub fn apply_bump(version: &Version, bump: BumpLevel) -> Version {
    match bump {
        BumpLevel::Major => Version::new(version.major + 1, 0, 0),
        BumpLevel::Minor => Version::new(version.major, version.minor + 1, 0),
        BumpLevel::Patch => Version::new(version.major, version.minor, version.patch + 1),
        BumpLevel::None => version.clone(),
    }
}

/// Strip pre-release and build metadata, returning just major.minor.patch.
pub fn base_version(v: &Version) -> Version {
    Version::new(v.major, v.minor, v.patch)
}

/// Compute the next pre-release version.
///
/// - `last_stable`: the latest stable release (e.g., 0.5.0)
/// - `last_prerelease`: the latest pre-release tag for the computed base, if any
/// - `bump`: bump level from conventional commits
/// - `prerelease_id`: e.g., "alpha", "beta", "rc"
pub fn compute_prerelease_version(
    last_stable: &Version,
    last_prerelease: Option<&Version>,
    bump: BumpLevel,
    prerelease_id: &str,
) -> Version {
    // Step 1: determine base version
    let base = match last_prerelease {
        Some(pre) => base_version(pre),
        None => apply_bump(last_stable, bump),
    };

    // Step 2: determine pre-release number
    let number = match last_prerelease {
        Some(pre) => {
            // Parse existing pre-release: expect "{id}.{n}"
            let pre_str = pre.pre.as_str();
            match parse_prerelease_parts(pre_str) {
                Some((existing_id, n)) if existing_id == prerelease_id => n + 1,
                _ => 1, // different id or unparseable → reset
            }
        }
        None => 1,
    };

    let mut version = base;
    version.pre = semver::Prerelease::new(&format!("{prerelease_id}.{number}")).unwrap();
    version
}

/// Parse a pre-release string like "alpha.3" into ("alpha", 3).
fn parse_prerelease_parts(pre: &str) -> Option<(&str, u64)> {
    let dot = pre.rfind('.')?;
    let id = &pre[..dot];
    let n = pre[dot + 1..].parse().ok()?;
    Some((id, n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_major() {
        assert_eq!(apply_bump(&Version::new(1, 2, 3), BumpLevel::Major), Version::new(2, 0, 0));
    }

    #[test]
    fn apply_minor() {
        assert_eq!(apply_bump(&Version::new(1, 2, 3), BumpLevel::Minor), Version::new(1, 3, 0));
    }

    #[test]
    fn apply_patch() {
        assert_eq!(apply_bump(&Version::new(1, 2, 3), BumpLevel::Patch), Version::new(1, 2, 4));
    }

    #[test]
    fn apply_none() {
        assert_eq!(apply_bump(&Version::new(1, 2, 3), BumpLevel::None), Version::new(1, 2, 3));
    }

    #[test]
    fn bump_ordering() {
        assert!(BumpLevel::Major > BumpLevel::Minor);
        assert!(BumpLevel::Minor > BumpLevel::Patch);
        assert!(BumpLevel::Patch > BumpLevel::None);
    }

    #[test]
    fn base_version_strips_pre() {
        let v = Version::parse("1.2.3-alpha.1").unwrap();
        assert_eq!(base_version(&v), Version::new(1, 2, 3));
    }

    #[test]
    fn prerelease_first_alpha() {
        let stable = Version::new(0, 5, 0);
        let v = compute_prerelease_version(&stable, None, BumpLevel::Major, "alpha");
        assert_eq!(v, Version::parse("1.0.0-alpha.1").unwrap());
    }

    #[test]
    fn prerelease_increment_alpha() {
        let stable = Version::new(0, 5, 0);
        let prev = Version::parse("1.0.0-alpha.2").unwrap();
        let v = compute_prerelease_version(&stable, Some(&prev), BumpLevel::Major, "alpha");
        assert_eq!(v, Version::parse("1.0.0-alpha.3").unwrap());
    }

    #[test]
    fn prerelease_switch_id_resets() {
        let stable = Version::new(0, 5, 0);
        let prev = Version::parse("1.0.0-alpha.3").unwrap();
        let v = compute_prerelease_version(&stable, Some(&prev), BumpLevel::Major, "beta");
        assert_eq!(v, Version::parse("1.0.0-beta.1").unwrap());
    }

    #[test]
    fn prerelease_minor_bump() {
        let stable = Version::new(1, 2, 0);
        let v = compute_prerelease_version(&stable, None, BumpLevel::Minor, "rc");
        assert_eq!(v, Version::parse("1.3.0-rc.1").unwrap());
    }
}
