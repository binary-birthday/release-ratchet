use std::path::Path;

use anyhow::{Context, Result};
use semver::Version;

use crate::cli::BackportArgs;
use crate::config::Config;
use crate::git::repo;

pub fn execute(repo_path: &Path, config: &Config, args: BackportArgs) -> Result<()> {
    let repository = repo::open(repo_path).context("failed to open repository")?;

    // 1. Resolve the target: tag or existing branch
    let (target_oid, branch_name) = resolve_target(&repository, config, &args)?;

    // 2. Resolve all commits to cherry-pick
    let commit_oids: Vec<(git2::Oid, String)> = args
        .commits
        .iter()
        .map(|c| {
            let oid = repo::resolve_ref(&repository, c)
                .context(format!("failed to resolve commit '{c}'"))?;
            let commit = repository.find_commit(oid)?;
            let summary = commit
                .message()
                .unwrap_or("")
                .lines()
                .next()
                .unwrap_or("")
                .to_string();
            Ok((oid, summary))
        })
        .collect::<Result<Vec<_>>>()?;

    if args.dry_run {
        eprintln!("--- DRY RUN ---");
        eprintln!("Would create/checkout branch '{branch_name}' from {}", crate::git::repo::short_oid(target_oid));
        for (oid, summary) in &commit_oids {
            eprintln!("Would cherry-pick {} {summary}", crate::git::repo::short_oid(*oid));
        }
        eprintln!(
            "\nAfter cherry-pick, run:\n  \
             release-ratchet prepare --no-branch\n  \
             release-ratchet release"
        );
        return Ok(());
    }

    // 3. Create or checkout the maintenance branch
    let branch_exists = repository
        .find_branch(&branch_name, git2::BranchType::Local)
        .is_ok();

    if branch_exists {
        // Checkout existing branch
        let refname = format!("refs/heads/{branch_name}");
        let obj = repository.revparse_single(&refname)?;
        repository.checkout_tree(
            &obj,
            Some(git2::build::CheckoutBuilder::new().safe()),
        )?;
        repository.set_head(&refname)?;
        eprintln!("Checked out existing branch '{branch_name}'");
    } else {
        // Create branch from the target commit
        let target_commit = repository.find_commit(target_oid)?;
        repository.branch(&branch_name, &target_commit, false)?;
        let refname = format!("refs/heads/{branch_name}");
        let obj = repository.revparse_single(&refname)?;
        repository.checkout_tree(
            &obj,
            Some(git2::build::CheckoutBuilder::new().safe()),
        )?;
        repository.set_head(&refname)?;
        eprintln!("Created branch '{branch_name}' from {}", crate::git::repo::short_oid(target_oid));
    }

    // 4. Cherry-pick each commit
    for (oid, summary) in &commit_oids {
        cherry_pick(&repository, *oid)
            .context(format!("failed to cherry-pick {} {summary}", crate::git::repo::short_oid(*oid)))?;
        eprintln!("Cherry-picked {} {summary}", crate::git::repo::short_oid(*oid));
    }

    eprintln!(
        "\nBackport complete on '{branch_name}'. Next steps:\n  \
         release-ratchet prepare --no-branch\n  \
         release-ratchet release"
    );

    Ok(())
}

fn resolve_target(
    repo: &git2::Repository,
    config: &Config,
    args: &BackportArgs,
) -> Result<(git2::Oid, String)> {
    let onto = &args.onto;

    // Check if it's an existing branch
    if let Ok(branch) = repo.find_branch(onto, git2::BranchType::Local) {
        let oid = branch.get().peel_to_commit()?.id();
        return Ok((oid, onto.clone()));
    }

    // Try as a tag
    let tag_ref = format!("refs/tags/{onto}");
    if let Ok(oid) = repo.refname_to_id(&tag_ref) {
        let commit_oid = repo
            .find_object(oid, None)?
            .peel(git2::ObjectType::Commit)?
            .id();

        let branch_name = if let Some(ref name) = args.branch {
            name.clone()
        } else {
            derive_maintenance_branch(onto, &config.tag_prefix)
        };

        return Ok((commit_oid, branch_name));
    }

    // Try as a raw ref/SHA
    let oid = repo::resolve_ref(repo, onto)
        .context(format!("'{onto}' is not a branch, tag, or commit"))?;
    let branch_name = args
        .branch
        .clone()
        .unwrap_or_else(|| format!("maintain/{onto}"));
    Ok((oid, branch_name))
}

/// Derive a maintenance branch name from a tag.
/// v1.2.3 → maintain/v1.2.x
/// release-v1.2.3 → maintain/release-v1.2.x
fn derive_maintenance_branch(tag: &str, tag_prefix: &str) -> String {
    let version_str = tag.strip_prefix(tag_prefix).unwrap_or(tag);
    if let Ok(version) = Version::parse(version_str) {
        format!("maintain/{tag_prefix}{}.{}.x", version.major, version.minor)
    } else {
        format!("maintain/{tag}")
    }
}

fn cherry_pick(repo: &git2::Repository, oid: git2::Oid) -> Result<(), anyhow::Error> {
    let commit = repo.find_commit(oid)?;

    anyhow::ensure!(
        commit.parent_count() > 0,
        "cannot cherry-pick root commit {} (no parent to diff against)",
        crate::git::repo::short_oid(oid),
    );
    anyhow::ensure!(
        commit.parent_count() == 1,
        "cannot cherry-pick merge commit {} — use the original non-merge commit instead",
        crate::git::repo::short_oid(oid),
    );

    // Compute the cherry-pick: apply the diff between parent and commit onto HEAD
    let head_commit = repo.head()?.peel_to_commit()?;
    let mut index = repo.cherrypick_commit(&commit, &head_commit, 0, None)?;

    if index.has_conflicts() {
        anyhow::bail!(
            "cherry-pick of {} has conflicts. Resolve manually, then continue.",
            crate::git::repo::short_oid(oid),
        );
    }

    // Write the merged tree and create a new commit
    let tree_oid = index.write_tree_to(repo)?;
    let tree = repo.find_tree(tree_oid)?;
    let sig = repo.signature().context(
        "git user.name and user.email must be configured"
    )?;
    let message = commit.message().unwrap_or("cherry-picked commit");

    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[&head_commit])?;

    // Update working tree to match the new commit
    repo.checkout_head(Some(
        git2::build::CheckoutBuilder::new().force(),
    ))?;

    Ok(())
}

