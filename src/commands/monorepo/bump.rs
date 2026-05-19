use std::path::Path;
use anyhow::Result;
use crate::cli::BumpArgs;
use crate::config::Config;

pub fn execute(_repo_path: &Path, _config: &Config, _args: BumpArgs, _package_filter: Option<&str>) -> Result<()> {
    anyhow::bail!("monorepo bump is not yet implemented")
}
