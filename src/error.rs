use thiserror::Error;

#[derive(Error, Debug)]
pub enum RatchetError {
    #[error("git error: {0}")]
    Git(#[from] git2::Error),

    #[error("config error: {0}")]
    Config(String),

    #[error("version file error for {path}: {reason}")]
    VersionFile { path: String, reason: String },

    #[error("changelog error: {0}")]
    Changelog(String),

    #[error("invalid semver: {0}")]
    Semver(#[from] semver::Error),

    #[error("tag '{tag}' already exists")]
    TagAlreadyExists { tag: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}
