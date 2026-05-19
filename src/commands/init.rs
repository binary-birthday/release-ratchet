use std::path::Path;

use anyhow::{Result, bail};

use crate::cli::InitArgs;

const DEFAULT_CONFIG: &str = r#"# release-ratchet configuration
# See: https://github.com/your-org/release-ratchet

# Tag prefix prepended to version numbers (e.g., "v" -> "v1.2.3")
tag_prefix: "v"

# The primary branch
main_branch: "main"

# The branch name created during `prepare`
release_branch: "release-ratchet--release"

# Path to the changelog file
changelog_path: "CHANGELOG.md"

# Ecosystems to update version numbers in
ecosystems:
  - type: cargo
    path: "Cargo.toml"
  # - type: node
  #   path: "package.json"
  # - type: python
  #   path: "pyproject.toml"
  # - type: generic
  #   path: "version.txt"
  #   pattern: 'version\s*=\s*"(\d+\.\d+\.\d+)"'

# Optional: override built-in commit type behavior
# commit_type_overrides:
#   refactor:
#     bump: patch
#     changelog: "Refactoring"

# GPG sign release tags (creates annotated tags)
sign_tags: false
"#;

pub fn execute(repo_path: &Path, args: InitArgs) -> Result<()> {
    let config_path = repo_path.join(".release-ratchet.yml");

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
