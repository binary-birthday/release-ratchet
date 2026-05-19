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
}
