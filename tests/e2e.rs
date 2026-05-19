//! End-to-end workflow tests exercising realistic git topologies and
//! edge cases that the happy-path integration tests don't cover.

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

fn run_fail(cmd: &mut std::process::Command) -> (i32, String, String) {
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(!output.status.success(), "command unexpectedly succeeded: {stderr}");
    (output.status.code().unwrap_or(-1), stdout, stderr)
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
// E2E: Breaking change drives major bump
// ============================================================================

#[test]
fn e2e_breaking_change_footer_drives_major() {
    let (dir, repo) = setup_with_config(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n",
    );

    // A fix commit with a BREAKING CHANGE footer (not !)
    let msg = "fix: change API response format\n\nBREAKING CHANGE: the response body is now JSON instead of XML";
    commit(&repo, dir.path(), "a.txt", "x", msg);

    let (stdout, _) = run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "status", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["bump_level"], "major");
    assert_eq!(json["next_version"], "1.0.0");
    assert_eq!(json["breaking_changes"], 1);
}

#[test]
fn e2e_breaking_change_bang_drives_major() {
    let (dir, repo) = setup_with_config(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n",
    );
    commit(&repo, dir.path(), "a.txt", "x", "refactor!: drop Python 2 support");

    let (stdout, _) = run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "status", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["bump_level"], "major");
    // refactor normally doesn't bump, but ! forces major
    assert_eq!(json["next_version"], "1.0.0");
}

// ============================================================================
// E2E: Three-release progression (0.1.0 → 0.2.0 → 1.0.0)
// ============================================================================

#[test]
fn e2e_three_release_progression() {
    let (dir, repo) = setup_with_config(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n",
    );

    // Release 1: feat → 0.1.0
    commit(&repo, dir.path(), "a.txt", "1", "feat: first feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));

    // Release 2: another feat → 0.2.0
    commit(&repo, dir.path(), "b.txt", "2", "feat: second feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));

    // Release 3: breaking → 1.0.0
    commit(&repo, dir.path(), "c.txt", "3", "feat!: v1 rewrite");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    let (_, stderr) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));
    assert!(stderr.contains("Created tag 'v1.0.0'"));

    // Verify all three tags exist
    assert!(repo.refname_to_id("refs/tags/v0.1.0").is_ok());
    assert!(repo.refname_to_id("refs/tags/v0.2.0").is_ok());
    assert!(repo.refname_to_id("refs/tags/v1.0.0").is_ok());

    // Changelog has all three in reverse order
    let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();
    let p1 = changelog.find("## [1.0.0]").unwrap();
    let p2 = changelog.find("## [0.2.0]").unwrap();
    let p3 = changelog.find("## [0.1.0]").unwrap();
    assert!(p1 < p2 && p2 < p3, "changelog not in reverse order");

    // Status shows clean after v1.0.0
    let (stdout, _) = run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "status", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["last_version"], "1.0.0");
    assert_eq!(json["bump_level"], "none");
}

// ============================================================================
// E2E: Branch-based prepare workflow
// ============================================================================

#[test]
fn e2e_branch_based_prepare_then_release() {
    let (dir, repo) = setup_with_config(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n",
    );
    commit(&repo, dir.path(), "a.txt", "x", "feat: add feature");

    // Prepare creates a release branch
    let (_, stderr) = run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "prepare"]));
    assert!(stderr.contains("Release branch 'release-ratchet--release' is ready"));

    // We should be on the release branch
    let head = repo.head().unwrap();
    assert_eq!(head.shorthand().unwrap(), "release-ratchet--release");

    // Switch back to main (simulating the merge)
    {
        // Get the release branch's HEAD (the release commit)
        let release_commit = repo.head().unwrap().peel_to_commit().unwrap();

        // Checkout main
        let main_ref = repo.find_reference("refs/heads/main").unwrap();
        let main_obj = main_ref.peel(git2::ObjectType::Commit).unwrap();
        repo.checkout_tree(&main_obj, Some(git2::build::CheckoutBuilder::new().force())).unwrap();
        repo.set_head("refs/heads/main").unwrap();

        // Create a merge commit on main
        let main_commit = repo.head().unwrap().peel_to_commit().unwrap();
        let sig = Signature::now("Test User", "test@example.com").unwrap();
        let mut index = repo.index().unwrap();
        index.read(true).unwrap();
        // Read tree from release commit to get the changelog etc.
        let release_tree = release_commit.tree().unwrap();
        repo.checkout_tree(
            release_tree.as_object(),
            Some(git2::build::CheckoutBuilder::new().force()),
        ).unwrap();
        index.read_tree(&release_tree).unwrap();
        index.write().unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        repo.commit(
            Some("HEAD"), &sig, &sig,
            "Merge branch 'release-ratchet--release'",
            &tree,
            &[&main_commit, &release_commit],
        ).unwrap();
    }

    // Release should detect the version from the merged branch's commit
    let (_, stderr) = run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "release"]));
    assert!(stderr.contains("Created tag 'v0.1.0'"));
}

// ============================================================================
// E2E: Custom release branch name
// ============================================================================

#[test]
fn e2e_custom_branch_name() {
    let (dir, repo) = setup_with_config(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n",
    );
    commit(&repo, dir.path(), "a.txt", "x", "feat: add feature");

    run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "prepare", "--branch", "release/v0.1.0"]));

    let head = repo.head().unwrap();
    assert_eq!(head.shorthand().unwrap(), "release/v0.1.0");
}

// ============================================================================
// E2E: Dirty working tree rejection
// ============================================================================

#[test]
fn e2e_dirty_tree_rejected() {
    let (dir, repo) = setup_with_config(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n",
    );
    commit(&repo, dir.path(), "a.txt", "x", "feat: add feature");

    // Create a dirty CHANGELOG.md
    std::fs::write(dir.path().join("CHANGELOG.md"), "dirty content").unwrap();
    // Stage it so it's tracked, then modify it again so it's wt_modified
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
    // Now modify it so it's dirty
    std::fs::write(dir.path().join("CHANGELOG.md"), "modified dirty content").unwrap();

    let (code, _, stderr) = run_fail(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    assert_eq!(code, 1);
    assert!(stderr.contains("uncommitted changes"));
    assert!(stderr.contains("CHANGELOG.md"));
}

// ============================================================================
// E2E: Re-running prepare overwrites previous release branch
// ============================================================================

#[test]
fn e2e_rerun_prepare_overwrites_branch() {
    let (dir, repo) = setup_with_config(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n",
    );
    commit(&repo, dir.path(), "a.txt", "x", "feat: add feature");

    // First prepare
    run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "prepare"]));
    assert_eq!(repo.head().unwrap().shorthand().unwrap(), "release-ratchet--release");

    // Go back to main and add another commit
    {
        let main_ref = repo.find_reference("refs/heads/main").unwrap();
        let main_obj = main_ref.peel(git2::ObjectType::Commit).unwrap();
        repo.checkout_tree(&main_obj, Some(git2::build::CheckoutBuilder::new().force())).unwrap();
        repo.set_head("refs/heads/main").unwrap();
    }
    commit(&repo, dir.path(), "b.txt", "y", "fix: another fix");

    // Second prepare should succeed (force-recreates the branch)
    run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "prepare"]));

    // Changelog should include the fix
    let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();
    assert!(changelog.contains("another fix"));
}

// ============================================================================
// E2E: Only fix commits → patch bump
// ============================================================================

#[test]
fn e2e_only_fixes_is_patch() {
    let (dir, repo) = setup_with_config(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n",
    );
    commit(&repo, dir.path(), "a.txt", "x", "fix: fix one");
    commit(&repo, dir.path(), "b.txt", "y", "fix: fix two");
    commit(&repo, dir.path(), "c.txt", "z", "perf: speed up thing");

    let (stdout, _) = run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "status", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["bump_level"], "patch");
    assert_eq!(json["next_version"], "0.0.1");
}

// ============================================================================
// E2E: Mixed conventional and non-conventional commits
// ============================================================================

#[test]
fn e2e_mixed_conventional_and_nonconventional() {
    let (dir, repo) = setup_with_config(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n",
    );
    commit(&repo, dir.path(), "a.txt", "x", "feat: add feature");
    commit(&repo, dir.path(), "b.txt", "y", "WIP stuff");
    commit(&repo, dir.path(), "c.txt", "z", "update readme");
    commit(&repo, dir.path(), "d.txt", "w", "fix: important fix");

    let (stdout, _) = run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "status", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["conventional_commits"], 4); // initial + config + feat + fix
    assert_eq!(json["non_conventional_commits"], 2); // WIP + update
    assert_eq!(json["bump_level"], "minor"); // feat wins over fix

    // prepare should succeed despite non-conventional commits
    run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "prepare", "--dry-run"]));
}

// ============================================================================
// E2E: Non-standard tag prefix
// ============================================================================

#[test]
fn e2e_custom_tag_prefix_full_cycle() {
    let (dir, repo) = setup_with_config(
        "tag_prefix: \"myapp-v\"\nmain_branch: \"main\"\necosystems: []\n",
    );
    commit(&repo, dir.path(), "a.txt", "x", "feat: feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));

    assert!(repo.refname_to_id("refs/tags/myapp-v0.1.0").is_ok());

    // Next cycle should find the tag
    commit(&repo, dir.path(), "b.txt", "y", "fix: fix");
    let (stdout, _) = run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "status", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["last_version"], "0.1.0");
    assert_eq!(json["next_version"], "0.1.1");
}

// ============================================================================
// E2E: Validate range of commits
// ============================================================================

#[test]
fn e2e_validate_all_commits_valid() {
    let (dir, repo) = setup_with_config(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n",
    );
    commit(&repo, dir.path(), "a.txt", "x", "feat: feature A");
    commit(&repo, dir.path(), "b.txt", "y", "fix: bug B");

    run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "validate"]));
}

#[test]
fn e2e_validate_with_invalid_commits_fails() {
    let (dir, repo) = setup_with_config(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n",
    );
    commit(&repo, dir.path(), "a.txt", "x", "feat: feature A");
    commit(&repo, dir.path(), "b.txt", "y", "yolo deploy friday");

    let (code, _, stderr) = run_fail(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "validate"]));
    assert_eq!(code, 3);
    assert!(stderr.contains("NOT valid"));
}

// ============================================================================
// E2E: release --release-version override
// ============================================================================

#[test]
fn e2e_release_version_override() {
    let (dir, repo) = setup_with_config(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n",
    );
    commit(&repo, dir.path(), "a.txt", "x", "feat: feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));

    // Override the version at release time
    let (_, stderr) = run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "release", "--release-version", "0.1.0"]));
    assert!(stderr.contains("Created tag 'v0.1.0'"));
}

// ============================================================================
// E2E: prepare --release-version override
// ============================================================================

#[test]
fn e2e_prepare_version_override() {
    let (dir, repo) = setup_with_config(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n",
    );
    commit(&repo, dir.path(), "a.txt", "x", "fix: small fix");

    let (_, stderr) = run_ok(binary()
        .args([
            "--repo", dir.path().to_str().unwrap(),
            "prepare", "--no-branch", "--release-version", "5.0.0",
        ]));
    assert!(stderr.contains("5.0.0 (manual override)"));

    // The release commit should reference v5.0.0
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert!(head.message().unwrap().contains("v5.0.0"));
}

// ============================================================================
// E2E: Changelog accumulates correctly across multiple releases
// ============================================================================

#[test]
fn e2e_changelog_accumulation() {
    let (dir, repo) = setup_with_config(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n",
    );

    // Release 1
    commit(&repo, dir.path(), "a.txt", "1", "feat: alpha feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));

    // Release 2
    commit(&repo, dir.path(), "b.txt", "2", "fix: beta fix");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));

    let changelog = std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();

    // Both versions present
    assert!(changelog.contains("## [0.1.0]"));
    assert!(changelog.contains("## [0.1.1]"));

    // Both commit descriptions present
    assert!(changelog.contains("alpha feature"));
    assert!(changelog.contains("beta fix"));

    // Header present
    assert!(changelog.starts_with("# Changelog"));

    // "alpha feature" should be under 0.1.0, not 0.1.1
    let v011_pos = changelog.find("## [0.1.1]").unwrap();
    let v010_pos = changelog.find("## [0.1.0]").unwrap();
    let alpha_pos = changelog.find("alpha feature").unwrap();
    let beta_pos = changelog.find("beta fix").unwrap();
    assert!(beta_pos > v011_pos && beta_pos < v010_pos, "beta fix should be in 0.1.1 section");
    assert!(alpha_pos > v010_pos, "alpha feature should be in 0.1.0 section");
}
