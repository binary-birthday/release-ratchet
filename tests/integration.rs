use std::path::Path;

use git2::{Repository, Signature};
use tempfile::TempDir;

/// Create a temp dir with an initialized git repo and an initial commit.
fn init_repo() -> (TempDir, Repository) {
    let dir = TempDir::new().unwrap();
    let repo = Repository::init(dir.path()).unwrap();

    // Configure user for commits
    let mut config = repo.config().unwrap();
    config.set_str("user.name", "Test User").unwrap();
    config.set_str("user.email", "test@example.com").unwrap();

    // Create initial commit so HEAD exists
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

    // Ensure the branch is named "main"
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

/// Add a file and commit with the given message.
fn commit(repo: &Repository, path: &Path, filename: &str, content: &str, message: &str) {
    // Write the file
    let file_path = path.join(filename);
    std::fs::write(&file_path, content).unwrap();

    // Stage it
    let mut index = repo.index().unwrap();
    index.add_path(Path::new(filename)).unwrap();
    index.write().unwrap();

    // Commit
    let sig = Signature::now("Test User", "test@example.com").unwrap();
    let tree_oid = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_oid).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[&head])
        .unwrap();
}

/// Write a .release-ratchet.yml config file.
fn write_config(path: &Path, content: &str) {
    std::fs::write(path.join(".release-ratchet.yml"), content).unwrap();
}

/// Write a Cargo.toml with the given version.
fn write_cargo_toml(path: &Path, version: &str) {
    std::fs::write(
        path.join("Cargo.toml"),
        format!(
            r#"[package]
name = "test-project"
version = "{version}"
edition = "2021"
"#
        ),
    )
    .unwrap();
}

/// Write a package.json with the given version.
fn write_package_json(path: &Path, version: &str) {
    std::fs::write(
        path.join("package.json"),
        format!(
            r#"{{
  "name": "test-project",
  "version": "{version}"
}}"#
        ),
    )
    .unwrap();
}

/// Write a pyproject.toml with the given version.
fn write_pyproject_toml(path: &Path, version: &str) {
    std::fs::write(
        path.join("pyproject.toml"),
        format!(
            r#"[project]
name = "test-project"
version = "{version}"
"#
        ),
    )
    .unwrap();
}

fn binary() -> std::process::Command {
    let bin = env!("CARGO_BIN_EXE_release-ratchet");
    std::process::Command::new(bin)
}

// ============================================================================
// Tests
// ============================================================================

#[test]
fn status_on_fresh_repo_with_no_conventional_commits() {
    let (dir, _repo) = init_repo();

    let output = binary()
        .args(["--repo", dir.path().to_str().unwrap(), "status"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Last release:    (none)"));
    assert!(stderr.contains("Bump level:      none"));
}

#[test]
fn status_shows_pending_feat_as_minor() {
    let (dir, repo) = init_repo();

    commit(&repo, dir.path(), "a.txt", "hello", "feat: add feature A");
    commit(&repo, dir.path(), "b.txt", "world", "fix: fix bug B");

    let output = binary()
        .args(["--repo", dir.path().to_str().unwrap(), "status"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Bump level:      minor"));
    assert!(stderr.contains("Next version:    0.1.0"));
}

#[test]
fn status_json_output() {
    let (dir, repo) = init_repo();

    commit(&repo, dir.path(), "a.txt", "hello", "feat: add feature A");

    let output = binary()
        .args(["--repo", dir.path().to_str().unwrap(), "status", "--json"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["bump_level"], "minor");
    assert_eq!(json["next_version"], "0.1.0");
    // 2 conventional commits: "chore: initial commit" + "feat: add feature A"
    assert_eq!(json["conventional_commits"], 2);
}

#[test]
fn status_breaking_change_is_major() {
    let (dir, repo) = init_repo();

    commit(&repo, dir.path(), "a.txt", "v2", "feat!: rewrite everything");

    let output = binary()
        .args(["--repo", dir.path().to_str().unwrap(), "status"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Bump level:      major"));
    assert!(stderr.contains("Next version:    1.0.0"));
}

#[test]
fn validate_valid_message() {
    let (dir, _repo) = init_repo();

    let output = binary()
        .args([
            "--repo", dir.path().to_str().unwrap(),
            "validate", "--message", "feat(auth): add OAuth support",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
}

#[test]
fn validate_invalid_message() {
    let (dir, _repo) = init_repo();

    let output = binary()
        .args([
            "--repo", dir.path().to_str().unwrap(),
            "validate", "--message", "updated the readme",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(3));
}

#[test]
fn init_creates_config_file() {
    let (dir, _repo) = init_repo();

    let output = binary()
        .args(["--repo", dir.path().to_str().unwrap(), "init"])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(dir.path().join(".release-ratchet.yml").exists());

    // Running again without --force should fail
    let output2 = binary()
        .args(["--repo", dir.path().to_str().unwrap(), "init"])
        .output()
        .unwrap();
    assert!(!output2.status.success());

    // With --force should succeed
    let output3 = binary()
        .args(["--repo", dir.path().to_str().unwrap(), "init", "--force"])
        .output()
        .unwrap();
    assert!(output3.status.success());
}

#[test]
fn prepare_dry_run() {
    let (dir, repo) = init_repo();

    write_config(dir.path(), "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n");
    commit(&repo, dir.path(), ".release-ratchet.yml",
        &std::fs::read_to_string(dir.path().join(".release-ratchet.yml")).unwrap(),
        "chore: add config");

    commit(&repo, dir.path(), "a.txt", "hello", "feat: add feature A");
    commit(&repo, dir.path(), "b.txt", "world", "fix(core): fix bug B");

    let output = binary()
        .args([
            "--repo", dir.path().to_str().unwrap(),
            "prepare", "--dry-run",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should show the changelog preview
    assert!(stdout.contains("## [0.1.0]"));
    assert!(stdout.contains("### Features"));
    assert!(stdout.contains("add feature A"));
    assert!(stdout.contains("### Bug Fixes"));
    assert!(stdout.contains("**core**: fix bug B"));

    // Should show version bump
    assert!(stderr.contains("0.0.0 -> 0.1.0"));

    // Should NOT have created a branch or changelog file
    assert!(!dir.path().join("CHANGELOG.md").exists());
}

#[test]
fn prepare_creates_release_branch_and_changelog() {
    let (dir, repo) = init_repo();

    write_cargo_toml(dir.path(), "0.0.0");
    write_config(
        dir.path(),
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems:\n  - type: cargo\n    path: \"Cargo.toml\"\n",
    );

    // Stage and commit initial files
    {
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("Cargo.toml")).unwrap();
        index.add_path(Path::new(".release-ratchet.yml")).unwrap();
        index.write().unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let sig = Signature::now("Test User", "test@example.com").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "chore: setup project", &tree, &[&head]).unwrap();
    }

    commit(&repo, dir.path(), "feature.rs", "fn main() {}", "feat: add main feature");
    commit(&repo, dir.path(), "fix.rs", "fn fix() {}", "fix: patch a bug");

    let output = binary()
        .args([
            "--repo", dir.path().to_str().unwrap(),
            "prepare",
        ])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "prepare failed: {stderr}");
    assert!(stderr.contains("0.0.0 -> 0.1.0"));
    assert!(stderr.contains("Created release commit"));

    // Should be on the release branch
    let head = repo.head().unwrap();
    let branch_name = head.shorthand().unwrap();
    assert_eq!(branch_name, "release-ratchet--release");

    // CHANGELOG.md should exist and contain the release
    let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();
    assert!(changelog.contains("## [0.1.0]"));
    assert!(changelog.contains("### Features"));
    assert!(changelog.contains("add main feature"));
    assert!(changelog.contains("### Bug Fixes"));
    assert!(changelog.contains("patch a bug"));

    // Cargo.toml should have the new version
    let cargo = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
    assert!(cargo.contains("version = \"0.1.0\""));
}

#[test]
fn prepare_with_no_branch_flag() {
    let (dir, repo) = init_repo();

    write_config(dir.path(), "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n");
    commit(&repo, dir.path(), ".release-ratchet.yml",
        &std::fs::read_to_string(dir.path().join(".release-ratchet.yml")).unwrap(),
        "chore: add config");
    commit(&repo, dir.path(), "a.txt", "hello", "feat: new thing");

    let output = binary()
        .args([
            "--repo", dir.path().to_str().unwrap(),
            "prepare", "--no-branch",
        ])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "prepare failed: {stderr}");

    // Should still be on main (the default initial branch)
    let head = repo.head().unwrap();
    let branch_name = head.shorthand().unwrap();
    assert_ne!(branch_name, "release-ratchet--release");
}

#[test]
fn prepare_bump_override() {
    let (dir, repo) = init_repo();

    write_config(dir.path(), "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n");
    commit(&repo, dir.path(), ".release-ratchet.yml",
        &std::fs::read_to_string(dir.path().join(".release-ratchet.yml")).unwrap(),
        "chore: add config");
    commit(&repo, dir.path(), "a.txt", "hello", "fix: small fix");

    let output = binary()
        .args([
            "--repo", dir.path().to_str().unwrap(),
            "prepare", "--bump", "major", "--no-branch",
        ])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "prepare failed: {stderr}");
    assert!(stderr.contains("0.0.0 -> 1.0.0 (major)"));
}

#[test]
fn full_prepare_and_release_cycle() {
    let (dir, repo) = init_repo();

    write_cargo_toml(dir.path(), "0.0.0");
    write_config(
        dir.path(),
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems:\n  - type: cargo\n    path: \"Cargo.toml\"\n",
    );

    {
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("Cargo.toml")).unwrap();
        index.add_path(Path::new(".release-ratchet.yml")).unwrap();
        index.write().unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let sig = Signature::now("Test User", "test@example.com").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "chore: setup", &tree, &[&head]).unwrap();
    }



    commit(&repo, dir.path(), "feature.rs", "fn feat() {}", "feat: add feature");

    // Phase 1: prepare
    let output = binary()
        .args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "prepare failed: {stderr}");

    // The HEAD commit should now be "chore: release v0.1.0"
    let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
    let msg = head_commit.message().unwrap();
    assert!(msg.contains("chore: release v0.1.0"), "unexpected commit message: {msg}");

    // Phase 2: release (tag the HEAD)
    let output = binary()
        .args(["--repo", dir.path().to_str().unwrap(), "release"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "release failed: {stderr}");
    assert!(stderr.contains("Created tag 'v0.1.0'"));

    // Verify the tag exists
    let tag_ref = repo.refname_to_id("refs/tags/v0.1.0");
    assert!(tag_ref.is_ok(), "tag v0.1.0 should exist");

    // Now add more commits and do another cycle
    commit(&repo, dir.path(), "fix.rs", "fn fix() {}", "fix: patch something");

    let output = binary()
        .args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "second prepare failed: {stderr}");
    assert!(stderr.contains("0.1.0 -> 0.1.1 (patch)"));

    let output = binary()
        .args(["--repo", dir.path().to_str().unwrap(), "release"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let tag_ref = repo.refname_to_id("refs/tags/v0.1.1");
    assert!(tag_ref.is_ok(), "tag v0.1.1 should exist");

    // Changelog should have both versions
    let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();
    assert!(changelog.contains("## [0.1.1]"));
    assert!(changelog.contains("## [0.1.0]"));

    // 0.1.1 should come before 0.1.0 in the file
    let pos_011 = changelog.find("## [0.1.1]").unwrap();
    let pos_010 = changelog.find("## [0.1.0]").unwrap();
    assert!(pos_011 < pos_010, "0.1.1 should appear before 0.1.0 in changelog");
}

#[test]
fn prepare_exits_2_when_nothing_to_release() {
    let (dir, repo) = init_repo();

    write_config(dir.path(), "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n");
    commit(&repo, dir.path(), ".release-ratchet.yml",
        &std::fs::read_to_string(dir.path().join(".release-ratchet.yml")).unwrap(),
        "chore: add config");

    // Only chore commits (no bump)
    commit(&repo, dir.path(), "a.txt", "hello", "chore: update deps");
    commit(&repo, dir.path(), "b.txt", "world", "docs: update readme");

    let output = binary()
        .args(["--repo", dir.path().to_str().unwrap(), "prepare"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn release_dry_run() {
    let (dir, repo) = init_repo();

    write_config(dir.path(), "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n");
    commit(&repo, dir.path(), ".release-ratchet.yml",
        &std::fs::read_to_string(dir.path().join(".release-ratchet.yml")).unwrap(),
        "chore: add config");
    commit(&repo, dir.path(), "a.txt", "hello", "feat: add feature");

    // Prepare first
    let output = binary()
        .args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"])
        .output()
        .unwrap();
    assert!(output.status.success());

    // Dry run release
    let output = binary()
        .args(["--repo", dir.path().to_str().unwrap(), "release", "--dry-run"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Would create tag"));

    // Tag should NOT exist
    assert!(repo.refname_to_id("refs/tags/v0.1.0").is_err());
}

#[test]
fn multiple_ecosystem_version_bumping() {
    let (dir, repo) = init_repo();

    write_cargo_toml(dir.path(), "0.0.0");
    write_package_json(dir.path(), "0.0.0");
    write_pyproject_toml(dir.path(), "0.0.0");

    write_config(
        dir.path(),
        r#"tag_prefix: "v"
main_branch: "main"
ecosystems:
  - type: cargo
    path: "Cargo.toml"
  - type: node
    path: "package.json"
  - type: python
    path: "pyproject.toml"
"#,
    );

    // Stage all initial files
    {
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("Cargo.toml")).unwrap();
        index.add_path(Path::new("package.json")).unwrap();
        index.add_path(Path::new("pyproject.toml")).unwrap();
        index.add_path(Path::new(".release-ratchet.yml")).unwrap();
        index.write().unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let sig = Signature::now("Test User", "test@example.com").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "chore: setup", &tree, &[&head]).unwrap();
    }

    commit(&repo, dir.path(), "feature.rs", "fn f() {}", "feat: add feature");

    let output = binary()
        .args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "prepare failed: {stderr}");

    // All three should be bumped
    let cargo = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
    assert!(cargo.contains("version = \"0.1.0\""), "Cargo.toml not bumped: {cargo}");

    let pkg = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
    assert!(pkg.contains("\"0.1.0\""), "package.json not bumped: {pkg}");

    let pyp = std::fs::read_to_string(dir.path().join("pyproject.toml")).unwrap();
    assert!(pyp.contains("version = \"0.1.0\""), "pyproject.toml not bumped: {pyp}");
}

#[test]
fn release_prevents_duplicate_tag() {
    let (dir, repo) = init_repo();

    write_config(dir.path(), "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n");
    commit(&repo, dir.path(), ".release-ratchet.yml",
        &std::fs::read_to_string(dir.path().join(".release-ratchet.yml")).unwrap(),
        "chore: add config");
    commit(&repo, dir.path(), "a.txt", "hello", "feat: add feature");

    // Prepare and release
    let _ = binary()
        .args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"])
        .output()
        .unwrap();
    let _ = binary()
        .args(["--repo", dir.path().to_str().unwrap(), "release"])
        .output()
        .unwrap();

    // Try to release again -- should fail because tag exists
    let output = binary()
        .args(["--repo", dir.path().to_str().unwrap(), "release"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("already exists"));
}

#[test]
fn status_after_tag_shows_incremental() {
    let (dir, repo) = init_repo();

    write_config(dir.path(), "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n");
    commit(&repo, dir.path(), ".release-ratchet.yml",
        &std::fs::read_to_string(dir.path().join(".release-ratchet.yml")).unwrap(),
        "chore: add config");
    commit(&repo, dir.path(), "a.txt", "hello", "feat: feature one");

    // Do a full release cycle
    let _ = binary()
        .args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"])
        .output()
        .unwrap();
    let _ = binary()
        .args(["--repo", dir.path().to_str().unwrap(), "release"])
        .output()
        .unwrap();

    // Add another fix commit
    commit(&repo, dir.path(), "b.txt", "world", "fix: fix something");

    let output = binary()
        .args(["--repo", dir.path().to_str().unwrap(), "status", "--json"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["last_version"], "0.1.0");
    assert_eq!(json["bump_level"], "patch");
    assert_eq!(json["next_version"], "0.1.1");
    // Should only see 1 commit (the fix), not the whole history
    assert_eq!(json["conventional_commits"], 1);
}
