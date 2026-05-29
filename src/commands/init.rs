use std::path::Path;

use anyhow::{Result, bail};

use crate::cli::InitArgs;

const DEFAULT_CONFIG: &str = r#"# release-ratchet configuration

# Tag prefix prepended to version numbers (e.g., "v" -> "v1.2.3")
tag_prefix = "v"

# The primary branch
main_branch = "main"

# The branch name created during `prepare`
release_branch = "chore/next-release"

# Path to the changelog file
changelog_path = "CHANGELOG.md"

# Git forge (affects how merge commit messages are parsed)
# Bitbucket Cloud squash merges put the PR title in the body, not the subject.
# forge = "bitbucket-cloud"

# Ecosystems to update version numbers in
[[ecosystems]]
type = "cargo"
path = "Cargo.toml"

# [[ecosystems]]
# type = "node"
# path = "package.json"

# [[ecosystems]]
# type = "python"
# path = "pyproject.toml"

# [[ecosystems]]
# type = "generic"
# path = "version.txt"
# pattern = 'version\s*=\s*"(\d+\.\d+\.\d+)"'

# Optional: override built-in commit type behavior
# [commit_type_overrides.refactor]
# bump = "patch"
# changelog = "Refactoring"

# GPG sign release tags (creates annotated tags)
sign_tags = false

# Delete the release branch after tagging
cleanup_branch = false

# Lifecycle hooks
# [hooks]
# post_prepare = ["cargo check"]
# post_release = ["cargo publish"]
"#;

pub fn execute(repo_path: &Path, args: InitArgs) -> Result<()> {
    let config_path = repo_path.join(".release-ratchet.toml");

    if config_path.exists() && !args.force {
        bail!(
            "Config file already exists at {}. Use --force to overwrite.",
            config_path.display()
        );
    }

    std::fs::write(&config_path, DEFAULT_CONFIG)?;
    eprintln!("Created {}", config_path.display());

    Ok(())
}
