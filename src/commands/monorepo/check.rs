use std::path::Path;
use anyhow::Result;
use crate::cli::CheckArgs;
use crate::config::Config;

pub fn execute(_repo_path: &Path, _config: &Config, _args: CheckArgs, _package_filter: Option<&str>) -> Result<()> {
    anyhow::bail!("monorepo check is not yet implemented")
}
