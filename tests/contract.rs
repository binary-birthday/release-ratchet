//! Contract tests: verify that prepare's outputs are consumable by release,
//! that the changelog format round-trips, and that ecosystem writes are
//! readable back.

use std::path::Path;

use git2::{Repository, Signature};
use tempfile::TempDir;

fn init_repo() -> (TempDir, Repository) {
    let dir = TempDir::new().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    let mut config = repo.config().unwrap();
    config.set_str("user.name", "Test User").unwrap();
    config.set_str("user.email", "test@example.com").unwrap();
    {
        let sig = Signature::now("Test User", "test@example.com").unwrap();
        let tree_oid = repo.index().unwrap().write_tree().unwrap();
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

fn setup_with_config(config: &str) -> (TempDir, Repository) {
    let (dir, repo) = init_repo();
    std::fs::write(dir.path().join(".release-ratchet.yml"), config).unwrap();
    commit(&repo, dir.path(), ".release-ratchet.yml",
        &std::fs::read_to_string(dir.path().join(".release-ratchet.yml")).unwrap(),
        "chore: add config");
    (dir, repo)
}

// ============================================================================
// Contract: prepare's commit message is parseable by release
// ============================================================================

#[test]
fn contract_prepare_commit_message_consumed_by_release() {
    let (dir, repo) = setup_with_config(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n",
    );
    commit(&repo, dir.path(), "a.txt", "x", "feat: add feature");

    // prepare creates a commit
    run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));

    // release must be able to parse the version from that commit
    let (_, stderr) = run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "release"]));
    assert!(stderr.contains("Created tag 'v0.1.0'"));
}

#[test]
fn contract_prepare_commit_message_with_custom_prefix() {
    let (dir, repo) = setup_with_config(
        "tag_prefix: \"release-v\"\nmain_branch: \"main\"\necosystems: []\n",
    );
    commit(&repo, dir.path(), "a.txt", "x", "feat: something");

    run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));

    // release must handle multi-char prefix correctly
    let (_, stderr) = run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "release"]));
    assert!(stderr.contains("Created tag 'release-v0.1.0'"));
}

#[test]
fn contract_prepare_commit_message_with_empty_prefix() {
    let (dir, repo) = setup_with_config(
        "tag_prefix: \"\"\nmain_branch: \"main\"\necosystems: []\n",
    );
    commit(&repo, dir.path(), "a.txt", "x", "feat: something");

    run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));

    let (_, stderr) = run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "release"]));
    assert!(stderr.contains("Created tag '0.1.0'"));
}

// ============================================================================
// Contract: changelog format round-trips (prepare writes, release reads)
// ============================================================================

#[test]
fn contract_changelog_is_parseable_by_release_fallback() {
    // Simulate a squash merge: release can't find version in commit message
    // but falls back to reading CHANGELOG.md from the tree
    let (dir, repo) = setup_with_config(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n",
    );
    commit(&repo, dir.path(), "a.txt", "x", "feat: add feature");

    // prepare creates the release commit with changelog
    run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));

    // Amend the commit message to simulate a squash merge (no "chore: release" pattern).
    // We create a new commit with the same tree and parent as HEAD, but a different message.
    {
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let tree = head.tree().unwrap();
        let sig = Signature::now("Test User", "test@example.com").unwrap();
        let parent = head.parent(0).unwrap();
        // Create the new commit (not attached to any ref yet)
        let new_oid = repo.commit(
            None, &sig, &sig,
            "Merge pull request #1 from feature-branch",
            &tree, &[&parent],
        ).unwrap();
        // Point HEAD (main) at the new commit
        let mut head_ref = repo.find_reference("refs/heads/main").unwrap();
        head_ref.set_target(new_oid, "simulate squash merge").unwrap();
    }

    // release should still detect v0.1.0 from the CHANGELOG.md in the tree
    let (_, stderr) = run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "release"]));
    assert!(stderr.contains("Created tag 'v0.1.0'"));
}

// ============================================================================
// Contract: status JSON schema is stable
// ============================================================================

#[test]
fn contract_status_json_has_all_fields() {
    let (dir, repo) = setup_with_config(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n",
    );
    commit(&repo, dir.path(), "a.txt", "x", "feat!: breaking thing");

    let (stdout, _) = run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "status", "--json"]));

    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    // All expected fields must be present
    assert!(json.get("last_tag").is_some(), "missing last_tag");
    assert!(json.get("last_version").is_some(), "missing last_version");
    assert!(json.get("current_file_version").is_some(), "missing current_file_version");
    assert!(json.get("commits_since").is_some(), "missing commits_since");
    assert!(json.get("conventional_commits").is_some(), "missing conventional_commits");
    assert!(json.get("non_conventional_commits").is_some(), "missing non_conventional_commits");
    assert!(json.get("bump_level").is_some(), "missing bump_level");
    assert!(json.get("next_version").is_some(), "missing next_version");
    assert!(json.get("breaking_changes").is_some(), "missing breaking_changes");

    // Verify types
    assert!(json["last_version"].is_string());
    assert!(json["commits_since"].is_number());
    assert!(json["bump_level"].is_string());
    assert!(json["breaking_changes"].is_number());
}

// ============================================================================
// Contract: ecosystem write_version → read_version round-trip
// ============================================================================

#[test]
fn contract_cargo_version_round_trips() {
    let (dir, repo) = setup_with_config(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems:\n  - type: cargo\n    path: \"Cargo.toml\"\n",
    );
    std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"test\"\nversion = \"0.0.0\"\nedition = \"2021\"\n").unwrap();
    commit(&repo, dir.path(), "Cargo.toml",
        &std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap(),
        "chore: add cargo");
    commit(&repo, dir.path(), "a.txt", "x", "feat: feature");

    run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));

    // Read it back via status
    let (stdout, _) = run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "status", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["current_file_version"], "0.1.0");
}

#[test]
fn contract_node_version_round_trips() {
    let (dir, repo) = setup_with_config(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems:\n  - type: node\n    path: \"package.json\"\n",
    );
    std::fs::write(dir.path().join("package.json"), "{\n  \"name\": \"test\",\n  \"version\": \"0.0.0\"\n}\n").unwrap();
    commit(&repo, dir.path(), "package.json",
        &std::fs::read_to_string(dir.path().join("package.json")).unwrap(),
        "chore: add pkg");
    commit(&repo, dir.path(), "a.txt", "x", "feat: feature");

    run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));

    let (stdout, _) = run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "status", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["current_file_version"], "0.1.0");
}

#[test]
fn contract_node_preserves_formatting() {
    let (dir, repo) = setup_with_config(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems:\n  - type: node\n    path: \"package.json\"\n",
    );
    // Use tabs and unusual spacing
    let original = "{\n\t\"name\": \"test\",\n\t\"version\": \"0.0.0\",\n\t\"description\": \"a thing\"\n}\n";
    std::fs::write(dir.path().join("package.json"), original).unwrap();
    commit(&repo, dir.path(), "package.json", original, "chore: add pkg");
    commit(&repo, dir.path(), "a.txt", "x", "feat: feature");

    run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));

    let result = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
    // Tabs should be preserved
    assert!(result.contains("\t\"name\""), "tabs lost: {result}");
    assert!(result.contains("\t\"description\""), "other fields lost: {result}");
    // Version should be bumped
    assert!(result.contains("\"version\": \"0.1.0\""), "version not bumped: {result}");
}

#[test]
fn contract_node_with_nested_version_only_bumps_toplevel() {
    let (dir, repo) = setup_with_config(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems:\n  - type: node\n    path: \"package.json\"\n",
    );
    // package.json with nested "version" in overrides
    let json = r#"{
  "name": "test",
  "overrides": {
    "some-dep": {
      "version": "0.0.0"
    }
  },
  "version": "0.0.0",
  "description": "test"
}"#;
    std::fs::write(dir.path().join("package.json"), json).unwrap();
    commit(&repo, dir.path(), "package.json", json, "chore: add pkg");
    commit(&repo, dir.path(), "a.txt", "x", "feat: feature");

    run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));

    let result = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
    // Top-level version bumped
    assert!(result.contains("\"version\": \"0.1.0\""), "top-level not bumped: {result}");
    // Nested version unchanged — the override still has 0.0.0
    // Count occurrences of "0.0.0" — should be exactly 1 (the nested one)
    let count = result.matches("\"0.0.0\"").count();
    assert_eq!(count, 1, "nested version was incorrectly modified: {result}");
}

#[test]
fn contract_python_version_round_trips() {
    let (dir, repo) = setup_with_config(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems:\n  - type: python\n    path: \"pyproject.toml\"\n",
    );
    std::fs::write(dir.path().join("pyproject.toml"), "[project]\nname = \"test\"\nversion = \"0.0.0\"\n").unwrap();
    commit(&repo, dir.path(), "pyproject.toml",
        &std::fs::read_to_string(dir.path().join("pyproject.toml")).unwrap(),
        "chore: add pyproject");
    commit(&repo, dir.path(), "a.txt", "x", "feat: feature");

    run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));

    let (stdout, _) = run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "status", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["current_file_version"], "0.1.0");
}

#[test]
fn contract_generic_version_round_trips() {
    let (dir, repo) = setup_with_config(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems:\n  - type: generic\n    path: \"version.txt\"\n    pattern: 'VERSION=(\\d+\\.\\d+\\.\\d+)'\n",
    );
    std::fs::write(dir.path().join("version.txt"), "VERSION=0.0.0\n").unwrap();
    commit(&repo, dir.path(), "version.txt", "VERSION=0.0.0\n", "chore: add version file");
    commit(&repo, dir.path(), "a.txt", "x", "feat: feature");

    run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));

    let result = std::fs::read_to_string(dir.path().join("version.txt")).unwrap();
    assert!(result.contains("VERSION=0.1.0"), "generic not bumped: {result}");
}
