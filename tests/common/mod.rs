use std::path::Path;

use git2::{Repository, Signature};
use tempfile::TempDir;

pub fn init_repo() -> (TempDir, Repository) {
    let dir = TempDir::new().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    let mut config = repo.config().unwrap();
    config.set_str("user.name", "Test User").unwrap();
    config.set_str("user.email", "test@example.com").unwrap();
    {
        let sig = Signature::now("Test User", "test@example.com").unwrap();
        let tree_oid = {
            let mut index = repo.index().unwrap();
            index.write_tree().unwrap()
        };
        let tree = repo.find_tree(tree_oid).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "chore: initial commit", &tree, &[])
            .unwrap();
    }
    {
        let head_ref = repo.head().unwrap();
        let head_name = head_ref.name().unwrap().to_string();
        drop(head_ref);
        if head_name != "refs/heads/main" {
            let mut reference = repo.find_reference(&head_name).unwrap();
            reference.rename("refs/heads/main", true, "rename to main").unwrap();
        }
    }
    (dir, repo)
}

pub fn commit(repo: &Repository, path: &Path, filename: &str, content: &str, message: &str) {
    std::fs::write(path.join(filename), content).unwrap();
    let mut index = repo.index().unwrap();
    index.read(true).unwrap();
    index.add_path(Path::new(filename)).unwrap();
    index.write().unwrap();
    let sig = Signature::now("Test User", "test@example.com").unwrap();
    let tree_oid = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_oid).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[&head]).unwrap();
}

pub fn binary() -> std::process::Command {
    std::process::Command::new(env!("CARGO_BIN_EXE_release-ratchet"))
}

pub fn run_ok(cmd: &mut std::process::Command) -> (String, String) {
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(output.status.success(), "command failed: {stderr}");
    (stdout, stderr)
}

pub fn run_fail(cmd: &mut std::process::Command) -> (i32, String, String) {
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(!output.status.success(), "command unexpectedly succeeded: {stderr}");
    (output.status.code().unwrap_or(-1), stdout, stderr)
}

pub fn setup_with_config(config: &str) -> (TempDir, Repository) {
    let (dir, repo) = init_repo();
    std::fs::write(dir.path().join(".release-ratchet.yml"), config).unwrap();
    commit(
        &repo, dir.path(), ".release-ratchet.yml",
        &std::fs::read_to_string(dir.path().join(".release-ratchet.yml")).unwrap(),
        "chore: add config",
    );
    (dir, repo)
}

pub const MINIMAL_CONFIG: &str = "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n";
pub const CARGO_CONFIG: &str = "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems:\n  - type: cargo\n    path: \"Cargo.toml\"\n";

pub fn write_cargo_toml(path: &Path, version: &str) {
    std::fs::write(
        path.join("Cargo.toml"),
        format!("[package]\nname = \"test-project\"\nversion = \"{version}\"\nedition = \"2021\"\n"),
    ).unwrap();
}

pub fn write_package_json(path: &Path, version: &str) {
    std::fs::write(
        path.join("package.json"),
        format!("{{\n  \"name\": \"test-project\",\n  \"version\": \"{version}\"\n}}"),
    ).unwrap();
}

pub fn write_pyproject_toml(path: &Path, version: &str) {
    std::fs::write(
        path.join("pyproject.toml"),
        format!("[project]\nname = \"test-project\"\nversion = \"{version}\"\n"),
    ).unwrap();
}

pub fn commit_initial_files(repo: &Repository, _dir: &Path, files: &[&str]) {
    let mut index = repo.index().unwrap();
    for f in files {
        index.add_path(Path::new(f)).unwrap();
    }
    index.write().unwrap();
    let tree_oid = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_oid).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    let sig = Signature::now("Test User", "test@example.com").unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "chore: setup project", &tree, &[&head]).unwrap();
}
