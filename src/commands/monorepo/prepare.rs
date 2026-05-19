use std::path::Path;
use anyhow::Result;
use crate::cli::PrepareArgs;
use crate::config::Config;

pub fn execute(_repo_path: &Path, _config: &Config, _args: PrepareArgs, _package_filter: Option<&str>) -> Result<()> {
    anyhow::bail!("monorepo prepare is not yet implemented")
}
