use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::conventional::types::CommitType;
use crate::error::RatchetError;
use crate::semver_bump::BumpLevel;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "default_tag_prefix")]
    pub tag_prefix: String,

    #[serde(default = "default_main_branch")]
    pub main_branch: String,

    #[serde(default = "default_release_branch")]
    pub release_branch: String,

    #[serde(default = "default_changelog_path")]
    pub changelog_path: PathBuf,

    #[serde(default)]
    pub ecosystems: Vec<EcosystemConfig>,

    #[serde(default)]
    pub commit_type_overrides: HashMap<String, CommitTypeOverride>,

    #[serde(default)]
    pub sign_tags: bool,

    #[serde(default)]
    pub sign_commits: bool,
}

impl Config {
    pub fn bump_for_type(&self, commit_type: &CommitType) -> BumpLevel {
        if let Some(override_cfg) = self.commit_type_overrides.get(commit_type.as_str()) {
            override_cfg.bump.to_bump_level()
        } else {
            commit_type.default_bump()
        }
    }

    pub fn changelog_heading_for_type(&self, commit_type: &CommitType) -> Option<String> {
        if let Some(override_cfg) = self.commit_type_overrides.get(commit_type.as_str()) {
            override_cfg.changelog.clone()
        } else {
            commit_type.default_changelog_heading().map(String::from)
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            tag_prefix: default_tag_prefix(),
            main_branch: default_main_branch(),
            release_branch: default_release_branch(),
            changelog_path: default_changelog_path(),
            ecosystems: Vec::new(),
            commit_type_overrides: HashMap::new(),
            sign_tags: false,
            sign_commits: false,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CommitTypeOverride {
    pub bump: BumpLevelConfig,
    #[serde(default)]
    pub changelog: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum BumpLevelConfig {
    Major,
    Minor,
    Patch,
    None,
}

impl BumpLevelConfig {
    pub fn to_bump_level(&self) -> BumpLevel {
        match self {
            Self::Major => BumpLevel::Major,
            Self::Minor => BumpLevel::Minor,
            Self::Patch => BumpLevel::Patch,
            Self::None => BumpLevel::None,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "type")]
pub enum EcosystemConfig {
    #[serde(rename = "cargo")]
    Cargo {
        #[serde(default = "default_cargo_path")]
        path: PathBuf,
    },
    #[serde(rename = "node")]
    Node {
        #[serde(default = "default_node_path")]
        path: PathBuf,
    },
    #[serde(rename = "python")]
    Python {
        #[serde(default = "default_python_path")]
        path: PathBuf,
    },
    #[serde(rename = "generic")]
    Generic { path: PathBuf, pattern: String },
}

fn default_tag_prefix() -> String {
    "v".to_string()
}
fn default_main_branch() -> String {
    "main".to_string()
}
fn default_release_branch() -> String {
    "release-ratchet--release".to_string()
}
fn default_changelog_path() -> PathBuf {
    PathBuf::from("CHANGELOG.md")
}
fn default_cargo_path() -> PathBuf {
    PathBuf::from("Cargo.toml")
}
fn default_node_path() -> PathBuf {
    PathBuf::from("package.json")
}
fn default_python_path() -> PathBuf {
    PathBuf::from("pyproject.toml")
}

pub fn load_config(repo_root: &Path, config_path: Option<&Path>) -> Result<Config, RatchetError> {
    let path = match config_path {
        Some(p) => p.to_path_buf(),
        None => repo_root.join(".release-ratchet.yml"),
    };

    if !path.exists() {
        log::info!("No config file found at {}, using defaults", path.display());
        return Ok(Config::default());
    }

    let contents = std::fs::read_to_string(&path).map_err(|e| RatchetError::Config(format!(
        "failed to read {}: {e}",
        path.display()
    )))?;

    serde_yaml::from_str(&contents).map_err(|e| RatchetError::Config(format!(
        "failed to parse {}: {e}",
        path.display()
    )))
}
