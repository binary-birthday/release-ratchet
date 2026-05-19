pub mod bump;
pub mod check;
pub mod notes;
pub mod prepare;
pub mod release;
pub mod status;

use anyhow::Result;

use crate::config::{Config, PackageConfig, SharedPathConfig};

/// Resolve which packages to operate on based on the --package filter.
pub fn resolve_packages<'a>(
    config: &'a Config,
    filter: Option<&str>,
) -> Result<Vec<&'a PackageConfig>> {
    if !config.is_monorepo() {
        if filter.is_some() {
            anyhow::bail!("--package is only valid in monorepo mode (define [[packages]] in config)");
        }
        return Ok(vec![]);
    }
    match filter {
        Some(name) => {
            let pkg = config
                .packages
                .iter()
                .find(|p| p.name == name)
                .ok_or_else(|| anyhow::anyhow!("package '{name}' not found in config"))?;
            Ok(vec![pkg])
        }
        None => Ok(config.packages.iter().collect()),
    }
}

/// Get all path prefixes that should attribute commits to a given package,
/// including the package's own path and any shared_paths that affect it.
pub fn path_prefixes_for_package(
    pkg: &PackageConfig,
    shared_paths: &[SharedPathConfig],
) -> Vec<String> {
    let mut prefixes = vec![pkg.path_prefix()];
    for shared in shared_paths {
        if shared.affects.iter().any(|a| a == &pkg.name) {
            let s = shared.path.to_string_lossy();
            if s.ends_with('/') {
                prefixes.push(s.to_string());
            } else {
                prefixes.push(format!("{s}/"));
            }
        }
    }
    prefixes
}
