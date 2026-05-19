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
