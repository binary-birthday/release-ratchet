use std::path::Path;
use std::process::Command;

/// Run lifecycle hooks. Returns the number of failed hooks.
pub fn run_hooks(commands: &[String], repo_root: &Path, version: &str) -> usize {
    let mut failures = 0;
    for cmd in commands {
        log::info!("running hook: {cmd}");
        let result = Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .current_dir(repo_root)
            .env("RELEASE_VERSION", version)
            .status();
        match result {
            Ok(status) if status.success() => {
                log::info!("hook succeeded: {cmd}");
            }
            Ok(status) => {
                eprintln!("warning: hook '{cmd}' exited with {status}");
                failures += 1;
            }
            Err(e) => {
                eprintln!("warning: failed to run hook '{cmd}': {e}");
                failures += 1;
            }
        }
    }
    failures
}
