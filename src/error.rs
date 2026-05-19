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

/// Exit codes that commands can return to signal non-error conditions.
/// These are distinct from errors -- they indicate "nothing to do" or
/// "validation failed" rather than unexpected failures.
#[derive(Debug)]
pub enum ExitCode {
    /// Nothing to release (no releasable commits found). Exit code 2.
    NothingToRelease,
    /// Validation failed (invalid commit messages). Exit code 3.
    ValidationFailed,
}

impl std::fmt::Display for ExitCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NothingToRelease => write!(f, "nothing to release"),
            Self::ValidationFailed => write!(f, "validation failed"),
        }
    }
}

impl std::error::Error for ExitCode {}

impl ExitCode {
    pub fn code(&self) -> i32 {
        match self {
            Self::NothingToRelease => 2,
            Self::ValidationFailed => 3,
        }
    }
}
