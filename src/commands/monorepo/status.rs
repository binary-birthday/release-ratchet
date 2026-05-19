use std::path::Path;
use anyhow::Result;
use crate::cli::StatusArgs;
use crate::config::Config;

pub fn execute(_repo_path: &Path, _config: &Config, _args: StatusArgs, _package_filter: Option<&str>) -> Result<()> {
    anyhow::bail!("monorepo status is not yet implemented")
}
