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

        // Find the top-level "version" field by tracking brace depth.
        // Only match at depth 1 (inside the root object, not nested objects).
        let (pos, len) = find_toplevel_version_value(&contents, old_version)
            .ok_or_else(|| RatchetError::VersionFile {
                path: self.path.display().to_string(),
                reason: format!(
                    "could not locate top-level \"version\": \"{old_version}\" in file"
                ),
            })?;

        let mut result = String::with_capacity(contents.len());
        result.push_str(&contents[..pos]);
        result.push_str(&version.to_string());
        result.push_str(&contents[pos + len..]);

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

/// Find the byte offset and length of the version value string in the
/// top-level "version" field. Uses brace-depth tracking to skip nested
/// "version" keys inside sub-objects.
fn find_toplevel_version_value(contents: &str, old_version: &str) -> Option<(usize, usize)> {
    let bytes = contents.as_bytes();
    let needle_key = "\"version\"";
    let mut depth: i32 = 0;
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            b'"' => {
                // Check if this is the "version" key at depth 1
                if depth == 1 && contents[i..].starts_with(needle_key) {
                    // Skip past the key and find the colon + value
                    let after_key = i + needle_key.len();
                    // Find the colon
                    let colon = contents[after_key..].find(':')?;
                    let after_colon = after_key + colon + 1;
                    // Find the opening quote of the value
                    let quote_start = contents[after_colon..].find('"')?;
                    let value_start = after_colon + quote_start + 1;
                    // Find the closing quote
                    let quote_end = contents[value_start..].find('"')?;
                    let value = &contents[value_start..value_start + quote_end];

                    if value == old_version {
                        return Some((value_start, quote_end));
                    }
                }
                // Skip the string to avoid counting braces inside strings
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    if bytes[i] == b'\\' {
                        i += 1; // skip escaped char
                    }
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    None
}
