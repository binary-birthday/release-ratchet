pub mod cargo;
pub mod generic;
pub mod node;
pub mod python;

use std::path::{Path, PathBuf};

use semver::Version;

use crate::error::RatchetError;

pub trait Ecosystem {
    fn read_version(&self, repo_root: &Path) -> Result<Version, RatchetError>;
    fn write_version(&self, repo_root: &Path, version: &Version) -> Result<(), RatchetError>;
    fn modified_files(&self) -> Vec<PathBuf>;
}
