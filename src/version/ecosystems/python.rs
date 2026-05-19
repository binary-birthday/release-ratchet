use std::path::{Path, PathBuf};

use semver::Version;

use super::Ecosystem;
use crate::error::RatchetError;

pub struct PythonEcosystem {
    pub path: PathBuf,
}

impl Ecosystem for PythonEcosystem {
    fn read_version(&self, repo_root: &Path) -> Result<Version, RatchetError> {
        let full_path = repo_root.join(&self.path);
        let contents = std::fs::read_to_string(&full_path).map_err(|e| {
            RatchetError::VersionFile {
                path: self.path.display().to_string(),
                reason: e.to_string(),
            }
        })?;
        let doc = contents.parse::<toml_edit::DocumentMut>().map_err(|e| {
            RatchetError::VersionFile {
                path: self.path.display().to_string(),
                reason: format!("invalid TOML: {e}"),
            }
        })?;
        let version_str = doc["project"]["version"]
            .as_str()
            .ok_or_else(|| RatchetError::VersionFile {
                path: self.path.display().to_string(),
                reason: "missing project.version".to_string(),
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
        let mut doc = contents.parse::<toml_edit::DocumentMut>().map_err(|e| {
            RatchetError::VersionFile {
                path: self.path.display().to_string(),
                reason: format!("invalid TOML: {e}"),
            }
        })?;
        doc["project"]["version"] = toml_edit::value(version.to_string());
        std::fs::write(&full_path, doc.to_string()).map_err(|e| {
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
