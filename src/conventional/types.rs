use std::str::FromStr;

use crate::semver_bump::BumpLevel;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CommitType {
    Feat,
    Fix,
    Docs,
    Style,
    Refactor,
    Perf,
    Test,
    Build,
    Ci,
    Chore,
    Revert,
    Custom(String),
}

impl FromStr for CommitType {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_lowercase().as_str() {
            "feat" => Self::Feat,
            "fix" => Self::Fix,
            "docs" => Self::Docs,
            "style" => Self::Style,
            "refactor" => Self::Refactor,
            "perf" => Self::Perf,
            "test" => Self::Test,
            "build" => Self::Build,
            "ci" => Self::Ci,
            "chore" => Self::Chore,
            "revert" => Self::Revert,
            other => Self::Custom(other.to_string()),
        })
    }
}

impl CommitType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Feat => "feat",
            Self::Fix => "fix",
            Self::Docs => "docs",
            Self::Style => "style",
            Self::Refactor => "refactor",
            Self::Perf => "perf",
            Self::Test => "test",
            Self::Build => "build",
            Self::Ci => "ci",
            Self::Chore => "chore",
            Self::Revert => "revert",
            Self::Custom(s) => s.as_str(),
        }
    }

    pub fn default_bump(&self) -> BumpLevel {
        match self {
            Self::Feat => BumpLevel::Minor,
            Self::Fix => BumpLevel::Patch,
            Self::Perf => BumpLevel::Patch,
            Self::Revert => BumpLevel::Patch,
            Self::Docs => BumpLevel::None,
            Self::Style => BumpLevel::None,
            Self::Refactor => BumpLevel::None,
            Self::Test => BumpLevel::None,
            Self::Build => BumpLevel::None,
            Self::Ci => BumpLevel::None,
            Self::Chore => BumpLevel::None,
            Self::Custom(_) => BumpLevel::None,
        }
    }

    pub fn default_changelog_heading(&self) -> Option<&'static str> {
        match self {
            Self::Feat => Some("Features"),
            Self::Fix => Some("Bug Fixes"),
            Self::Perf => Some("Performance"),
            Self::Revert => Some("Reverts"),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommitFooter {
    pub token: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct ConventionalCommit {
    pub oid: git2::Oid,
    pub commit_type: CommitType,
    pub scope: Option<String>,
    pub breaking: bool,
    pub description: String,
    pub body: Option<String>,
    pub footers: Vec<CommitFooter>,
    #[allow(dead_code)]
    pub raw_message: String,
    #[allow(dead_code)]
    pub author: String,
}

impl ConventionalCommit {
    pub fn is_breaking(&self) -> bool {
        self.breaking
            || self.footers.iter().any(|f| {
                let upper = f.token.to_uppercase();
                upper == "BREAKING CHANGE" || upper == "BREAKING-CHANGE"
            })
    }

    pub fn short_oid(&self) -> String {
        let hex = self.oid.to_string();
        hex.get(..7).unwrap_or(&hex).to_string()
    }
}
