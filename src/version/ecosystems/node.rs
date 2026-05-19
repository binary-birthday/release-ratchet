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

        // Parse to find the exact top-level "version" value, then do a targeted
        // string replacement at that byte offset. This ensures we modify only the
        // top-level field (not nested "version" keys) while preserving formatting.
        let json: serde_json::Value = serde_json::from_str(&contents).map_err(|e| {
            RatchetError::VersionFile {
                path: self.path.display().to_string(),
                reason: format!("invalid JSON: {e}"),
            }
        })?;
        let old_version = json["version"].as_str().ok_or_else(|| {
            RatchetError::VersionFile {
                path: self.path.display().to_string(),
                reason: "missing \"version\" field".to_string(),
            }
        })?;

        // Find the top-level "version": "..." pattern by searching for the key
        // followed by the exact old version string. We search for the literal
        // old value to avoid matching nested occurrences.
        let (pos, replace_len) = find_version_needle(&contents, old_version)
            .ok_or_else(|| RatchetError::VersionFile {
                path: self.path.display().to_string(),
                reason: format!(
                    "could not locate top-level \"version\": \"{old_version}\" in file"
                ),
            })?;

        let mut result = String::with_capacity(contents.len());
        result.push_str(&contents[..pos]);
        result.push_str(&version.to_string());
        result.push_str(&contents[pos + replace_len..]);

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

/// Find the byte offset and length of the version *value* in the first
/// top-level `"version": "<value>"` occurrence that matches `old_version`.
fn find_version_needle(contents: &str, old_version: &str) -> Option<(usize, usize)> {
    // Try patterns: `"version": "X.Y.Z"`, `"version":"X.Y.Z"`, `"version" : "X.Y.Z"`
    for pattern in [
        format!("\"version\": \"{old_version}\""),
        format!("\"version\":\"{old_version}\""),
        format!("\"version\" : \"{old_version}\""),
    ] {
        if let Some(match_start) = contents.find(&pattern) {
            // Find the version value within the matched pattern
            let value_start = match_start + pattern.find(old_version).unwrap();
            return Some((value_start, old_version.len()));
        }
    }
    None
}
