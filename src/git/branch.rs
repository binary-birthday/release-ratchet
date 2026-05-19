use git2::Repository;

use crate::error::RatchetError;

pub fn create_and_checkout(repo: &Repository, name: &str) -> Result<(), RatchetError> {
    let head = repo.head()?;
    let commit = head.peel_to_commit()?;

    // Create the branch (force=true to overwrite if re-running prepare)
    repo.branch(name, &commit, true)?;

    // Checkout the branch
    let refname = format!("refs/heads/{name}");
    let obj = repo.revparse_single(&refname)?;
    repo.checkout_tree(
        &obj,
        Some(git2::build::CheckoutBuilder::new().safe()),
    )?;
    repo.set_head(&refname)?;

    Ok(())
}

pub fn delete_branch(repo: &Repository, name: &str) -> Result<(), RatchetError> {
    match repo.find_branch(name, git2::BranchType::Local) {
        Ok(mut branch) => {
            branch.delete()?;
            Ok(())
        }
        Err(_) => Ok(()), // branch doesn't exist, nothing to do
    }
}
