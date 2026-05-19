use std::path::{Path, PathBuf};

use regex::Regex;
use semver::Version;

use super::Ecosystem;
use crate::error::RatchetError;

pub struct NodeEcosystem {
    pub path: PathBuf,
}

impl Ecosystem for NodeEcosystem {
    fn read_version(&self, repo_root: &Path) -> Result<Version, RatchetError> {
        let full_path = repo_root.join(&self.path);
        let contents = std::fs::read_to_string(&full_path).map_err(|e| {
            RatchetError::VersionFile {
                path: self.path.display().to_string(),
                reason: e.to_string(),
            }
        })?;
        let json: serde_json::Value = serde_json::from_str(&contents).map_err(|e| {
            RatchetError::VersionFile {
                path: self.path.display().to_string(),
                reason: format!("invalid JSON: {e}"),
            }
        })?;
        let version_str = json["version"].as_str().ok_or_else(|| {
            RatchetError::VersionFile {
                path: self.path.display().to_string(),
                reason: "missing \"version\" field".to_string(),
            }
        })?;
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

        // Use regex replacement to preserve original formatting (indentation, key order, etc.)
        let re = Regex::new(r#"("version"\s*:\s*")([^"]+)(")"#).map_err(|e| {
            RatchetError::VersionFile {
                path: self.path.display().to_string(),
                reason: format!("regex error: {e}"),
            }
        })?;

        if !re.is_match(&contents) {
            return Err(RatchetError::VersionFile {
                path: self.path.display().to_string(),
                reason: "could not find \"version\" field in JSON".to_string(),
            });
        }

        let result = re.replacen(&contents, 1, format!("${{1}}{version}${{3}}"));

        std::fs::write(&full_path, result.as_bytes()).map_err(|e| {
            RatchetError::VersionFile {
                path: self.path.display().to_string(),
                reason: e.to_string(),
            }
        })
    }

    fn modified_files(&self) -> Vec<PathBuf> {
        vec![self.path.clone()]
    }
}
