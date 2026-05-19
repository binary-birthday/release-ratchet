//! Functional tests: black-box tests of the CLI binary against local git repos.
//!
//! These test release-ratchet as a user would invoke it — through the compiled
//! binary, against real (temporary) git repositories. No external services are
//! contacted. The boundary under test is the CLI interface.

use std::path::Path;

use git2::{Repository, Signature};
use tempfile::TempDir;

// ============================================================================
// Helpers
// ============================================================================

fn init_repo() -> (TempDir, Repository) {
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

fn commit(repo: &Repository, path: &Path, filename: &str, content: &str, message: &str) {
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

fn write_config(path: &Path, content: &str) {
    std::fs::write(path.join(".release-ratchet.yml"), content).unwrap();
}

fn write_cargo_toml(path: &Path, version: &str) {
    std::fs::write(
        path.join("Cargo.toml"),
        format!("[package]\nname = \"test-project\"\nversion = \"{version}\"\nedition = \"2021\"\n"),
    )
    .unwrap();
}

fn write_package_json(path: &Path, version: &str) {
    std::fs::write(
        path.join("package.json"),
        format!("{{\n  \"name\": \"test-project\",\n  \"version\": \"{version}\"\n}}"),
    )
    .unwrap();
}

fn write_pyproject_toml(path: &Path, version: &str) {
    std::fs::write(
        path.join("pyproject.toml"),
        format!("[project]\nname = \"test-project\"\nversion = \"{version}\"\n"),
    )
    .unwrap();
}

fn binary() -> std::process::Command {
    std::process::Command::new(env!("CARGO_BIN_EXE_release-ratchet"))
}

fn run_ok(cmd: &mut std::process::Command) -> (String, String) {
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(output.status.success(), "command failed: {stderr}");
    (stdout, stderr)
}

fn run_fail(cmd: &mut std::process::Command) -> (i32, String, String) {
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(!output.status.success(), "command unexpectedly succeeded: {stderr}");
    (output.status.code().unwrap_or(-1), stdout, stderr)
}

fn setup_with_config(config: &str) -> (TempDir, Repository) {
    let (dir, repo) = init_repo();
    write_config(dir.path(), config);
    commit(
        &repo, dir.path(), ".release-ratchet.yml",
        &std::fs::read_to_string(dir.path().join(".release-ratchet.yml")).unwrap(),
        "chore: add config",
    );
    (dir, repo)
}

fn commit_initial_files(repo: &Repository, _dir: &Path, files: &[&str]) {
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

const MINIMAL_CONFIG: &str = "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n";
const CARGO_CONFIG: &str = "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems:\n  - type: cargo\n    path: \"Cargo.toml\"\n";

// ============================================================================
// Status
// ============================================================================

#[test]
fn status_fresh_repo() {
    let (dir, _) = init_repo();
    let output = binary().args(["--repo", dir.path().to_str().unwrap(), "status"]).output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Last release:    (none)"));
    assert!(stderr.contains("Bump level:      none"));
}

#[test]
fn status_pending_feat_is_minor() {
    let (dir, repo) = init_repo();
    commit(&repo, dir.path(), "a.txt", "x", "feat: add feature A");
    commit(&repo, dir.path(), "b.txt", "y", "fix: fix bug B");
    let output = binary().args(["--repo", dir.path().to_str().unwrap(), "status"]).output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Bump level:      minor"));
    assert!(stderr.contains("Next version:    0.1.0"));
}

#[test]
fn status_json() {
    let (dir, repo) = init_repo();
    commit(&repo, dir.path(), "a.txt", "x", "feat: add feature A");
    let (stdout, _) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "status", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["bump_level"], "minor");
    assert_eq!(json["next_version"], "0.1.0");
    assert_eq!(json["conventional_commits"], 2); // initial + feat
}

#[test]
fn status_breaking_is_major() {
    let (dir, repo) = init_repo();
    commit(&repo, dir.path(), "a.txt", "x", "feat!: rewrite everything");
    let output = binary().args(["--repo", dir.path().to_str().unwrap(), "status"]).output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Bump level:      major"));
    assert!(stderr.contains("Next version:    1.0.0"));
}

#[test]
fn status_after_release_shows_incremental() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: feature one");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));
    commit(&repo, dir.path(), "b.txt", "y", "fix: fix something");
    let (stdout, _) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "status", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["last_version"], "0.1.0");
    assert_eq!(json["bump_level"], "patch");
    assert_eq!(json["next_version"], "0.1.1");
    assert_eq!(json["conventional_commits"], 1);
}

// ============================================================================
// Validate
// ============================================================================

#[test]
fn validate_valid_message() {
    let (dir, _) = init_repo();
    let output = binary()
        .args(["--repo", dir.path().to_str().unwrap(), "validate", "--message", "feat(auth): add OAuth"])
        .output().unwrap();
    assert!(output.status.success());
}

#[test]
fn validate_invalid_message_exits_3() {
    let (dir, _) = init_repo();
    let output = binary()
        .args(["--repo", dir.path().to_str().unwrap(), "validate", "--message", "updated the readme"])
        .output().unwrap();
    assert_eq!(output.status.code(), Some(3));
}

#[test]
fn validate_all_valid_in_range() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: feature A");
    commit(&repo, dir.path(), "b.txt", "y", "fix: bug B");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "validate"]));
}

#[test]
fn validate_invalid_in_range_exits_3() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: feature A");
    commit(&repo, dir.path(), "b.txt", "y", "yolo deploy friday");
    let (code, _, stderr) = run_fail(binary().args(["--repo", dir.path().to_str().unwrap(), "validate"]));
    assert_eq!(code, 3);
    assert!(stderr.contains("NOT valid"));
}

// ============================================================================
// Init
// ============================================================================

#[test]
fn init_creates_config() {
    let (dir, _) = init_repo();
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "init"]));
    assert!(dir.path().join(".release-ratchet.yml").exists());
    // Without --force: fails
    let output = binary().args(["--repo", dir.path().to_str().unwrap(), "init"]).output().unwrap();
    assert!(!output.status.success());
    // With --force: succeeds
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "init", "--force"]));
}

// ============================================================================
// Prepare
// ============================================================================

#[test]
fn prepare_dry_run() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: add feature A");
    commit(&repo, dir.path(), "b.txt", "y", "fix(core): fix bug B");
    let output = binary()
        .args(["--repo", dir.path().to_str().unwrap(), "prepare", "--dry-run"])
        .output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stdout.contains("## [0.1.0]"));
    assert!(stdout.contains("### Features"));
    assert!(stdout.contains("add feature A"));
    assert!(stdout.contains("**core**: fix bug B"));
    assert!(stderr.contains("0.0.0 -> 0.1.0"));
    assert!(!dir.path().join("CHANGELOG.md").exists());
}

#[test]
fn prepare_creates_branch_and_changelog() {
    let (dir, repo) = init_repo();
    write_cargo_toml(dir.path(), "0.0.0");
    write_config(dir.path(), CARGO_CONFIG);
    commit_initial_files(&repo, dir.path(), &["Cargo.toml", ".release-ratchet.yml"]);
    commit(&repo, dir.path(), "feature.rs", "fn main() {}", "feat: add main feature");
    commit(&repo, dir.path(), "fix.rs", "fn fix() {}", "fix: patch a bug");
    let (_, stderr) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare"]));
    assert!(stderr.contains("0.0.0 -> 0.1.0"));
    assert!(stderr.contains("Created release commit"));
    assert_eq!(repo.head().unwrap().shorthand().unwrap(), "release-ratchet--release");
    let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();
    assert!(changelog.contains("## [0.1.0]"));
    assert!(changelog.contains("add main feature"));
    assert!(changelog.contains("patch a bug"));
    let cargo = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
    assert!(cargo.contains("version = \"0.1.0\""));
}

#[test]
fn prepare_no_branch_stays_on_current() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: new thing");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    assert_ne!(repo.head().unwrap().shorthand().unwrap(), "release-ratchet--release");
}

#[test]
fn prepare_bump_override() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "fix: small fix");
    let (_, stderr) = run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "prepare", "--bump", "major", "--no-branch"]));
    assert!(stderr.contains("0.0.0 -> 1.0.0 (major)"));
}

#[test]
fn prepare_version_override() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "fix: small fix");
    let (_, stderr) = run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch", "--release-version", "5.0.0"]));
    assert!(stderr.contains("5.0.0 (manual override)"));
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert!(head.message().unwrap().contains("v5.0.0"));
}

#[test]
fn prepare_nothing_to_release_exits_2() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "chore: update deps");
    commit(&repo, dir.path(), "b.txt", "y", "docs: update readme");
    let output = binary().args(["--repo", dir.path().to_str().unwrap(), "prepare"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn prepare_dirty_tree_rejected() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: add feature");
    // Create tracked CHANGELOG.md, then dirty it
    std::fs::write(dir.path().join("CHANGELOG.md"), "clean").unwrap();
    {
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("CHANGELOG.md")).unwrap();
        index.write().unwrap();
        let sig = Signature::now("Test User", "test@example.com").unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "chore: add changelog", &tree, &[&head]).unwrap();
    }
    std::fs::write(dir.path().join("CHANGELOG.md"), "dirty").unwrap();
    let (code, _, stderr) = run_fail(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    assert_eq!(code, 1);
    assert!(stderr.contains("uncommitted changes"));
}

#[test]
fn prepare_custom_branch_name() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: add feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--branch", "release/v0.1.0"]));
    assert_eq!(repo.head().unwrap().shorthand().unwrap(), "release/v0.1.0");
}

#[test]
fn prepare_rerun_overwrites_branch() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: add feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare"]));
    // Go back to main
    {
        let main_ref = repo.find_reference("refs/heads/main").unwrap();
        let main_obj = main_ref.peel(git2::ObjectType::Commit).unwrap();
        repo.checkout_tree(&main_obj, Some(git2::build::CheckoutBuilder::new().force())).unwrap();
        repo.set_head("refs/heads/main").unwrap();
    }
    commit(&repo, dir.path(), "b.txt", "y", "fix: another fix");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare"]));
    let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();
    assert!(changelog.contains("another fix"));
}

// ============================================================================
// Release
// ============================================================================

#[test]
fn release_dry_run() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: add feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    let (_, stderr) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release", "--dry-run"]));
    assert!(stderr.contains("Would create tag"));
    assert!(repo.refname_to_id("refs/tags/v0.1.0").is_err());
}

#[test]
fn release_prevents_duplicate_tag() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: add feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));
    let (_, _, stderr) = run_fail(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));
    assert!(stderr.contains("already exists"));
}

#[test]
fn release_version_override() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    let (_, stderr) = run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "release", "--release-version", "0.1.0"]));
    assert!(stderr.contains("Created tag 'v0.1.0'"));
}

// ============================================================================
// Full cycles
// ============================================================================

#[test]
fn full_prepare_release_two_cycles() {
    let (dir, repo) = init_repo();
    write_cargo_toml(dir.path(), "0.0.0");
    write_config(dir.path(), CARGO_CONFIG);
    commit_initial_files(&repo, dir.path(), &["Cargo.toml", ".release-ratchet.yml"]);

    // Cycle 1: feat → 0.1.0
    commit(&repo, dir.path(), "feature.rs", "fn feat() {}", "feat: add feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert!(head.message().unwrap().contains("chore: release v0.1.0"));
    let (_, stderr) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));
    assert!(stderr.contains("Created tag 'v0.1.0'"));

    // Cycle 2: fix → 0.1.1
    commit(&repo, dir.path(), "fix.rs", "fn fix() {}", "fix: patch something");
    let (_, stderr) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    assert!(stderr.contains("0.1.0 -> 0.1.1 (patch)"));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));
    assert!(repo.refname_to_id("refs/tags/v0.1.1").is_ok());

    // Changelog ordering
    let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();
    let p1 = changelog.find("## [0.1.1]").unwrap();
    let p2 = changelog.find("## [0.1.0]").unwrap();
    assert!(p1 < p2);
}

#[test]
fn full_three_release_progression() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    // 0.1.0
    commit(&repo, dir.path(), "a.txt", "1", "feat: first feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));
    // 0.2.0
    commit(&repo, dir.path(), "b.txt", "2", "feat: second feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));
    // 1.0.0
    commit(&repo, dir.path(), "c.txt", "3", "feat!: v1 rewrite");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    let (_, stderr) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));
    assert!(stderr.contains("Created tag 'v1.0.0'"));

    let (stdout, _) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "status", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["last_version"], "1.0.0");
    assert_eq!(json["bump_level"], "none");
}

#[test]
fn full_branch_prepare_merge_release() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: add feature");
    let (_, stderr) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare"]));
    assert!(stderr.contains("Release branch"));
    // Simulate merge to main
    {
        let release_commit = repo.head().unwrap().peel_to_commit().unwrap();
        let main_ref = repo.find_reference("refs/heads/main").unwrap();
        let main_obj = main_ref.peel(git2::ObjectType::Commit).unwrap();
        repo.checkout_tree(&main_obj, Some(git2::build::CheckoutBuilder::new().force())).unwrap();
        repo.set_head("refs/heads/main").unwrap();
        let main_commit = repo.head().unwrap().peel_to_commit().unwrap();
        let sig = Signature::now("Test User", "test@example.com").unwrap();
        let mut index = repo.index().unwrap();
        index.read(true).unwrap();
        let release_tree = release_commit.tree().unwrap();
        repo.checkout_tree(release_tree.as_object(), Some(git2::build::CheckoutBuilder::new().force())).unwrap();
        index.read_tree(&release_tree).unwrap();
        index.write().unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "Merge branch 'release-ratchet--release'",
            &tree, &[&main_commit, &release_commit]).unwrap();
    }
    let (_, stderr) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));
    assert!(stderr.contains("Created tag 'v0.1.0'"));
}

#[test]
fn full_custom_tag_prefix() {
    let (dir, repo) = setup_with_config("tag_prefix: \"myapp-v\"\nmain_branch: \"main\"\necosystems: []\n");
    commit(&repo, dir.path(), "a.txt", "x", "feat: feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));
    assert!(repo.refname_to_id("refs/tags/myapp-v0.1.0").is_ok());
    commit(&repo, dir.path(), "b.txt", "y", "fix: fix");
    let (stdout, _) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "status", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["last_version"], "0.1.0");
    assert_eq!(json["next_version"], "0.1.1");
}

// ============================================================================
// Multiple ecosystems
// ============================================================================

#[test]
fn multiple_ecosystem_bumping() {
    let (dir, repo) = init_repo();
    write_cargo_toml(dir.path(), "0.0.0");
    write_package_json(dir.path(), "0.0.0");
    write_pyproject_toml(dir.path(), "0.0.0");
    write_config(dir.path(), "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems:\n  - type: cargo\n    path: \"Cargo.toml\"\n  - type: node\n    path: \"package.json\"\n  - type: python\n    path: \"pyproject.toml\"\n");
    commit_initial_files(&repo, dir.path(), &["Cargo.toml", "package.json", "pyproject.toml", ".release-ratchet.yml"]);
    commit(&repo, dir.path(), "feature.rs", "fn f() {}", "feat: add feature");
    let (_, stderr) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    assert!(stderr.contains("0.0.0 -> 0.1.0"));
    assert!(std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap().contains("version = \"0.1.0\""));
    assert!(std::fs::read_to_string(dir.path().join("package.json")).unwrap().contains("\"0.1.0\""));
    assert!(std::fs::read_to_string(dir.path().join("pyproject.toml")).unwrap().contains("version = \"0.1.0\""));
}

// ============================================================================
// Bump semantics
// ============================================================================

#[test]
fn only_fixes_is_patch() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "fix: fix one");
    commit(&repo, dir.path(), "b.txt", "y", "fix: fix two");
    commit(&repo, dir.path(), "c.txt", "z", "perf: speed up thing");
    let (stdout, _) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "status", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["bump_level"], "patch");
    assert_eq!(json["next_version"], "0.0.1");
}

#[test]
fn breaking_footer_drives_major() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    let msg = "fix: change API response format\n\nBREAKING CHANGE: response is now JSON instead of XML";
    commit(&repo, dir.path(), "a.txt", "x", msg);
    let (stdout, _) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "status", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["bump_level"], "major");
    assert_eq!(json["breaking_changes"], 1);
}

#[test]
fn breaking_bang_on_non_bumping_type_drives_major() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "refactor!: drop Python 2 support");
    let (stdout, _) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "status", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["bump_level"], "major");
}

#[test]
fn mixed_conventional_and_nonconventional() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: add feature");
    commit(&repo, dir.path(), "b.txt", "y", "WIP stuff");
    commit(&repo, dir.path(), "c.txt", "z", "update readme");
    commit(&repo, dir.path(), "d.txt", "w", "fix: important fix");
    let (stdout, _) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "status", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["conventional_commits"], 4); // initial + config + feat + fix
    assert_eq!(json["non_conventional_commits"], 2);
    assert_eq!(json["bump_level"], "minor");
}

// ============================================================================
// Changelog
// ============================================================================

#[test]
fn changelog_accumulates_correctly() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "1", "feat: alpha feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));
    commit(&repo, dir.path(), "b.txt", "2", "fix: beta fix");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();
    assert!(changelog.starts_with("# Changelog"));
    assert!(changelog.contains("## [0.1.0]"));
    assert!(changelog.contains("## [0.1.1]"));
    let v011 = changelog.find("## [0.1.1]").unwrap();
    let v010 = changelog.find("## [0.1.0]").unwrap();
    let alpha = changelog.find("alpha feature").unwrap();
    let beta = changelog.find("beta fix").unwrap();
    assert!(beta > v011 && beta < v010);
    assert!(alpha > v010);
}

// ============================================================================
// Backport
// ============================================================================

#[test]
fn backport_dry_run() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));
    // Add a fix on main
    commit(&repo, dir.path(), "b.txt", "y", "fix: critical bugfix");
    let fix_oid = repo.head().unwrap().peel_to_commit().unwrap().id().to_string();

    let (_, stderr) = run_ok(binary().args([
        "--repo", dir.path().to_str().unwrap(),
        "backport", &fix_oid, "--onto", "v0.1.0", "--dry-run",
    ]));
    assert!(stderr.contains("DRY RUN"));
    assert!(stderr.contains("maintain/v0.1.x"));
    assert!(stderr.contains("critical bugfix"));
    // Should not have created the branch
    assert!(repo.find_branch("maintain/v0.1.x", git2::BranchType::Local).is_err());
}

#[test]
fn backport_onto_tag_creates_maintenance_branch() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));
    // v0.1.0 is tagged. Add a fix on main.
    commit(&repo, dir.path(), "b.txt", "y", "fix: critical bugfix");
    let fix_oid = repo.head().unwrap().peel_to_commit().unwrap().id().to_string();

    run_ok(binary().args([
        "--repo", dir.path().to_str().unwrap(),
        "backport", &fix_oid, "--onto", "v0.1.0",
    ]));

    // Should be on maintain/v0.1.x
    let head = repo.head().unwrap();
    assert_eq!(head.shorthand().unwrap(), "maintain/v0.1.x");

    // The fix commit should be on this branch
    let head_commit = head.peel_to_commit().unwrap();
    assert!(head_commit.message().unwrap().contains("critical bugfix"));
}

#[test]
fn backport_then_prepare_and_release() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    let r = dir.path().to_str().unwrap();

    // Release v0.1.0 on main
    commit(&repo, dir.path(), "a.txt", "x", "feat: feature");
    run_ok(binary().args(["--repo", r, "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", r, "release"]));

    // Add a fix on main (after the release)
    drop(repo);
    let repo = Repository::open(dir.path()).unwrap();
    commit(&repo, dir.path(), "b.txt", "y", "fix: security patch");
    let fix_oid = repo.head().unwrap().peel_to_commit().unwrap().id().to_string();

    // Backport the fix to v0.1.x line
    drop(repo);
    run_ok(binary().args(["--repo", r, "backport", &fix_oid, "--onto", "v0.1.0"]));

    // Prepare and release the backport
    let (_, stderr) = run_ok(binary().args(["--repo", r, "prepare", "--no-branch"]));
    assert!(stderr.contains("0.1.0 -> 0.1.1"), "expected 0.1.0 -> 0.1.1, got: {stderr}");
    run_ok(binary().args(["--repo", r, "release"]));

    // Both tags should exist
    let repo = Repository::open(dir.path()).unwrap();
    assert!(repo.refname_to_id("refs/tags/v0.1.0").is_ok());
    assert!(repo.refname_to_id("refs/tags/v0.1.1").is_ok());
}

#[test]
fn backport_onto_existing_branch() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));

    // First backport creates the branch
    commit(&repo, dir.path(), "b.txt", "y", "fix: fix one");
    let fix1 = repo.head().unwrap().peel_to_commit().unwrap().id().to_string();
    run_ok(binary().args([
        "--repo", dir.path().to_str().unwrap(),
        "backport", &fix1, "--onto", "v0.1.0",
    ]));

    // Go back to main
    {
        let obj = repo.revparse_single("refs/heads/main").unwrap();
        repo.checkout_tree(&obj, Some(git2::build::CheckoutBuilder::new().force())).unwrap();
        repo.set_head("refs/heads/main").unwrap();
    }

    // Second backport onto the same maintenance branch (already exists)
    commit(&repo, dir.path(), "c.txt", "z", "fix: fix two");
    let fix2 = repo.head().unwrap().peel_to_commit().unwrap().id().to_string();
    let (_, stderr) = run_ok(binary().args([
        "--repo", dir.path().to_str().unwrap(),
        "backport", &fix2, "--onto", "maintain/v0.1.x",
    ]));
    assert!(stderr.contains("Checked out existing branch"));
    assert!(stderr.contains("fix two"));
}

#[test]
fn backport_custom_branch_name() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));

    commit(&repo, dir.path(), "b.txt", "y", "fix: bugfix");
    let fix_oid = repo.head().unwrap().peel_to_commit().unwrap().id().to_string();

    run_ok(binary().args([
        "--repo", dir.path().to_str().unwrap(),
        "backport", &fix_oid, "--onto", "v0.1.0", "--branch", "hotfix/v0.1.1",
    ]));

    assert_eq!(repo.head().unwrap().shorthand().unwrap(), "hotfix/v0.1.1");
}

#[test]
fn backport_multiple_commits() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));

    commit(&repo, dir.path(), "b.txt", "y", "fix: fix one");
    let fix1 = repo.head().unwrap().peel_to_commit().unwrap().id().to_string();
    commit(&repo, dir.path(), "c.txt", "z", "fix: fix two");
    let fix2 = repo.head().unwrap().peel_to_commit().unwrap().id().to_string();

    let (_, stderr) = run_ok(binary().args([
        "--repo", dir.path().to_str().unwrap(),
        "backport", &fix1, &fix2, "--onto", "v0.1.0",
    ]));
    assert!(stderr.contains("fix one"));
    assert!(stderr.contains("fix two"));
    assert!(stderr.contains("Backport complete"));
}

// ============================================================================
// Notes
// ============================================================================

#[test]
fn notes_extract_specific_version() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: cool feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));

    let (stdout, _) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "notes", "v0.1.0"]));
    assert!(stdout.contains("## [0.1.0]"));
    assert!(stdout.contains("cool feature"));
}

#[test]
fn notes_extract_without_prefix() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: cool feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));

    let (stdout, _) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "notes", "0.1.0"]));
    assert!(stdout.contains("## [0.1.0]"));
}

#[test]
fn notes_latest() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: first");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));
    commit(&repo, dir.path(), "b.txt", "y", "fix: second");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));

    let (stdout, _) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "notes", "--latest"]));
    assert!(stdout.contains("## [0.1.1]"));
    assert!(stdout.contains("second"));
    assert!(!stdout.contains("first"));
}

#[test]
fn notes_generate_next() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: upcoming feature");

    let (stdout, _) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "notes"]));
    assert!(stdout.contains("## [0.1.0]"));
    assert!(stdout.contains("upcoming feature"));
    // Should NOT have created a changelog file
    assert!(!dir.path().join("CHANGELOG.md").exists());
}

#[test]
fn notes_missing_version_exits_1() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));

    let output = binary()
        .args(["--repo", dir.path().to_str().unwrap(), "notes", "v9.9.9"])
        .output().unwrap();
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn notes_nothing_to_release_exits_2() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "chore: nothing releasable");

    let output = binary()
        .args(["--repo", dir.path().to_str().unwrap(), "notes"])
        .output().unwrap();
    assert_eq!(output.status.code(), Some(2));
}

// ============================================================================
// Pre-release
// ============================================================================

#[test]
fn prerelease_first_alpha() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: new feature");
    let (_, stderr) = run_ok(binary().args([
        "--repo", dir.path().to_str().unwrap(),
        "prepare", "--no-branch", "--prerelease", "alpha",
    ]));
    assert!(stderr.contains("0.1.0-alpha.1"), "expected alpha.1: {stderr}");

    // Verify the commit message
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert!(head.message().unwrap().contains("v0.1.0-alpha.1"));
}

#[test]
fn prerelease_increment() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: new feature");

    // First alpha
    run_ok(binary().args([
        "--repo", dir.path().to_str().unwrap(),
        "prepare", "--no-branch", "--prerelease", "alpha",
    ]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));

    // Second alpha (add another commit)
    drop(repo);
    let repo = Repository::open(dir.path()).unwrap();
    commit(&repo, dir.path(), "b.txt", "y", "fix: fix something");
    let (_, stderr) = run_ok(binary().args([
        "--repo", dir.path().to_str().unwrap(),
        "prepare", "--no-branch", "--prerelease", "alpha",
    ]));
    assert!(stderr.contains("0.1.0-alpha.2"), "expected alpha.2: {stderr}");
}

#[test]
fn prerelease_switch_id_resets() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: new feature");

    // alpha.1
    run_ok(binary().args([
        "--repo", dir.path().to_str().unwrap(),
        "prepare", "--no-branch", "--prerelease", "alpha",
    ]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));

    // Switch to beta → beta.1 (not beta.2)
    drop(repo);
    let repo = Repository::open(dir.path()).unwrap();
    commit(&repo, dir.path(), "b.txt", "y", "fix: fix");
    let (_, stderr) = run_ok(binary().args([
        "--repo", dir.path().to_str().unwrap(),
        "prepare", "--no-branch", "--prerelease", "beta",
    ]));
    assert!(stderr.contains("0.1.0-beta.1"), "expected beta.1: {stderr}");
}

#[test]
fn prerelease_stable_promotion() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: new feature");

    // Create pre-release
    run_ok(binary().args([
        "--repo", dir.path().to_str().unwrap(),
        "prepare", "--no-branch", "--prerelease", "rc",
    ]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));

    // Promote to stable (no --prerelease flag)
    drop(repo);
    let repo = Repository::open(dir.path()).unwrap();
    commit(&repo, dir.path(), "b.txt", "y", "fix: last fix before stable");
    let (_, stderr) = run_ok(binary().args([
        "--repo", dir.path().to_str().unwrap(),
        "prepare", "--no-branch",
    ]));
    assert!(stderr.contains("-> 0.1.0 (stable promotion)"), "expected stable promotion: {stderr}");

    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));

    // Both tags should exist
    let repo = Repository::open(dir.path()).unwrap();
    assert!(repo.refname_to_id("refs/tags/v0.1.0-rc.1").is_ok());
    assert!(repo.refname_to_id("refs/tags/v0.1.0").is_ok());
}

#[test]
fn prerelease_full_cycle() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat!: breaking change");

    // alpha.1, alpha.2, beta.1, rc.1, stable
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch", "--prerelease", "alpha"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));

    drop(repo);
    let repo = Repository::open(dir.path()).unwrap();
    commit(&repo, dir.path(), "b.txt", "y", "fix: alpha fix");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch", "--prerelease", "alpha"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));

    drop(repo);
    let repo = Repository::open(dir.path()).unwrap();
    commit(&repo, dir.path(), "c.txt", "z", "fix: beta fix");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch", "--prerelease", "beta"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));

    drop(repo);
    let repo = Repository::open(dir.path()).unwrap();
    commit(&repo, dir.path(), "d.txt", "w", "fix: rc fix");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch", "--prerelease", "rc"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));

    // Promote to stable
    drop(repo);
    let repo = Repository::open(dir.path()).unwrap();
    commit(&repo, dir.path(), "e.txt", "v", "fix: final fix");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));

    // All tags should exist
    let repo = Repository::open(dir.path()).unwrap();
    assert!(repo.refname_to_id("refs/tags/v1.0.0-alpha.1").is_ok());
    assert!(repo.refname_to_id("refs/tags/v1.0.0-alpha.2").is_ok());
    assert!(repo.refname_to_id("refs/tags/v1.0.0-beta.1").is_ok());
    assert!(repo.refname_to_id("refs/tags/v1.0.0-rc.1").is_ok());
    assert!(repo.refname_to_id("refs/tags/v1.0.0").is_ok());
}

// ============================================================================
// Ecosystem auto-detection
// ============================================================================

#[test]
fn autodetect_cargo_and_node() {
    let (dir, repo) = init_repo();
    // Config with no ecosystems listed
    write_config(dir.path(), "tag_prefix: \"v\"\nmain_branch: \"main\"\n");
    write_cargo_toml(dir.path(), "0.0.0");
    write_package_json(dir.path(), "0.0.0");
    commit_initial_files(&repo, dir.path(), &[".release-ratchet.yml", "Cargo.toml", "package.json"]);
    commit(&repo, dir.path(), "a.txt", "x", "feat: add feature");

    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));

    let cargo = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
    assert!(cargo.contains("version = \"0.1.0\""), "Cargo.toml not bumped: {cargo}");
    let pkg = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
    assert!(pkg.contains("\"0.1.0\""), "package.json not bumped: {pkg}");
}

#[test]
fn autodetect_no_config_file() {
    let (dir, repo) = init_repo();
    // No .release-ratchet.yml at all
    write_cargo_toml(dir.path(), "0.0.0");
    commit_initial_files(&repo, dir.path(), &["Cargo.toml"]);
    commit(&repo, dir.path(), "a.txt", "x", "feat: add feature");

    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));

    let cargo = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
    assert!(cargo.contains("version = \"0.1.0\""), "Cargo.toml not bumped: {cargo}");
}

#[test]
fn autodetect_skipped_when_ecosystems_configured() {
    let (dir, repo) = init_repo();
    // Config explicitly lists only node
    write_config(dir.path(), "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems:\n  - type: node\n    path: \"package.json\"\n");
    write_cargo_toml(dir.path(), "0.0.0");
    write_package_json(dir.path(), "0.0.0");
    commit_initial_files(&repo, dir.path(), &[".release-ratchet.yml", "Cargo.toml", "package.json"]);
    commit(&repo, dir.path(), "a.txt", "x", "feat: add feature");

    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));

    // Node should be bumped (explicitly configured)
    let pkg = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
    assert!(pkg.contains("\"0.1.0\""), "package.json not bumped");
    // Cargo should NOT be bumped (not in explicit config, auto-detect skipped)
    let cargo = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
    assert!(cargo.contains("version = \"0.0.0\""), "Cargo.toml should not be bumped: {cargo}");
}
