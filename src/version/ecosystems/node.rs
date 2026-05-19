use std::path::{Path, PathBuf};

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
        let mut json: serde_json::Value = serde_json::from_str(&contents).map_err(|e| {
            RatchetError::VersionFile {
                path: self.path.display().to_string(),
                reason: format!("invalid JSON: {e}"),
            }
        })?;
        json["version"] = serde_json::Value::String(version.to_string());
        let output = serde_json::to_string_pretty(&json).map_err(|e| {
            RatchetError::VersionFile {
                path: self.path.display().to_string(),
                reason: format!("failed to serialize JSON: {e}"),
            }
        })?;
        // Preserve trailing newline convention
        std::fs::write(&full_path, format!("{output}\n")).map_err(|e| {
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
