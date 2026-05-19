use clap::{Parser, Subcommand, Args};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "release-ratchet")]
#[command(version, about = "Git-vendor-agnostic semantic release tool using conventional commits")]
#[command(propagate_version = true)]
pub struct Cli {
    /// Path to the git repository root.
    #[arg(long, short = 'C', global = true, default_value = ".")]
    pub repo: PathBuf,

    /// Path to config file.
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,

    /// Increase verbosity (-v, -vv, -vvv).
    #[arg(long, short, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Suppress all output except errors.
    #[arg(long, global = true, default_value_t = false)]
    pub quiet: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Analyze commits and prepare a release (changelog, version bump, release branch).
    Prepare(PrepareArgs),

    /// After the release branch is merged, tag the merge commit.
    Release(ReleaseArgs),

    /// Show current release status: last tag, pending commits, next version.
    Status(StatusArgs),

    /// Validate commit messages follow conventional commits format.
    Validate(ValidateArgs),

    /// Cherry-pick commits onto a maintenance branch for backport releases.
    Backport(BackportArgs),

    /// Initialize a .release-ratchet.yml config file with defaults.
    Init(InitArgs),
}


#[derive(Args, Debug)]
pub struct PrepareArgs {
    /// Override the computed bump level.
    #[arg(long, value_enum)]
    pub bump: Option<BumpOverride>,

    /// Override the computed next version (e.g., "2.0.0").
    #[arg(long = "release-version")]
    pub release_version: Option<String>,

    /// Print what would happen without making changes.
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,

    /// Apply changes to current branch instead of creating a release branch.
    #[arg(long, default_value_t = false)]
    pub no_branch: bool,

    /// Custom release branch name (overrides config).
    #[arg(long)]
    pub branch: Option<String>,
}

#[derive(clap::ValueEnum, Debug, Clone, Copy)]
pub enum BumpOverride {
    Major,
    Minor,
    Patch,
}

#[derive(Args, Debug)]
pub struct ReleaseArgs {
    /// The commit (SHA or ref) to tag. Defaults to HEAD of the main branch.
    #[arg(long)]
    pub commit: Option<String>,

    /// Print what would happen without making changes.
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,

    /// Override the version to tag.
    #[arg(long = "release-version")]
    pub release_version: Option<String>,
}

#[derive(Args, Debug)]
pub struct StatusArgs {
    /// Output in JSON format.
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct ValidateArgs {
    /// Validate commits in a range (e.g., "HEAD~5..HEAD").
    #[arg(long)]
    pub range: Option<String>,

    /// Validate a single message string directly.
    #[arg(long)]
    pub message: Option<String>,
}

#[derive(Args, Debug)]
pub struct BackportArgs {
    /// Commit(s) to cherry-pick (SHAs or refs).
    #[arg(required = true)]
    pub commits: Vec<String>,

    /// Tag or branch to backport onto (e.g., "v1.2.0" or "maintain/v1.x").
    /// If a tag is given, a maintenance branch is created from it.
    #[arg(long)]
    pub onto: String,

    /// Custom maintenance branch name. By default, derived from the tag
    /// (e.g., v1.2.0 → maintain/v1.2.x).
    #[arg(long)]
    pub branch: Option<String>,

    /// Print what would happen without making changes.
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
}

#[derive(Args, Debug)]
pub struct InitArgs {
    /// Overwrite existing config file if present.
    #[arg(long, default_value_t = false)]
    pub force: bool,
}
