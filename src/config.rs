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
    pub cleanup_branch: bool,

    #[serde(default)]
    pub hooks: HooksConfig,

    /// When non-empty, activates monorepo mode.
    #[serde(default)]
    pub packages: Vec<PackageConfig>,

    /// Shared directories that affect specific packages when changed.
    #[serde(default)]
    pub shared_paths: Vec<SharedPathConfig>,
}

impl Config {
    pub fn bump_for_type(&self, commit_type: &CommitType) -> BumpLevel {
        if let Some(override_cfg) = self.commit_type_overrides.get(commit_type.as_str()) {
            override_cfg.bump.to_bump_level()
        } else {
            commit_type.default_bump()
        }
    }

    pub fn is_monorepo(&self) -> bool {
        !self.packages.is_empty()
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
            cleanup_branch: false,
            hooks: HooksConfig::default(),
            packages: Vec::new(),
            shared_paths: Vec::new(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
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

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct HooksConfig {
    #[serde(default)]
    pub post_prepare: Vec<String>,
    #[serde(default)]
    pub post_release: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct PackageConfig {
    /// Unique name for this package.
    pub name: String,
    /// Path prefix for commit attribution (relative to repo root).
    pub path: PathBuf,
    /// Tag prefix for this package (e.g., "core-v" → core-v1.2.3).
    #[serde(default = "default_tag_prefix")]
    pub tag_prefix: String,
    /// Path to this package's CHANGELOG.md. Defaults to {path}/CHANGELOG.md.
    pub changelog_path: Option<PathBuf>,
    /// Ecosystem files for this package.
    #[serde(default)]
    pub ecosystems: Vec<EcosystemConfig>,
}

impl PackageConfig {
    pub fn resolved_changelog_path(&self) -> PathBuf {
        self.changelog_path
            .clone()
            .unwrap_or_else(|| self.path.join("CHANGELOG.md"))
    }

    /// Path prefix with trailing slash for safe prefix matching.
    /// "." normalizes to "" (matches all paths — root package).
    pub fn path_prefix(&self) -> String {
        let s = self.path.to_string_lossy();
        if s == "." || s == "./" {
            return String::new(); // root package matches everything
        }
        if s.ends_with('/') {
            s.to_string()
        } else {
            format!("{s}/")
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct SharedPathConfig {
    /// Path prefix (relative to repo root).
    pub path: PathBuf,
    /// Package names that are affected when this path changes.
    pub affects: Vec<String>,
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
    "chore/next-release".to_string()
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
    let (path, explicit) = match config_path {
        Some(p) => (p.to_path_buf(), true),
        None => (repo_root.join(".release-ratchet.toml"), false),
    };

    if !path.exists() {
        if explicit {
            return Err(RatchetError::Config(format!(
                "config file not found: {}",
                path.display()
            )));
        }
        log::info!("No config file found at {}, using defaults", path.display());
        return Ok(Config {
            ecosystems: detect_ecosystems(repo_root),
            ..Config::default()
        });
    }

    let contents = std::fs::read_to_string(&path).map_err(|e| RatchetError::Config(format!(
        "failed to read {}: {e}",
        path.display()
    )))?;

    let mut config: Config = toml::from_str(&contents).map_err(|e| RatchetError::Config(format!(
        "failed to parse {}: {e}",
        path.display()
    )))?;

    // Validate commit_type_override keys are lowercase
    for key in config.commit_type_overrides.keys() {
        if key != &key.to_lowercase() {
            return Err(RatchetError::Config(format!(
                "commit_type_overrides key '{key}' must be lowercase (use '{}')",
                key.to_lowercase()
            )));
        }
    }

    if config.ecosystems.is_empty() && !config.is_monorepo() {
        config.ecosystems = detect_ecosystems(repo_root);
    }

    validate_paths(repo_root, &config)?;
    if config.is_monorepo() {
        validate_monorepo(&config)?;
    }
    Ok(config)
}

fn validate_paths(repo_root: &Path, config: &Config) -> Result<(), RatchetError> {
    validate_safe_path(repo_root, &config.changelog_path, "changelog_path")?;
    for eco in &config.ecosystems {
        let p = match eco {
            EcosystemConfig::Cargo { path } => path,
            EcosystemConfig::Node { path } => path,
            EcosystemConfig::Python { path } => path,
            EcosystemConfig::Generic { path, .. } => path,
        };
        validate_safe_path(repo_root, p, "ecosystem path")?;
    }
    // Validate package paths
    for pkg in &config.packages {
        validate_relative_path(&pkg.path, &format!("package '{}' path", pkg.name))?;
        if let Some(ref cl) = pkg.changelog_path {
            validate_relative_path(cl, &format!("package '{}' changelog_path", pkg.name))?;
        }
        for eco in &pkg.ecosystems {
            let p = match eco {
                EcosystemConfig::Cargo { path } => path,
                EcosystemConfig::Node { path } => path,
                EcosystemConfig::Python { path } => path,
                EcosystemConfig::Generic { path, .. } => path,
            };
            validate_safe_path(repo_root, p, &format!("package '{}' ecosystem path", pkg.name))?;
        }
    }
    for shared in &config.shared_paths {
        validate_relative_path(&shared.path, "shared_paths path")?;
    }
    Ok(())
}

fn validate_safe_path(repo_root: &Path, path: &Path, label: &str) -> Result<(), RatchetError> {
    validate_relative_path(path, label)?;
    let resolved = repo_root.join(path);
    if let Ok(canonical) = resolved.canonicalize() {
        if !canonical.starts_with(repo_root) {
            return Err(RatchetError::Config(format!(
                "{label} '{}' resolves outside the repository",
                path.display()
            )));
        }
    }
    Ok(())
}

fn validate_relative_path(path: &Path, label: &str) -> Result<(), RatchetError> {
    if path.is_absolute() {
        return Err(RatchetError::Config(format!(
            "{label} must be a relative path, got '{}'",
            path.display()
        )));
    }
    Ok(())
}

fn validate_monorepo(config: &Config) -> Result<(), RatchetError> {
    use std::collections::HashSet;

    let mut names: HashSet<&str> = HashSet::new();
    let mut prefixes: HashSet<&str> = HashSet::new();

    for pkg in &config.packages {
        if !names.insert(pkg.name.as_str()) {
            return Err(RatchetError::Config(format!(
                "duplicate package name: '{}'", pkg.name
            )));
        }
        if !prefixes.insert(pkg.tag_prefix.as_str()) {
            return Err(RatchetError::Config(format!(
                "duplicate tag_prefix: '{}' (used by package '{}')", pkg.tag_prefix, pkg.name
            )));
        }
        if pkg.name.is_empty() {
            return Err(RatchetError::Config("package name must not be empty".into()));
        }
    }

    // Check for overlapping package paths
    let path_prefixes: Vec<(&str, String)> = config.packages.iter()
        .map(|p| (p.name.as_str(), p.path_prefix()))
        .collect();
    for (i, (name_a, prefix_a)) in path_prefixes.iter().enumerate() {
        for (name_b, prefix_b) in path_prefixes[i + 1..].iter() {
            if !prefix_a.is_empty()
                && !prefix_b.is_empty()
                && (prefix_a.starts_with(prefix_b.as_str()) || prefix_b.starts_with(prefix_a.as_str()))
            {
                return Err(RatchetError::Config(format!(
                    "overlapping package paths: '{}' ({name_a}) and '{}' ({name_b})",
                    prefix_a.trim_end_matches('/'), prefix_b.trim_end_matches('/')
                )));
            }
        }
    }

    // Validate shared_paths reference existing package names
    for shared in &config.shared_paths {
        for affected in &shared.affects {
            if !names.contains(affected.as_str()) {
                return Err(RatchetError::Config(format!(
                    "shared_path '{}' references unknown package '{affected}'",
                    shared.path.display()
                )));
            }
        }
    }

    Ok(())
}

fn detect_ecosystems(repo_root: &Path) -> Vec<EcosystemConfig> {
    let mut detected = Vec::new();

    let cargo_path = repo_root.join("Cargo.toml");
    if cargo_path.exists() && has_toml_version(&cargo_path, "package") {
        log::info!("auto-detected ecosystem: cargo (Cargo.toml)");
        detected.push(EcosystemConfig::Cargo {
            path: default_cargo_path(),
        });
    }

    let node_path = repo_root.join("package.json");
    if node_path.exists() && has_json_version(&node_path) {
        log::info!("auto-detected ecosystem: node (package.json)");
        detected.push(EcosystemConfig::Node {
            path: default_node_path(),
        });
    }

    let python_path = repo_root.join("pyproject.toml");
    if python_path.exists() && has_toml_version(&python_path, "project") {
        log::info!("auto-detected ecosystem: python (pyproject.toml)");
        detected.push(EcosystemConfig::Python {
            path: default_python_path(),
        });
    }

    detected
}

fn has_toml_version(path: &Path, table: &str) -> bool {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.parse::<toml_edit::DocumentMut>().ok())
        .and_then(|doc| {
            doc.get(table)
                .and_then(|t| t.get("version"))
                .and_then(|v| v.as_str())
                .map(|_| ())
        })
        .is_some()
}

fn has_json_version(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|json| json["version"].as_str().map(|_| ()))
        .is_some()
}
