use std::path::{Path, PathBuf};

use semver::Version;

use super::ecosystems::{self, Ecosystem};
use crate::config::EcosystemConfig;
use crate::error::RatchetError;

pub fn create_ecosystem(config: &EcosystemConfig) -> Result<Box<dyn Ecosystem>, RatchetError> {
    Ok(match config {
        EcosystemConfig::Cargo { path } => Box::new(ecosystems::cargo::CargoEcosystem {
            path: path.clone(),
        }),
        EcosystemConfig::Node { path } => Box::new(ecosystems::node::NodeEcosystem {
            path: path.clone(),
        }),
        EcosystemConfig::Python { path } => Box::new(ecosystems::python::PythonEcosystem {
            path: path.clone(),
        }),
        EcosystemConfig::Generic { path, pattern } => {
            Box::new(ecosystems::generic::GenericEcosystem::new(path.clone(), pattern)?)
        }
    })
}

pub fn bump_all(
    repo_root: &Path,
    ecosystem_configs: &[EcosystemConfig],
    version: &Version,
) -> Result<Vec<PathBuf>, RatchetError> {
    let mut modified = Vec::new();
    for eco_config in ecosystem_configs {
        let eco = create_ecosystem(eco_config)?;
        eco.write_version(repo_root, version)?;
        modified.extend(eco.modified_files());
    }
    Ok(modified)
}
