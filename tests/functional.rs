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
    std::fs::write(path.join(".release-ratchet.toml"), content).unwrap();
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
        &repo, dir.path(), ".release-ratchet.toml",
        &std::fs::read_to_string(dir.path().join(".release-ratchet.toml")).unwrap(),
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

const MINIMAL_CONFIG: &str = "tag_prefix = \"v\"\nmain_branch = \"main\"\n";
const CARGO_CONFIG: &str = "tag_prefix = \"v\"\nmain_branch = \"main\"\n\n[[ecosystems]]\ntype = \"cargo\"\npath = \"Cargo.toml\"\n";

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
    assert!(dir.path().join(".release-ratchet.toml").exists());
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
    commit_initial_files(&repo, dir.path(), &["Cargo.toml", ".release-ratchet.toml"]);
    commit(&repo, dir.path(), "feature.rs", "fn main() {}", "feat: add main feature");
    commit(&repo, dir.path(), "fix.rs", "fn fix() {}", "fix: patch a bug");
    let (_, stderr) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare"]));
    assert!(stderr.contains("0.0.0 -> 0.1.0"));
    assert!(stderr.contains("Created release commit"));
    assert_eq!(repo.head().unwrap().shorthand().unwrap(), "chore/next-release");
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
    assert_ne!(repo.head().unwrap().shorthand().unwrap(), "chore/next-release");
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
fn release_duplicate_tag_succeeds_silently() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: add feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));
    // Second release should succeed (already released, nothing to do)
    let (_, stderr) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));
    assert!(stderr.contains("already exists") || stderr.contains("Already released"));
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
    commit_initial_files(&repo, dir.path(), &["Cargo.toml", ".release-ratchet.toml"]);

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
        repo.commit(Some("HEAD"), &sig, &sig, "Merge branch 'chore/next-release'",
            &tree, &[&main_commit, &release_commit]).unwrap();
    }
    let (_, stderr) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));
    assert!(stderr.contains("Created tag 'v0.1.0'"));
}

#[test]
fn full_custom_tag_prefix() {
    let (dir, repo) = setup_with_config("tag_prefix = \"myapp-v\"\nmain_branch = \"main\"\n");
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
    write_config(dir.path(), "tag_prefix = \"v\"\nmain_branch = \"main\"\n\n[[ecosystems]]\ntype = \"cargo\"\npath = \"Cargo.toml\"\n\n[[ecosystems]]\ntype = \"node\"\npath = \"package.json\"\n\n[[ecosystems]]\ntype = \"python\"\npath = \"pyproject.toml\"\n");
    commit_initial_files(&repo, dir.path(), &["Cargo.toml", "package.json", "pyproject.toml", ".release-ratchet.toml"]);
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
    write_config(dir.path(), "tag_prefix = \"v\"\nmain_branch = \"main\"\n");
    write_cargo_toml(dir.path(), "0.0.0");
    write_package_json(dir.path(), "0.0.0");
    commit_initial_files(&repo, dir.path(), &[".release-ratchet.toml", "Cargo.toml", "package.json"]);
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
    // No .release-ratchet.toml at all
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
    write_config(dir.path(), "tag_prefix = \"v\"\nmain_branch = \"main\"\n\n[[ecosystems]]\ntype = \"node\"\npath = \"package.json\"\n");
    write_cargo_toml(dir.path(), "0.0.0");
    write_package_json(dir.path(), "0.0.0");
    commit_initial_files(&repo, dir.path(), &[".release-ratchet.toml", "Cargo.toml", "package.json"]);
    commit(&repo, dir.path(), "a.txt", "x", "feat: add feature");

    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));

    // Node should be bumped (explicitly configured)
    let pkg = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
    assert!(pkg.contains("\"0.1.0\""), "package.json not bumped");
    // Cargo should NOT be bumped (not in explicit config, auto-detect skipped)
    let cargo = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
    assert!(cargo.contains("version = \"0.0.0\""), "Cargo.toml should not be bumped: {cargo}");
}

// ============================================================================
// Bump command
// ============================================================================

#[test]
fn bump_auto_from_commits() {
    let (dir, repo) = init_repo();
    write_cargo_toml(dir.path(), "0.0.0");
    write_config(dir.path(), CARGO_CONFIG);
    commit_initial_files(&repo, dir.path(), &["Cargo.toml", ".release-ratchet.toml"]);
    commit(&repo, dir.path(), "a.txt", "x", "feat: feature");

    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "bump"]));

    let cargo = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
    assert!(cargo.contains("version = \"0.1.0\""), "not bumped: {cargo}");
}

#[test]
fn bump_with_override() {
    let (dir, repo) = init_repo();
    write_cargo_toml(dir.path(), "0.0.0");
    write_config(dir.path(), CARGO_CONFIG);
    commit_initial_files(&repo, dir.path(), &["Cargo.toml", ".release-ratchet.toml"]);
    commit(&repo, dir.path(), "a.txt", "x", "fix: fix");

    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "bump", "--bump", "major"]));

    let cargo = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
    assert!(cargo.contains("version = \"1.0.0\""), "not bumped to major: {cargo}");
}

#[test]
fn bump_release_version() {
    let (dir, repo) = init_repo();
    write_cargo_toml(dir.path(), "0.0.0");
    write_config(dir.path(), CARGO_CONFIG);
    commit_initial_files(&repo, dir.path(), &["Cargo.toml", ".release-ratchet.toml"]);

    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "bump", "--release-version", "3.0.0"]));

    let cargo = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
    assert!(cargo.contains("version = \"3.0.0\""), "not bumped: {cargo}");
}

#[test]
fn bump_dry_run_no_changes() {
    let (dir, repo) = init_repo();
    write_cargo_toml(dir.path(), "0.0.0");
    write_config(dir.path(), CARGO_CONFIG);
    commit_initial_files(&repo, dir.path(), &["Cargo.toml", ".release-ratchet.toml"]);
    commit(&repo, dir.path(), "a.txt", "x", "feat: feature");

    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "bump", "--dry-run"]));

    let cargo = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
    assert!(cargo.contains("version = \"0.0.0\""), "should not modify in dry-run: {cargo}");
}

#[test]
fn bump_nothing_to_release_exits_2() {
    let (dir, repo) = setup_with_config(CARGO_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "chore: nothing");

    let output = binary().args(["--repo", dir.path().to_str().unwrap(), "bump"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
}

// ============================================================================
// Check command
// ============================================================================

#[test]
fn check_consistent_state_passes() {
    let (dir, repo) = init_repo();
    write_cargo_toml(dir.path(), "0.0.0");
    write_config(dir.path(), CARGO_CONFIG);
    commit_initial_files(&repo, dir.path(), &["Cargo.toml", ".release-ratchet.toml"]);
    commit(&repo, dir.path(), "a.txt", "x", "feat: feature");

    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));

    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "check"]));
}

#[test]
fn check_version_drift_fails() {
    let (dir, repo) = init_repo();
    write_cargo_toml(dir.path(), "0.0.0");
    write_package_json(dir.path(), "0.0.0");
    write_config(dir.path(), "tag_prefix = \"v\"\nmain_branch = \"main\"\n\n[[ecosystems]]\ntype = \"cargo\"\npath = \"Cargo.toml\"\n\n[[ecosystems]]\ntype = \"node\"\npath = \"package.json\"\n");
    commit_initial_files(&repo, dir.path(), &["Cargo.toml", "package.json", ".release-ratchet.toml"]);
    commit(&repo, dir.path(), "a.txt", "x", "feat: feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));

    // Manually desync package.json
    write_package_json(dir.path(), "0.0.0");

    let output = binary().args(["--repo", dir.path().to_str().unwrap(), "check"]).output().unwrap();
    assert!(!output.status.success());
}

#[test]
fn check_json_output() {
    let (dir, repo) = init_repo();
    write_cargo_toml(dir.path(), "0.0.0");
    write_config(dir.path(), CARGO_CONFIG);
    commit_initial_files(&repo, dir.path(), &["Cargo.toml", ".release-ratchet.toml"]);
    commit(&repo, dir.path(), "a.txt", "x", "feat: feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));

    let (stdout, _) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "check", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["consistent"], true);
    assert!(json.get("tag_version").is_some());
    assert!(json.get("file_versions").is_some());
    assert!(json.get("errors").is_some());
}

#[test]
fn check_no_tag_passes() {
    let (dir, _repo) = init_repo();
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "check"]));
}

// ============================================================================
// Hook command
// ============================================================================

#[test]
fn hook_install_and_uninstall() {
    let (dir, _repo) = init_repo();

    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "hook", "install"]));

    let hook_path = dir.path().join(".git/hooks/commit-msg");
    assert!(hook_path.exists(), "hook should be installed");
    let content = std::fs::read_to_string(&hook_path).unwrap();
    assert!(content.contains("release-ratchet"));

    // Uninstall
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "hook", "uninstall"]));
    assert!(!hook_path.exists(), "hook should be removed");
}

#[test]
fn hook_install_refuses_overwrite() {
    let (dir, _repo) = init_repo();

    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "hook", "install"]));

    // Second install without --force should fail
    let output = binary().args(["--repo", dir.path().to_str().unwrap(), "hook", "install"]).output().unwrap();
    assert!(!output.status.success());
}

#[test]
fn hook_install_force_overwrites() {
    let (dir, _repo) = init_repo();

    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "hook", "install"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "hook", "install", "--force"]));
}

#[test]
fn hook_uninstall_refuses_foreign_hook() {
    let (dir, _repo) = init_repo();

    // Write a non-release-ratchet hook
    let hooks_dir = dir.path().join(".git/hooks");
    std::fs::create_dir_all(&hooks_dir).unwrap();
    std::fs::write(hooks_dir.join("commit-msg"), "#!/bin/sh\necho custom hook\n").unwrap();

    let output = binary().args(["--repo", dir.path().to_str().unwrap(), "hook", "uninstall"]).output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not installed by release-ratchet"));
}

// ============================================================================
// Completions command
// ============================================================================

#[test]
fn completions_bash() {
    let output = binary().args(["completions", "bash"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("release-ratchet"), "should contain the binary name");
}

#[test]
fn completions_zsh() {
    let output = binary().args(["completions", "zsh"]).output().unwrap();
    assert!(output.status.success());
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn edge_prepare_twice_blocked() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    // Second prepare should refuse
    let (_, _, stderr) = run_fail(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    assert!(stderr.contains("already a release commit"));
}

#[test]
fn edge_release_without_prepare() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: feature");
    let (_, _, stderr) = run_fail(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));
    assert!(stderr.contains("Could not detect version"));
}

#[test]
fn edge_only_nonconventional_commits() {
    let (dir, repo) = init_repo();
    commit(&repo, dir.path(), "a.txt", "x", "WIP stuff");
    commit(&repo, dir.path(), "b.txt", "y", "update things");
    let output = binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn edge_unicode_in_commits() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: add émoji support 🎉");
    commit(&repo, dir.path(), "b.txt", "y", "fix(i18n): handle CJK characters 中文日本語");
    let (stdout, _) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--dry-run"]));
    assert!(stdout.contains("émoji support 🎉"));
    assert!(stdout.contains("中文日本語"));
}

#[test]
fn edge_empty_description_not_conventional() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    // "feat: " with trailing space and nothing else — no description
    commit(&repo, dir.path(), "a.txt", "x", "feat: ");
    let (stdout, _) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "status", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    // Should be treated as non-conventional or no-bump (chore: init is the only conventional)
    assert_eq!(json["bump_level"], "none");
}

#[test]
fn edge_multiple_breaking_footers() {
    let (dir, repo) = init_repo();
    write_config(dir.path(), MINIMAL_CONFIG);
    commit(&repo, dir.path(), ".release-ratchet.toml",
        &std::fs::read_to_string(dir.path().join(".release-ratchet.toml")).unwrap(),
        "chore: config");
    // Commit with both ! and BREAKING CHANGE footer
    let msg = "refactor!: rewrite core\n\nBREAKING CHANGE: removed v1 API\nBREAKING-CHANGE: config format changed";
    commit(&repo, dir.path(), "a.txt", "x", msg);
    let (stdout, _) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "status", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["bump_level"], "major");
    assert!(json["breaking_changes"].as_u64().unwrap() >= 1);
}

#[test]
fn edge_long_body_with_footers() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    let msg = "feat(auth): add OAuth2 PKCE support\n\n\
        This implements the full OAuth2 PKCE flow.\n\
        It supports all major identity providers.\n\n\
        Reviewed-by: Alice <alice@example.com>\n\
        Refs #123\n\
        BREAKING CHANGE: token endpoint changed";
    commit(&repo, dir.path(), "a.txt", "x", msg);
    let (stdout, _) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--dry-run"]));
    assert!(stdout.contains("BREAKING CHANGES"));
    assert!(stdout.contains("OAuth2 PKCE"));
}

#[test]
fn edge_three_rapid_releases() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    let r = dir.path().to_str().unwrap();

    commit(&repo, dir.path(), "a.txt", "1", "fix: one");
    run_ok(binary().args(["--repo", r, "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", r, "release"]));

    drop(repo);
    let repo = Repository::open(dir.path()).unwrap();
    commit(&repo, dir.path(), "b.txt", "2", "fix: two");
    run_ok(binary().args(["--repo", r, "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", r, "release"]));

    drop(repo);
    let repo = Repository::open(dir.path()).unwrap();
    commit(&repo, dir.path(), "c.txt", "3", "feat: three");
    run_ok(binary().args(["--repo", r, "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", r, "release"]));

    let repo = Repository::open(dir.path()).unwrap();
    assert!(repo.refname_to_id("refs/tags/v0.0.1").is_ok());
    assert!(repo.refname_to_id("refs/tags/v0.0.2").is_ok());
    assert!(repo.refname_to_id("refs/tags/v0.1.0").is_ok());

    let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();
    let p1 = changelog.find("## [0.1.0]").unwrap();
    let p2 = changelog.find("## [0.0.2]").unwrap();
    let p3 = changelog.find("## [0.0.1]").unwrap();
    assert!(p1 < p2 && p2 < p3, "changelog not in reverse order");
}

#[test]
fn edge_empty_tag_prefix() {
    let (dir, repo) = setup_with_config("tag_prefix = \"\"\nmain_branch = \"main\"\n");
    commit(&repo, dir.path(), "a.txt", "x", "feat: thing");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));
    let repo = Repository::open(dir.path()).unwrap();
    assert!(repo.refname_to_id("refs/tags/0.1.0").is_ok());
}

#[test]
fn edge_bump_release_version_conflict_on_prepare() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: thing");
    let output = binary()
        .args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch", "--bump", "major", "--release-version", "2.0.0"])
        .output().unwrap();
    assert!(!output.status.success());
}

#[test]
fn edge_config_unknown_fields_rejected() {
    let (dir, repo) = init_repo();
    write_config(dir.path(), "tag_prefix = \"v\"\nbogus_field = true\n");
    commit(&repo, dir.path(), ".release-ratchet.toml",
        &std::fs::read_to_string(dir.path().join(".release-ratchet.toml")).unwrap(),
        "chore: config");
    let output = binary().args(["--repo", dir.path().to_str().unwrap(), "status"]).output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.to_lowercase().contains("unknown"));
}

#[test]
fn edge_config_uppercase_override_rejected() {
    let (dir, repo) = init_repo();
    write_config(dir.path(), "tag_prefix = \"v\"\n\n[commit_type_overrides.Refactor]\nbump = \"patch\"\n");
    commit(&repo, dir.path(), ".release-ratchet.toml",
        &std::fs::read_to_string(dir.path().join(".release-ratchet.toml")).unwrap(),
        "chore: config");
    let output = binary().args(["--repo", dir.path().to_str().unwrap(), "status"]).output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("lowercase"));
}

#[test]
fn edge_explicit_config_missing_errors() {
    let (dir, _) = init_repo();
    let output = binary()
        .args(["--repo", dir.path().to_str().unwrap(), "--config", "/nonexistent/config.toml", "status"])
        .output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not found"));
}

#[test]
fn edge_failing_hook_warns_not_fails() {
    let (dir, repo) = init_repo();
    write_config(dir.path(), "tag_prefix = \"v\"\n\n[hooks]\npost_prepare = [\"exit 1\"]\n");
    commit(&repo, dir.path(), ".release-ratchet.toml",
        &std::fs::read_to_string(dir.path().join(".release-ratchet.toml")).unwrap(),
        "chore: config");
    commit(&repo, dir.path(), "a.txt", "x", "feat: thing");
    let (_, stderr) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    assert!(stderr.contains("warning"));
}

#[test]
fn edge_hooks_skipped_on_dry_run() {
    let (dir, repo) = init_repo();
    write_config(dir.path(), "tag_prefix = \"v\"\n\n[hooks]\npost_prepare = [\"echo SHOULD_NOT_SEE\"]\n");
    commit(&repo, dir.path(), ".release-ratchet.toml",
        &std::fs::read_to_string(dir.path().join(".release-ratchet.toml")).unwrap(),
        "chore: config");
    commit(&repo, dir.path(), "a.txt", "x", "feat: thing");
    let output = binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--dry-run"]).output().unwrap();
    let all_output = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(!all_output.contains("SHOULD_NOT_SEE"));
}

#[test]
fn edge_bump_no_commit_created() {
    let (dir, repo) = init_repo();
    write_cargo_toml(dir.path(), "0.0.0");
    write_config(dir.path(), CARGO_CONFIG);
    commit_initial_files(&repo, dir.path(), &["Cargo.toml", ".release-ratchet.toml"]);
    commit(&repo, dir.path(), "a.txt", "x", "feat: thing");
    let before = repo.head().unwrap().peel_to_commit().unwrap().id();
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "bump"]));
    // Reload to see if HEAD changed
    let repo = Repository::open(dir.path()).unwrap();
    let after = repo.head().unwrap().peel_to_commit().unwrap().id();
    assert_eq!(before, after, "bump should not create a commit");
    let cargo = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
    assert!(cargo.contains("version = \"0.1.0\""), "but should modify files");
}

#[test]
fn edge_validate_no_space_after_colon() {
    let (dir, _) = init_repo();
    let output = binary()
        .args(["--repo", dir.path().to_str().unwrap(), "validate", "--message", "feat:no space"])
        .output().unwrap();
    assert_eq!(output.status.code(), Some(3));
}

#[test]
fn edge_validate_empty_message() {
    let (dir, _) = init_repo();
    let output = binary()
        .args(["--repo", dir.path().to_str().unwrap(), "validate", "--message", ""])
        .output().unwrap();
    assert_eq!(output.status.code(), Some(3));
}

#[test]
fn edge_status_json_schema_complete() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat!: breaking");
    let (stdout, _) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "status", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    // All fields present
    for field in ["last_tag", "last_version", "current_file_version", "commits_since",
                  "conventional_commits", "non_conventional_commits", "bump_level",
                  "next_version", "breaking_changes"] {
        assert!(json.get(field).is_some(), "missing field: {field}");
    }
}

#[test]
fn edge_manual_tag_recognized_as_baseline() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: v1");
    // Manually create a tag (not via release-ratchet)
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.tag_lightweight("v0.1.0", head.as_object(), false).unwrap();
    // New commit after the tag
    commit(&repo, dir.path(), "b.txt", "y", "fix: v1 fix");
    let (stdout, _) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "status", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["last_version"], "0.1.0");
    assert_eq!(json["next_version"], "0.1.1");
    assert_eq!(json["conventional_commits"], 1); // only the fix since the tag
}

#[test]
fn edge_notes_no_changelog_file() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: thing");
    // notes --latest when no CHANGELOG.md exists
    let output = binary().args(["--repo", dir.path().to_str().unwrap(), "notes", "--latest"]).output().unwrap();
    assert!(!output.status.success());
    // notes (generate) should still work — doesn't need existing file
    let (stdout, _) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "notes"]));
    assert!(stdout.contains("## [0.1.0]"));
}

#[test]
fn edge_check_no_ecosystems_passes() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: thing");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));
    // Check with no ecosystem files — should still pass (changelog + tag match)
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "check"]));
}

#[test]
fn edge_backport_dry_run_no_branch() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));
    drop(repo);
    let repo = Repository::open(dir.path()).unwrap();
    commit(&repo, dir.path(), "b.txt", "y", "fix: hotfix");
    let fix_oid = repo.head().unwrap().peel_to_commit().unwrap().id().to_string();
    let (_, stderr) = run_ok(binary().args([
        "--repo", dir.path().to_str().unwrap(),
        "backport", &fix_oid, "--onto", "v0.1.0", "--dry-run",
    ]));
    assert!(stderr.contains("DRY RUN"));
    assert!(repo.find_branch("maintain/v0.1.x", git2::BranchType::Local).is_err(),
        "branch should not be created in dry-run");
}

#[test]
fn edge_post_release_hook_runs() {
    let (dir, repo) = init_repo();
    write_config(dir.path(), "tag_prefix = \"v\"\n\n[hooks]\npost_release = [\"echo RELEASED\"]\n");
    commit(&repo, dir.path(), ".release-ratchet.toml",
        &std::fs::read_to_string(dir.path().join(".release-ratchet.toml")).unwrap(),
        "chore: config");
    commit(&repo, dir.path(), "a.txt", "x", "feat: thing");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    let output = binary().args(["--repo", dir.path().to_str().unwrap(), "release"]).output().unwrap();
    assert!(output.status.success());
    let all = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(all.contains("RELEASED"), "hook output not found: {all}");
}

#[test]
fn edge_release_cleanup_flag() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare"]));
    // Should be on release branch
    assert_eq!(repo.head().unwrap().shorthand().unwrap(), "chore/next-release");
    // Switch to main for release
    {
        let release_commit = repo.head().unwrap().peel_to_commit().unwrap();
        let release_oid = release_commit.id();
        let mut main_ref = repo.find_reference("refs/heads/main").unwrap();
        main_ref.set_target(release_oid, "ff merge").unwrap();
        let obj = repo.find_object(release_oid, None).unwrap();
        repo.checkout_tree(&obj, Some(git2::build::CheckoutBuilder::new().force())).unwrap();
        repo.set_head("refs/heads/main").unwrap();
    }
    let (_, stderr) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release", "--cleanup"]));
    assert!(stderr.contains("Deleted branch"));
}

#[test]
fn edge_commit_type_override_works() {
    let (dir, repo) = init_repo();
    // Make refactor trigger a patch bump
    write_config(dir.path(), "tag_prefix = \"v\"\n\n[commit_type_overrides.refactor]\nbump = \"patch\"\nchangelog = \"Refactoring\"\n");
    commit(&repo, dir.path(), ".release-ratchet.toml",
        &std::fs::read_to_string(dir.path().join(".release-ratchet.toml")).unwrap(),
        "chore: config");
    commit(&repo, dir.path(), "a.txt", "x", "refactor: clean up code");
    let (stdout, _) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "status", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["bump_level"], "patch", "refactor should be patch via override");
    let (stdout, _) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--dry-run"]));
    assert!(stdout.contains("### Refactoring"), "custom heading should appear");
}

#[test]
fn edge_shallow_clone_status() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));
    drop(repo);
    // Shallow clone
    let shallow_dir = tempfile::TempDir::new().unwrap();
    std::process::Command::new("git")
        .args(["clone", "--depth", "1", &format!("file://{}", dir.path().display()), shallow_dir.path().to_str().unwrap()])
        .output().unwrap();
    // Status should not crash
    let output = binary().args(["--repo", shallow_dir.path().to_str().unwrap(), "status"]).output().unwrap();
    assert!(output.status.success(), "status failed on shallow clone: {}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn edge_crlf_changelog() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    // Write CRLF changelog
    std::fs::write(dir.path().join("CHANGELOG.md"),
        "# Changelog\r\n\r\n## [0.1.0] - 2025-01-01\r\n\r\n### Features\r\n\r\n- old\r\n").unwrap();
    {
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("CHANGELOG.md")).unwrap();
        index.write().unwrap();
        let sig = git2::Signature::now("Test User", "test@example.com").unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "chore: add changelog", &tree, &[&head]).unwrap();
    }
    // Tag to set baseline
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.tag_lightweight("v0.1.0", head.as_object(), false).unwrap();
    commit(&repo, dir.path(), "a.txt", "x", "feat: new feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();
    let p_new = changelog.find("## [0.2.0]").expect("0.2.0 should exist");
    let p_old = changelog.find("## [0.1.0]").expect("0.1.0 should exist");
    assert!(p_new < p_old, "new version should come before old: {changelog}");
}

#[test]
fn edge_annotated_tag_recognized() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: v1");
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    let sig = git2::Signature::now("Test User", "test@example.com").unwrap();
    repo.tag("v0.1.0", head.as_object(), &sig, "Release v0.1.0", false).unwrap();
    commit(&repo, dir.path(), "b.txt", "y", "fix: after annotated tag");
    let (stdout, _) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "status", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["last_version"], "0.1.0");
    assert_eq!(json["next_version"], "0.1.1");
}

#[test]
fn edge_commit_with_shell_metacharacters() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: add `foo` helper with $PATH and \"quotes\"");
    let (stdout, _) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--dry-run"]));
    assert!(stdout.contains("`foo`"));
}

#[test]
fn edge_release_specific_commit() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    let target = repo.head().unwrap().peel_to_commit().unwrap().id().to_string();
    commit(&repo, dir.path(), "b.txt", "y", "chore: after release commit");
    let (_, stderr) = run_ok(binary().args([
        "--repo", dir.path().to_str().unwrap(), "release", "--commit", &target,
    ]));
    assert!(stderr.contains("Created tag 'v0.1.0'"));
}

#[test]
fn edge_package_json_no_version_not_detected() {
    let (dir, repo) = init_repo();
    std::fs::write(dir.path().join("package.json"), "{\"name\": \"private\", \"private\": true}").unwrap();
    {
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("package.json")).unwrap();
        index.write().unwrap();
        let sig = git2::Signature::now("Test User", "test@example.com").unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "chore: add pkg", &tree, &[&head]).unwrap();
    }
    commit(&repo, dir.path(), "a.txt", "x", "feat: thing");
    // Should work (auto-detect skips package.json with no version)
    let (stdout, _) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--dry-run"]));
    assert!(!stdout.contains("package.json"), "should not detect versionless package.json");
}

#[test]
fn edge_large_version_numbers() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: thing");
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.tag_lightweight("v999.999.999", head.as_object(), false).unwrap();
    commit(&repo, dir.path(), "b.txt", "y", "feat: another");
    let (stdout, _) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "status", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["last_version"], "999.999.999");
    assert_eq!(json["next_version"], "999.1000.0");
}

#[test]
fn edge_multiple_tags_same_commit() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: thing");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));
    // Add an extra tag on the same commit
    {
        let repo = Repository::open(dir.path()).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.tag_lightweight("extra-tag", head.as_object(), false).unwrap();
    }
    let repo = Repository::open(dir.path()).unwrap();
    commit(&repo, dir.path(), "b.txt", "y", "fix: fix");
    let (stdout, _) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "status", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["last_version"], "0.1.0", "should use semver tag, not extra-tag");
}

#[test]
fn edge_notes_stdout_clean_for_piping() {
    let (dir, repo) = setup_with_config(MINIMAL_CONFIG);
    commit(&repo, dir.path(), "a.txt", "x", "feat: feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));
    // stdout should be clean markdown, stderr should have nothing
    let output = binary().args(["--repo", dir.path().to_str().unwrap(), "notes", "--latest"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("## ["), "stdout should start with markdown heading: {stdout}");
}

#[test]
fn edge_check_json_schema() {
    let (dir, repo) = init_repo();
    write_cargo_toml(dir.path(), "0.0.0");
    write_config(dir.path(), CARGO_CONFIG);
    commit_initial_files(&repo, dir.path(), &["Cargo.toml", ".release-ratchet.toml"]);
    commit(&repo, dir.path(), "a.txt", "x", "feat: thing");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));
    let (stdout, _) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "check", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["consistent"], true);
    assert!(json.get("tag_version").is_some());
    assert!(json.get("file_versions").is_some());
    assert!(json.get("errors").is_some());
    assert!(json.get("changelog_has_section").is_some());
}

// ============================================================================
// Forge: Bitbucket Cloud
// ============================================================================

#[test]
fn forge_bitbucket_cloud_squash_merge_detected() {
    let config = r#"
        tag_prefix = "v"
        forge = "bitbucket-cloud"
        [[ecosystems]]
        type = "cargo"
        path = "Cargo.toml"
    "#;
    let (dir, repo) = setup_with_config(config);
    write_cargo_toml(dir.path(), "0.0.0");
    commit_initial_files(&repo, dir.path(), &["Cargo.toml"]);

    // Simulate a Bitbucket Cloud squash merge commit
    commit(
        &repo, dir.path(), "auth.rs", "fn login() {}",
        "Merged in feature/auth (pull request #42)\n\nfeat(auth): add login endpoint\n\n* abc1234 initial auth\n* def5678 add tests",
    );

    let (_, stderr) = run_ok(binary().args([
        "--repo", dir.path().to_str().unwrap(), "status",
    ]));
    assert!(stderr.contains("minor"), "expected minor bump from feat, got: {stderr}");
    assert!(stderr.contains("0.1.0"), "expected 0.1.0, got: {stderr}");
}

#[test]
fn forge_bitbucket_cloud_full_e2e() {
    let bb_config = r#"
        tag_prefix = "v"
        forge = "bitbucket-cloud"
        [[ecosystems]]
        type = "cargo"
        path = "Cargo.toml"
    "#;
    let (dir, repo) = setup_with_config(bb_config);
    let r = dir.path().to_str().unwrap();
    write_cargo_toml(dir.path(), "0.0.0");
    commit_initial_files(&repo, dir.path(), &["Cargo.toml"]);

    // --- Cycle 1: BB squash merge feat → 0.1.0 ---
    commit(
        &repo, dir.path(), "auth.rs", "fn login() {}",
        "Merged in feature/auth (pull request #42)\n\nfeat(auth): add login endpoint\n\n* abc1234 initial auth\n* def5678 add tests",
    );

    // Status should detect it as a minor bump
    let (stdout, _) = run_ok(binary().args(["--repo", r, "status", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["bump_level"], "minor");
    assert_eq!(json["next_version"], "0.1.0");

    // Notes should generate changelog content
    let (stdout, _) = run_ok(binary().args(["--repo", r, "notes"]));
    assert!(stdout.contains("add login endpoint"), "notes missing description: {stdout}");
    assert!(stdout.contains("Features"), "notes missing Features heading: {stdout}");

    // Validate should pass (BB merge commit recognized as valid via forge config)
    let (_, stderr) = run_ok(binary().args(["--repo", r, "validate"]));
    assert!(stderr.contains("All commits are valid"), "validate failed: {stderr}");

    // Prepare
    run_ok(binary().args(["--repo", r, "prepare", "--no-branch"]));
    let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();
    assert!(changelog.contains("## [0.1.0]"), "changelog missing 0.1.0 section");
    assert!(changelog.contains("add login endpoint"), "changelog missing commit description");
    let cargo = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
    assert!(cargo.contains("version = \"0.1.0\""), "Cargo.toml not bumped");

    // Release
    let (_, stderr) = run_ok(binary().args(["--repo", r, "release"]));
    assert!(stderr.contains("Created tag 'v0.1.0'"), "release didn't create tag: {stderr}");
    assert!(repo.refname_to_id("refs/tags/v0.1.0").is_ok());

    // Check should pass post-release
    let (stdout, _) = run_ok(binary().args(["--repo", r, "check", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["consistent"], true, "check failed: {stdout}");

    // --- Cycle 2: mix of BB squash merge fix + normal commit → 0.1.1 ---
    commit(
        &repo, dir.path(), "parser.rs", "fn parse() {}",
        "Merged in bugfix/parser (pull request #43)\n\nfix(parser): handle empty input\n\n* 111 fix empty\n* 222 add test",
    );
    // Also a regular (non-BB) commit in the same cycle
    commit(
        &repo, dir.path(), "docs.rs", "// docs",
        "docs: update API reference",
    );

    let (stdout, _) = run_ok(binary().args(["--repo", r, "status", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["bump_level"], "patch");
    assert_eq!(json["next_version"], "0.1.1");

    // Prepare and release cycle 2
    run_ok(binary().args(["--repo", r, "prepare", "--no-branch"]));
    let (_, stderr) = run_ok(binary().args(["--repo", r, "release"]));
    assert!(stderr.contains("Created tag 'v0.1.1'"), "cycle 2 tag: {stderr}");

    // Changelog should have both versions in order
    let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();
    let p1 = changelog.find("## [0.1.1]").expect("missing 0.1.1");
    let p2 = changelog.find("## [0.1.0]").expect("missing 0.1.0");
    assert!(p1 < p2, "0.1.1 should come before 0.1.0 in changelog");
    assert!(changelog.contains("handle empty input"), "changelog missing fix description");

    // Notes --latest should return the 0.1.1 section
    let (stdout, _) = run_ok(binary().args(["--repo", r, "notes", "--latest"]));
    assert!(stdout.contains("0.1.1"), "notes --latest missing 0.1.1: {stdout}");
    assert!(stdout.contains("handle empty input"), "notes --latest missing fix: {stdout}");

    // Notes for specific version
    let (stdout, _) = run_ok(binary().args(["--repo", r, "notes", "0.1.0"]));
    assert!(stdout.contains("add login endpoint"), "notes 0.1.0 wrong: {stdout}");

    // --- Cycle 3: BB squash merge breaking → 1.0.0 ---
    commit(
        &repo, dir.path(), "api.rs", "fn v2() {}",
        "Merged in feature/v2 (pull request #44)\n\nfeat!: redesign API\n\nNew API surface.\n\nBREAKING CHANGE: removed /v1 endpoints",
    );

    let (stdout, _) = run_ok(binary().args(["--repo", r, "status", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["bump_level"], "major");
    assert_eq!(json["next_version"], "1.0.0");
    assert_eq!(json["breaking_changes"], 1);

    run_ok(binary().args(["--repo", r, "prepare", "--no-branch"]));

    // Changelog should have BREAKING CHANGES section
    let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();
    assert!(changelog.contains("BREAKING CHANGES"), "missing breaking section: {changelog}");
    assert!(changelog.contains("removed /v1 endpoints"), "missing breaking detail: {changelog}");

    let (_, stderr) = run_ok(binary().args(["--repo", r, "release"]));
    assert!(stderr.contains("Created tag 'v1.0.0'"), "cycle 3 tag: {stderr}");

    // Final status should be clean
    let (stdout, _) = run_ok(binary().args(["--repo", r, "status", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["last_version"], "1.0.0");
    assert_eq!(json["bump_level"], "none");

    // Final check
    let (stdout, _) = run_ok(binary().args(["--repo", r, "check", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["consistent"], true);
    assert_eq!(json["tag_version"], "1.0.0");
}

#[test]
fn forge_bitbucket_cloud_breaking_change_is_major() {
    let config = r#"
        tag_prefix = "v"
        forge = "bitbucket-cloud"
        [[ecosystems]]
        type = "cargo"
        path = "Cargo.toml"
    "#;
    let (dir, repo) = setup_with_config(config);
    write_cargo_toml(dir.path(), "1.0.0");
    commit_initial_files(&repo, dir.path(), &["Cargo.toml"]);
    // Tag current state as v1.0.0
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.tag_lightweight("v1.0.0", head.as_object(), false).unwrap();

    commit(
        &repo, dir.path(), "api.rs", "fn new_api() {}",
        "Merged in feature/v2-api (pull request #100)\n\nfeat!: redesign API\n\nBREAKING CHANGE: old endpoints removed",
    );

    let (_, stderr) = run_ok(binary().args([
        "--repo", dir.path().to_str().unwrap(), "status",
    ]));
    assert!(stderr.contains("major"), "expected major bump, got: {stderr}");
    assert!(stderr.contains("2.0.0"), "expected 2.0.0, got: {stderr}");
}

#[test]
fn forge_none_ignores_bb_merge_commits() {
    let config = r#"
        tag_prefix = "v"
        [[ecosystems]]
        type = "cargo"
        path = "Cargo.toml"
    "#;
    let (dir, repo) = setup_with_config(config);
    write_cargo_toml(dir.path(), "0.0.0");
    commit_initial_files(&repo, dir.path(), &["Cargo.toml"]);

    commit(
        &repo, dir.path(), "auth.rs", "fn login() {}",
        "Merged in feature/auth (pull request #42)\n\nfeat(auth): add login endpoint",
    );

    // Without forge config, BB merge commit should be treated as non-conventional
    let (_, stderr) = run_ok(binary().args([
        "--repo", dir.path().to_str().unwrap(), "status",
    ]));
    assert!(stderr.contains("none") || stderr.contains("None"), "expected no bump without forge config, got: {stderr}");
}
