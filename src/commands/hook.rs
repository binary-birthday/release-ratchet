use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::cli::HookAction;

const HOOK_CONTENT: &str = r#"#!/bin/sh
# Installed by release-ratchet
exec release-ratchet validate --message "$(cat "$1")"
"#;

const HOOK_MARKER: &str = "release-ratchet";

pub fn execute(repo_path: &Path, action: HookAction) -> Result<()> {
    let git_dir = repo_path.join(".git");
    let hooks_dir = git_dir.join("hooks");
    let hook_path = hooks_dir.join("commit-msg");

    match action {
        HookAction::Install { force } => {
            if hook_path.exists() && !force {
                bail!(
                    "commit-msg hook already exists at {}. Use --force to overwrite.",
                    hook_path.display()
                );
            }
            std::fs::create_dir_all(&hooks_dir)
                .context("failed to create hooks directory")?;
            std::fs::write(&hook_path, HOOK_CONTENT)
                .context("failed to write commit-msg hook")?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perms = std::fs::Permissions::from_mode(0o755);
                std::fs::set_permissions(&hook_path, perms)
                    .context("failed to set hook permissions")?;
            }
            eprintln!("Installed commit-msg hook at {}", hook_path.display());
        }
        HookAction::Uninstall => {
            if !hook_path.exists() {
                eprintln!("No commit-msg hook found.");
                return Ok(());
            }
            let content = std::fs::read_to_string(&hook_path)
                .context("failed to read commit-msg hook")?;
            if !content.contains(HOOK_MARKER) {
                bail!(
                    "commit-msg hook exists but was not installed by release-ratchet. \
                     Remove it manually if intended."
                );
            }
            std::fs::remove_file(&hook_path)
                .context("failed to remove commit-msg hook")?;
            eprintln!("Removed commit-msg hook.");
        }
    }

    Ok(())
}
