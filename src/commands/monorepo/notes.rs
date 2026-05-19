use std::path::Path;
use anyhow::Result;
use crate::cli::NotesArgs;
use crate::config::Config;

pub fn execute(_repo_path: &Path, _config: &Config, _args: NotesArgs, _package_filter: Option<&str>) -> Result<()> {
    anyhow::bail!("monorepo notes is not yet implemented")
}
