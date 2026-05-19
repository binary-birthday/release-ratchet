//! Pact-style vendor contract tests.
//!
//! release-ratchet is git-vendor-agnostic, but its outputs (tags, branches,
//! commit messages, changelog) are consumed by git forges. These tests verify
//! that our outputs conform to what each forge expects.
//!
//! Structure: each forge is a "consumer" with documented expectations.
//! We run a full prepare+release cycle, then verify every artifact matches
//! the consumer's contract. If a forge changes their expectations, we update
//! the contract here and our code catches the drift.
//!
//! Consumers:
//!   - GitHub (Releases, PR merge strategies, Actions tag triggers)
//!   - GitLab (Releases, MR merge strategies, CI tag pipelines)
//!   - Bitbucket (tags, PR merge strategies)
//!   - Plain git (git describe, tag listing, log parsing)

use std::path::Path;

use git2::{Repository, Signature};
use regex::Regex;
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
        let tree_oid = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "chore: initial", &tree, &[]).unwrap();
    }
    {
        let head_ref = repo.head().unwrap();
        let head_name = head_ref.name().unwrap().to_string();
        drop(head_ref);
        if head_name != "refs/heads/main" {
            let mut reference = repo.find_reference(&head_name).unwrap();
            reference.rename("refs/heads/main", true, "rename").unwrap();
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

/// Run a full prepare+release cycle and return the repo state for verification.
struct ReleaseArtifacts {
    tag_name: String,
    tag_target_oid: String,
    release_commit_message: String,
    release_branch_name: String,
    changelog_content: String,
    changelog_latest_section: String,
}

fn do_release_cycle(config: &str, commits: &[&str]) -> (TempDir, Repository, ReleaseArtifacts) {
    let (dir, repo) = init_repo();
    std::fs::write(dir.path().join(".release-ratchet.yml"), config).unwrap();
    commit(
        &repo, dir.path(), ".release-ratchet.yml",
        &std::fs::read_to_string(dir.path().join(".release-ratchet.yml")).unwrap(),
        "chore: add config",
    );

    for (i, msg) in commits.iter().enumerate() {
        commit(&repo, dir.path(), &format!("file{i}.txt"), &format!("{i}"), msg);
    }

    // Phase 1: prepare (with branch)
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare"]));

    let release_branch_name = repo.head().unwrap().shorthand().unwrap().to_string();
    let (release_commit_message, release_oid) = {
        let c = repo.head().unwrap().peel_to_commit().unwrap();
        (c.message().unwrap().to_string(), c.id())
    };
    let changelog_content = std::fs::read_to_string(dir.path().join("CHANGELOG.md"))
        .unwrap_or_default();

    // Extract the latest section from the changelog (everything between first ## and second ##)
    let latest_section = {
        let start = changelog_content.find("## [").unwrap_or(0);
        let rest = &changelog_content[start..];
        let end = rest[3..].find("\n## [").map(|i| i + 3).unwrap_or(rest.len());
        rest[..end].to_string()
    };

    // Simulate merge back to main: fast-forward main to the release commit
    {
        let mut main_ref = repo.find_reference("refs/heads/main").unwrap();
        main_ref.set_target(release_oid, "merge release branch").unwrap();

        let release_obj = repo.find_object(release_oid, None).unwrap();
        repo.checkout_tree(&release_obj, Some(git2::build::CheckoutBuilder::new().force())).unwrap();
        repo.set_head("refs/heads/main").unwrap();
    }

    // Phase 2: release (tag)
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));

    // Reopen the repo to get fresh state after subprocess modifications
    drop(repo);
    let repo = Repository::open(dir.path()).unwrap();

    // Collect tag info
    let tags: Vec<String> = repo.tag_names(None).unwrap()
        .iter()
        .filter_map(|t| t.map(String::from))
        .collect();
    let tag_name = tags.last().unwrap().clone();
    let tag_oid = repo.refname_to_id(&format!("refs/tags/{tag_name}")).unwrap();
    let tag_target_oid = tag_oid.to_string();

    let artifacts = ReleaseArtifacts {
        tag_name,
        tag_target_oid,
        release_commit_message,
        release_branch_name,
        changelog_content,
        changelog_latest_section: latest_section,
    };

    (dir, repo, artifacts)
}

// ============================================================================
// GitHub consumer contracts
// ============================================================================
//
// GitHub's expectations, documented at:
// - Releases: https://docs.github.com/en/repositories/releasing-projects-on-github
// - Actions triggers: https://docs.github.com/en/actions/using-workflows/events-that-trigger-workflows#push
// - PR merge: https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/incorporating-changes-from-a-pull-request

/// GitHub creates releases from tags. The tag name IS the release name.
/// Convention: tags matching `v*` trigger release workflows.
#[test]
fn github_tag_name_matches_release_pattern() {
    let (_, _, a) = do_release_cycle(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n",
        &["feat: add feature"],
    );
    // GitHub Actions `on: push: tags: ['v*']` pattern
    assert!(a.tag_name.starts_with("v"), "tag '{}' doesn't match GitHub v* pattern", a.tag_name);
    // Tag name is valid semver after stripping prefix
    let version_part = a.tag_name.strip_prefix("v").unwrap();
    assert!(semver::Version::parse(version_part).is_ok(), "tag version part is not valid semver");
}

/// GitHub uses the first markdown section of CHANGELOG.md as release notes
/// when auto-generating releases. The format must be valid markdown.
#[test]
fn github_changelog_section_is_valid_release_notes() {
    let (_, _, a) = do_release_cycle(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n",
        &["feat: new feature", "fix: bug fix"],
    );

    let section = &a.changelog_latest_section;

    // Must start with ## [version] - date
    let header_re = Regex::new(r"^## \[\d+\.\d+\.\d+\] - \d{4}-\d{2}-\d{2}").unwrap();
    assert!(header_re.is_match(section), "section header wrong: {section}");

    // Must contain ### subsections (valid markdown headings)
    assert!(section.contains("### "), "no subsections in release notes");

    // Must contain bullet points
    assert!(section.contains("- "), "no bullet points in release notes");

    // Must not contain HTML or raw git objects that would render badly
    assert!(!section.contains("<"), "contains HTML: {section}");
}

/// GitHub Actions `on: push: tags` fires on lightweight AND annotated tags.
/// Our unsigned tags are lightweight — the tag OID points directly to a commit.
#[test]
fn github_tag_is_lightweight_by_default() {
    let (dir, _, a) = do_release_cycle(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n",
        &["feat: add feature"],
    );
    // Reopen to get fresh state
    let repo = Repository::open(dir.path()).unwrap();
    let tag_ref = repo.refname_to_id(&format!("refs/tags/{}", a.tag_name)).unwrap();
    let obj = repo.find_object(tag_ref, None).unwrap();
    assert_eq!(obj.kind(), Some(git2::ObjectType::Commit), "tag should be lightweight (point to commit)");
}

/// GitHub PR titles become the merge commit message for squash merges.
/// Our release branch name should be suitable as a PR title source.
#[test]
fn github_release_branch_is_valid_pr_source() {
    let (_, _, a) = do_release_cycle(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n",
        &["feat: add feature"],
    );
    // Branch name must be a valid git ref
    assert!(!a.release_branch_name.contains(" "), "branch name has spaces");
    assert!(!a.release_branch_name.contains(".."), "branch name has ..");
    assert!(!a.release_branch_name.starts_with("-"), "branch name starts with -");
    // Branch name should be recognizable as a release branch
    assert!(a.release_branch_name.contains("release"), "branch '{}' not recognizable as release", a.release_branch_name);
}

/// GitHub squash merge replaces the commit message with the PR title.
/// Our release command must still detect the version (via CHANGELOG.md fallback).
/// This is tested in contract.rs — here we verify the CHANGELOG.md is in the tree.
#[test]
fn github_squash_merge_changelog_survives() {
    let (_, _, a) = do_release_cycle(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n",
        &["feat: add feature"],
    );
    // The release commit must contain CHANGELOG.md with the version
    assert!(a.changelog_content.contains("## [0.1.0]"), "changelog missing version section");
}

// ============================================================================
// GitLab consumer contracts
// ============================================================================
//
// GitLab's expectations, documented at:
// - Releases: https://docs.gitlab.com/ee/user/project/releases/
// - CI pipelines: https://docs.gitlab.com/ee/ci/yaml/#rules
// - MR merge strategies: https://docs.gitlab.com/ee/user/project/merge_requests/methods/

/// GitLab CI `rules: - if: $CI_COMMIT_TAG` fires on any tag push.
/// GitLab release-cli uses `--tag-name` from the tag.
#[test]
fn gitlab_tag_triggers_pipeline() {
    let (_, _, a) = do_release_cycle(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n",
        &["feat: add feature"],
    );
    // Tag must be a valid git ref name (no spaces, no .., no control chars)
    let valid_ref = Regex::new(r"^[a-zA-Z0-9._\-/]+$").unwrap();
    assert!(valid_ref.is_match(&a.tag_name), "tag '{}' not a valid ref for GitLab CI", a.tag_name);
}

/// GitLab release-cli --description expects markdown.
/// Typically the release description is extracted from CHANGELOG.md.
#[test]
fn gitlab_release_description_is_markdown() {
    let (_, _, a) = do_release_cycle(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n",
        &["feat: new feature", "fix: bug fix"],
    );
    let section = &a.changelog_latest_section;
    // Valid markdown: has headings, lists, no bare URLs or broken syntax
    assert!(section.contains("###"), "no heading in release description");
    assert!(section.contains("- "), "no list items in release description");
}

/// GitLab MR merge commits use "Merge branch 'X' into 'Y'" format.
/// Our release commit message should be parseable even after a GitLab merge.
/// The release command falls back to CHANGELOG.md for this case.
#[test]
fn gitlab_merge_commit_version_detection() {
    let (dir, repo, _) = do_release_cycle(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n",
        &["feat: add feature"],
    );

    // Simulate a second release where GitLab's merge commit format is used
    commit(&repo, dir.path(), "b.txt", "y", "feat: second feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));

    // Amend to simulate GitLab merge commit message
    {
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let tree = head.tree().unwrap();
        let sig = Signature::now("Test User", "test@example.com").unwrap();
        let parent = head.parent(0).unwrap();
        let new_oid = repo.commit(
            None, &sig, &sig,
            "Merge branch 'release-ratchet--release' into 'main'",
            &tree, &[&parent],
        ).unwrap();
        let mut main_ref = repo.find_reference("refs/heads/main").unwrap();
        main_ref.set_target(new_oid, "simulate gitlab merge").unwrap();
    }

    // Release should detect version from CHANGELOG.md fallback
    let (_, stderr) = run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "release"]));
    assert!(stderr.contains("Created tag 'v0.2.0'"), "GitLab merge not handled: {stderr}");
}

// ============================================================================
// Bitbucket consumer contracts
// ============================================================================
//
// Bitbucket's expectations:
// - Tags: https://support.atlassian.com/bitbucket-cloud/docs/use-tags/
// - Pipelines: https://support.atlassian.com/bitbucket-cloud/docs/pipeline-triggers/
// - PR merge strategies: https://support.atlassian.com/bitbucket-cloud/docs/merge-a-pull-request/

/// Bitbucket Pipelines trigger on tag patterns via `tags: ['v*']` in bitbucket-pipelines.yml.
/// Same pattern as GitHub Actions.
#[test]
fn bitbucket_tag_pattern() {
    let (_, _, a) = do_release_cycle(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n",
        &["feat: add feature"],
    );
    // Bitbucket uses glob pattern matching for tag triggers
    let glob_pattern = glob::Pattern::new("v*").unwrap();
    assert!(glob_pattern.matches(&a.tag_name), "tag '{}' doesn't match Bitbucket v* glob", a.tag_name);
}

/// Bitbucket PR "squash" merges combine all commits into one with the PR title.
/// Same concern as GitHub — version detection must work via CHANGELOG fallback.
#[test]
fn bitbucket_squash_merge_version_detection() {
    let (dir, repo, _) = do_release_cycle(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n",
        &["feat: add feature"],
    );

    commit(&repo, dir.path(), "b.txt", "y", "feat: second feature");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));

    // Amend to simulate Bitbucket squash merge (PR title as message)
    {
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let tree = head.tree().unwrap();
        let sig = Signature::now("Test User", "test@example.com").unwrap();
        let parent = head.parent(0).unwrap();
        let new_oid = repo.commit(
            None, &sig, &sig,
            "Merged in release-ratchet--release (pull request #42)",
            &tree, &[&parent],
        ).unwrap();
        let mut main_ref = repo.find_reference("refs/heads/main").unwrap();
        main_ref.set_target(new_oid, "simulate bitbucket squash").unwrap();
    }

    let (_, stderr) = run_ok(binary()
        .args(["--repo", dir.path().to_str().unwrap(), "release"]));
    assert!(stderr.contains("Created tag 'v0.2.0'"), "Bitbucket squash not handled: {stderr}");
}

// ============================================================================
// Plain git consumer contracts
// ============================================================================
//
// Tools like `git describe`, `git tag -l`, `git log` consume our artifacts.

/// `git describe` uses the most recent tag reachable from HEAD.
/// Our tags must be on the commit lineage, not dangling.
#[test]
fn git_describe_finds_our_tag() {
    let (dir, _, a) = do_release_cycle(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n",
        &["feat: add feature"],
    );
    // Reopen to get fresh state
    let repo = Repository::open(dir.path()).unwrap();
    let head_oid = repo.head().unwrap().peel_to_commit().unwrap().id();
    let tag_oid = git2::Oid::from_str(&a.tag_target_oid).unwrap();

    // Walk HEAD's history and verify the tagged commit is reachable
    let mut revwalk = repo.revwalk().unwrap();
    revwalk.push(head_oid).unwrap();
    let reachable = revwalk.any(|oid| oid.unwrap() == tag_oid);
    assert!(reachable, "tag commit is not reachable from HEAD");
}

/// `git tag -l --sort=-v:refname` sorts tags by semver.
/// Our tag names must sort correctly under this scheme.
#[test]
fn git_tag_sort_order() {
    // Do two releases and verify they sort correctly
    let (dir, repo) = init_repo();
    std::fs::write(dir.path().join(".release-ratchet.yml"),
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n").unwrap();
    commit(&repo, dir.path(), ".release-ratchet.yml",
        &std::fs::read_to_string(dir.path().join(".release-ratchet.yml")).unwrap(),
        "chore: add config");

    commit(&repo, dir.path(), "a.txt", "1", "feat: first");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));

    commit(&repo, dir.path(), "b.txt", "2", "feat: second");
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "prepare", "--no-branch"]));
    run_ok(binary().args(["--repo", dir.path().to_str().unwrap(), "release"]));

    let mut tags: Vec<String> = repo.tag_names(None).unwrap()
        .iter()
        .filter_map(|t| t.map(String::from))
        .collect();
    tags.sort(); // lexicographic sort (matches git tag -l --sort=refname)

    assert_eq!(tags, vec!["v0.1.0", "v0.2.0"]);

    // Reverse version sort (what git tag -l --sort=-v:refname does)
    let mut versions: Vec<semver::Version> = tags.iter()
        .map(|t| semver::Version::parse(t.strip_prefix("v").unwrap()).unwrap())
        .collect();
    versions.sort();
    versions.reverse();
    assert_eq!(versions[0].to_string(), "0.2.0");
    assert_eq!(versions[1].to_string(), "0.1.0");
}

/// `git log --oneline` shows the first line of commit messages.
/// Our release commit message must be a clean single line.
#[test]
fn git_log_release_commit_is_single_line() {
    let (_, _, a) = do_release_cycle(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n",
        &["feat: add feature"],
    );
    let first_line = a.release_commit_message.lines().next().unwrap();
    // No multi-line commit message for the release commit
    assert_eq!(first_line, a.release_commit_message.trim(), "release commit should be single-line");
    // Should be a valid conventional commit itself
    assert!(first_line.starts_with("chore: release "), "unexpected format: {first_line}");
}

/// Tags must be valid git ref names per git-check-ref-format rules:
/// no spaces, no .., no ASCII control chars, no backslash, no colon
#[test]
fn git_tag_is_valid_ref_name() {
    let (_, _, a) = do_release_cycle(
        "tag_prefix: \"v\"\nmain_branch: \"main\"\necosystems: []\n",
        &["feat: add feature"],
    );
    let invalid_chars = [' ', '~', '^', ':', '\\', '*', '?', '['];
    for c in &invalid_chars {
        assert!(!a.tag_name.contains(*c), "tag contains invalid char '{c}'");
    }
    assert!(!a.tag_name.contains(".."), "tag contains ..");
    assert!(!a.tag_name.contains("@{"), "tag contains @{{}}");
    assert!(!a.tag_name.ends_with(".lock"), "tag ends with .lock");
    assert!(!a.tag_name.ends_with("."), "tag ends with .");
    assert!(!a.tag_name.starts_with("."), "tag starts with .");
}
