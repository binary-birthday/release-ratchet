use std::path::{Path, PathBuf};

use regex::Regex;
use semver::Version;

use super::Ecosystem;
use crate::error::RatchetError;

pub struct GenericEcosystem {
    pub path: PathBuf,
    pub pattern: String,
}

impl Ecosystem for GenericEcosystem {
    fn read_version(&self, repo_root: &Path) -> Result<Version, RatchetError> {
        let full_path = repo_root.join(&self.path);
        let contents = std::fs::read_to_string(&full_path).map_err(|e| {
            RatchetError::VersionFile {
                path: self.path.display().to_string(),
                reason: e.to_string(),
            }
        })?;
        let re = Regex::new(&self.pattern).map_err(|e| RatchetError::VersionFile {
            path: self.path.display().to_string(),
            reason: format!("invalid regex pattern: {e}"),
        })?;
        let caps = re.captures(&contents).ok_or_else(|| RatchetError::VersionFile {
            path: self.path.display().to_string(),
            reason: format!("pattern '{}' did not match", self.pattern),
        })?;
        let version_str = caps.get(1).ok_or_else(|| RatchetError::VersionFile {
            path: self.path.display().to_string(),
            reason: "pattern must have a capture group for the version".to_string(),
        })?.as_str();
        Version::parse(version_str).map_err(|e| RatchetError::VersionFile {
            path: self.path.display().to_string(),
            reason: format!("invalid semver '{version_str}': {e}"),
        })
    }

    fn write_version(&self, repo_root: &Path, version: &Version) -> Result<(), RatchetError> {
        let full_path = repo_root.join(&self.path);
        let contents = std::fs::read_to_string(&full_path).map_err(|e| {
            RatchetError::VersionFile {
                path: self.path.display().to_string(),
                reason: e.to_string(),
            }
        })?;
        let re = Regex::new(&self.pattern).map_err(|e| RatchetError::VersionFile {
            path: self.path.display().to_string(),
            reason: format!("invalid regex pattern: {e}"),
        })?;
        let caps = re.captures(&contents).ok_or_else(|| RatchetError::VersionFile {
            path: self.path.display().to_string(),
            reason: format!("pattern '{}' did not match", self.pattern),
        })?;
        let full_match = caps.get(0).unwrap();
        let version_match = caps.get(1).ok_or_else(|| RatchetError::VersionFile {
            path: self.path.display().to_string(),
            reason: "pattern must have a capture group for the version".to_string(),
        })?;

        // Replace just the captured version within the full match
        let new_full = format!(
            "{}{}{}",
            &full_match.as_str()[..version_match.start() - full_match.start()],
            version,
            &full_match.as_str()[version_match.end() - full_match.start()..],
        );

        let result = format!(
            "{}{}{}",
            &contents[..full_match.start()],
            new_full,
            &contents[full_match.end()..],
        );

        std::fs::write(&full_path, result).map_err(|e| RatchetError::VersionFile {
            path: self.path.display().to_string(),
            reason: e.to_string(),
        })
    }

    fn modified_files(&self) -> Vec<PathBuf> {
        vec![self.path.clone()]
    }
}
