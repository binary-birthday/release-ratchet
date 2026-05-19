use std::path::{Path, PathBuf};

use semver::Version;

use super::Ecosystem;
use crate::error::RatchetError;

pub struct CargoEcosystem {
    pub path: PathBuf,
}

impl CargoEcosystem {
    fn lockfile_path(&self) -> PathBuf {
        self.path
            .parent()
            .map(|p| p.join("Cargo.lock"))
            .unwrap_or_else(|| PathBuf::from("Cargo.lock"))
    }
}

impl Ecosystem for CargoEcosystem {
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
        let version_str = doc["package"]["version"]
            .as_str()
            .ok_or_else(|| RatchetError::VersionFile {
                path: self.path.display().to_string(),
                reason: "missing package.version".to_string(),
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
        doc["package"]["version"] = toml_edit::value(version.to_string());
        std::fs::write(&full_path, doc.to_string()).map_err(|e| {
            RatchetError::VersionFile {
                path: self.path.display().to_string(),
                reason: e.to_string(),
            }
        })?;

        // Also update the version in Cargo.lock if it exists and contains
        // a matching package entry. This keeps the lockfile consistent without
        // shelling out to cargo (which can modify other files).
        let lockfile_path = repo_root.join(self.lockfile_path());
        if lockfile_path.exists() {
            if let Ok(lock_contents) = std::fs::read_to_string(&lockfile_path) {
                if let Ok(mut lock_doc) = lock_contents.parse::<toml_edit::DocumentMut>() {
                    if let Some(packages) = lock_doc.get_mut("package").and_then(|p| p.as_array_of_tables_mut()) {
                        // Read the package name from Cargo.toml to match
                        let pkg_name = doc["package"]["name"].as_str().unwrap_or("");
                        for pkg in packages.iter_mut() {
                            if pkg.get("name").and_then(|n| n.as_str()) == Some(pkg_name) {
                                pkg["version"] = toml_edit::value(version.to_string());
                            }
                        }
                        if let Err(e) = std::fs::write(&lockfile_path, lock_doc.to_string()) {
                            log::warn!("failed to update Cargo.lock: {e}");
                        } else {
                            log::info!("updated version in Cargo.lock");
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn modified_files(&self) -> Vec<PathBuf> {
        let mut files = vec![self.path.clone()];
        files.push(self.lockfile_path());
        files
    }
}
