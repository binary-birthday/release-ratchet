//! Monorepo functional tests.

mod common;
use common::*;
use std::path::Path;
use git2::Repository;

const MONOREPO_CONFIG: &str = r#"
tag_prefix = "v"
main_branch = "main"

[[packages]]
name = "core"
path = "packages/core"
tag_prefix = "core-v"

[[packages.ecosystems]]
type = "cargo"
path = "packages/core/Cargo.toml"

[[packages]]
name = "cli"
path = "packages/cli"
tag_prefix = "cli-v"

[[packages.ecosystems]]
type = "cargo"
path = "packages/cli/Cargo.toml"
"#;

fn setup_monorepo() -> (tempfile::TempDir, Repository) {
    let (dir, repo) = init_repo();

    // Create package directories and Cargo.toml files
    std::fs::create_dir_all(dir.path().join("packages/core/src")).unwrap();
    std::fs::create_dir_all(dir.path().join("packages/cli/src")).unwrap();
    std::fs::write(
        dir.path().join("packages/core/Cargo.toml"),
        "[package]\nname = \"core\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    ).unwrap();
    std::fs::write(
        dir.path().join("packages/cli/Cargo.toml"),
        "[package]\nname = \"cli\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    ).unwrap();
    std::fs::write(dir.path().join(".release-ratchet.toml"), MONOREPO_CONFIG).unwrap();

    // Commit everything
    {
        let mut index = repo.index().unwrap();
        index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None).unwrap();
        index.write().unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let sig = git2::Signature::now("Test User", "test@example.com").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "chore: setup monorepo", &tree, &[&head]).unwrap();
    }

    (dir, repo)
}

fn commit_to_package(repo: &Repository, dir: &Path, pkg: &str, filename: &str, content: &str, message: &str) {
    let file_path = dir.join(format!("packages/{pkg}/src/{filename}"));
    std::fs::write(&file_path, content).unwrap();
    let mut index = repo.index().unwrap();
    index.read(true).unwrap();
    index.add_path(Path::new(&format!("packages/{pkg}/src/{filename}"))).unwrap();
    index.write().unwrap();
    let sig = git2::Signature::now("Test User", "test@example.com").unwrap();
    let tree_oid = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_oid).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[&head]).unwrap();
}

// ============================================================================
// Status
// ============================================================================

#[test]
fn monorepo_status_shows_all_packages() {
    let (dir, repo) = setup_monorepo();
    commit_to_package(&repo, dir.path(), "core", "lib.rs", "fn core() {}", "feat: core feature");
    commit_to_package(&repo, dir.path(), "cli", "main.rs", "fn main() {}", "fix: cli fix");

    let (stdout, _) = run_ok(binary().args([
        "--repo", dir.path().to_str().unwrap(), "status", "--json",
    ]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["package"], "core");
    assert_eq!(arr[0]["bump_level"], "minor");
    assert_eq!(arr[1]["package"], "cli");
    assert_eq!(arr[1]["bump_level"], "patch");
}

#[test]
fn monorepo_status_single_package() {
    let (dir, repo) = setup_monorepo();
    commit_to_package(&repo, dir.path(), "core", "lib.rs", "fn core() {}", "feat: core feature");

    let (stdout, _) = run_ok(binary().args([
        "--repo", dir.path().to_str().unwrap(), "-p", "core", "status", "--json",
    ]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["package"], "core");
}

// ============================================================================
// Prepare + Release cycle
// ============================================================================

#[test]
fn monorepo_prepare_and_release() {
    let (dir, repo) = setup_monorepo();
    let r = dir.path().to_str().unwrap();

    commit_to_package(&repo, dir.path(), "core", "lib.rs", "fn core() {}", "feat: core feature");
    commit_to_package(&repo, dir.path(), "cli", "main.rs", "fn main() {}", "fix: cli bugfix");

    // Prepare
    let (_, stderr) = run_ok(binary().args(["--repo", r, "prepare", "--no-branch"]));
    assert!(stderr.contains("core-v0.1.0"), "expected core-v0.1.0: {stderr}");
    assert!(stderr.contains("cli-v0.0.1"), "expected cli-v0.0.1: {stderr}");

    // Verify changelogs created
    let core_cl = std::fs::read_to_string(dir.path().join("packages/core/CHANGELOG.md")).unwrap();
    assert!(core_cl.contains("## [0.1.0]"));
    assert!(core_cl.contains("core feature"));

    let cli_cl = std::fs::read_to_string(dir.path().join("packages/cli/CHANGELOG.md")).unwrap();
    assert!(cli_cl.contains("## [0.0.1]"));
    assert!(cli_cl.contains("cli bugfix"));

    // Verify version files bumped
    let core_cargo = std::fs::read_to_string(dir.path().join("packages/core/Cargo.toml")).unwrap();
    assert!(core_cargo.contains("version = \"0.1.0\""));
    let cli_cargo = std::fs::read_to_string(dir.path().join("packages/cli/Cargo.toml")).unwrap();
    assert!(cli_cargo.contains("version = \"0.0.1\""));

    // Release
    let (_, stderr) = run_ok(binary().args(["--repo", r, "release"]));
    assert!(stderr.contains("Created tag 'core-v0.1.0'"));
    assert!(stderr.contains("Created tag 'cli-v0.0.1'"));

    // Tags exist
    let repo = Repository::open(dir.path()).unwrap();
    assert!(repo.refname_to_id("refs/tags/core-v0.1.0").is_ok());
    assert!(repo.refname_to_id("refs/tags/cli-v0.0.1").is_ok());
}

#[test]
fn monorepo_commit_only_affects_touched_package() {
    let (dir, repo) = setup_monorepo();
    let r = dir.path().to_str().unwrap();

    // Only commit to core
    commit_to_package(&repo, dir.path(), "core", "lib.rs", "fn core() {}", "feat: core only");

    let (_, stderr) = run_ok(binary().args(["--repo", r, "prepare", "--dry-run"]));
    // core should bump, cli should not appear
    assert!(stderr.contains("[core]"));
    assert!(!stderr.contains("[cli]"));
}

#[test]
fn monorepo_cross_package_commit_attributed_to_both() {
    let (dir, repo) = setup_monorepo();
    let r = dir.path().to_str().unwrap();

    // Commit touching both packages
    std::fs::write(dir.path().join("packages/core/src/shared.rs"), "// shared").unwrap();
    std::fs::write(dir.path().join("packages/cli/src/shared.rs"), "// shared").unwrap();
    let mut index = repo.index().unwrap();
    index.read(true).unwrap();
    index.add_path(Path::new("packages/core/src/shared.rs")).unwrap();
    index.add_path(Path::new("packages/cli/src/shared.rs")).unwrap();
    index.write().unwrap();
    let sig = git2::Signature::now("Test User", "test@example.com").unwrap();
    let tree_oid = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_oid).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "feat: cross-package feature", &tree, &[&head]).unwrap();

    let (stdout, _) = run_ok(binary().args(["--repo", r, "status", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let arr = json.as_array().unwrap();
    // Both packages should see the commit
    assert_eq!(arr[0]["bump_level"], "minor");
    assert_eq!(arr[1]["bump_level"], "minor");
}

// ============================================================================
// Shared paths
// ============================================================================

#[test]
fn monorepo_shared_path_attributes_to_affected_packages() {
    let (dir, repo) = init_repo();

    let config = r#"
tag_prefix = "v"
main_branch = "main"

[[packages]]
name = "core"
path = "packages/core"
tag_prefix = "core-v"

[[packages]]
name = "cli"
path = "packages/cli"
tag_prefix = "cli-v"

[[shared_paths]]
path = "utils"
affects = ["core", "cli"]
"#;
    std::fs::create_dir_all(dir.path().join("packages/core")).unwrap();
    std::fs::create_dir_all(dir.path().join("packages/cli")).unwrap();
    std::fs::create_dir_all(dir.path().join("utils")).unwrap();
    std::fs::write(dir.path().join(".release-ratchet.toml"), config).unwrap();
    {
        let mut index = repo.index().unwrap();
        index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None).unwrap();
        index.write().unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let sig = git2::Signature::now("Test User", "test@example.com").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "chore: setup", &tree, &[&head]).unwrap();
    }

    // Commit to utils only
    std::fs::write(dir.path().join("utils/helper.rs"), "fn help() {}").unwrap();
    commit(&repo, dir.path(), "utils/helper.rs", "fn help() {}", "feat: add util helper");

    let (stdout, _) = run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "status", "--json"]));
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let arr = json.as_array().unwrap();
    // Both core and cli should see the feat from utils
    assert_eq!(arr[0]["bump_level"], "minor", "core should see utils change");
    assert_eq!(arr[1]["bump_level"], "minor", "cli should see utils change");
}

// ============================================================================
// Notes + Check + Bump
// ============================================================================

#[test]
fn monorepo_notes_per_package() {
    let (dir, repo) = setup_monorepo();
    let r = dir.path().to_str().unwrap();

    commit_to_package(&repo, dir.path(), "core", "lib.rs", "fn core() {}", "feat: core feature");

    let (stdout, _) = run_ok(binary().args(["--repo", r, "-p", "core", "notes"]));
    assert!(stdout.contains("## [0.1.0]"));
    assert!(stdout.contains("core feature"));
}

#[test]
fn monorepo_check_after_release() {
    let (dir, repo) = setup_monorepo();
    let r = dir.path().to_str().unwrap();

    commit_to_package(&repo, dir.path(), "core", "lib.rs", "fn core() {}", "feat: core feature");
    run_ok(binary().args(["--repo", r, "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", r, "release"]));

    // Should pass for core (has tag + changelog + matching version)
    run_ok(binary().args(["--repo", r, "-p", "core", "check"]));
}

#[test]
fn monorepo_bump_single_package() {
    let (dir, repo) = setup_monorepo();
    let r = dir.path().to_str().unwrap();

    commit_to_package(&repo, dir.path(), "core", "lib.rs", "fn core() {}", "feat: core feature");

    run_ok(binary().args(["--repo", r, "-p", "core", "bump"]));

    let core_cargo = std::fs::read_to_string(dir.path().join("packages/core/Cargo.toml")).unwrap();
    assert!(core_cargo.contains("version = \"0.1.0\""), "core not bumped");

    // cli should be untouched
    let cli_cargo = std::fs::read_to_string(dir.path().join("packages/cli/Cargo.toml")).unwrap();
    assert!(cli_cargo.contains("version = \"0.0.0\""), "cli should not be bumped");
}

#[test]
fn monorepo_second_release_cycle() {
    let (dir, repo) = setup_monorepo();
    let r = dir.path().to_str().unwrap();

    // First cycle
    commit_to_package(&repo, dir.path(), "core", "lib.rs", "fn v1() {}", "feat: core v1");
    run_ok(binary().args(["--repo", r, "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", r, "release"]));

    // Second cycle: only core changes
    drop(repo);
    let repo = Repository::open(dir.path()).unwrap();
    commit_to_package(&repo, dir.path(), "core", "lib.rs", "fn v2() {}", "fix: core fix");

    let (_, stderr) = run_ok(binary().args(["--repo", r, "prepare", "--no-branch"]));
    assert!(stderr.contains("core-v0.1.1"), "expected core-v0.1.1: {stderr}");
    // cli should NOT appear in the release (no changes)
    assert!(!stderr.contains("cli-v"), "cli should not be released: {stderr}");
}
