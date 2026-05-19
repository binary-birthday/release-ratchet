use std::path::Path;

use git2::Repository;

use crate::error::RatchetError;

pub fn open(path: &Path) -> Result<Repository, RatchetError> {
    Ok(Repository::open(path)?)
}

pub fn resolve_ref(repo: &Repository, refspec: &str) -> Result<git2::Oid, RatchetError> {
    let obj = repo
        .revparse_single(refspec)
        .map_err(|e| RatchetError::Other(format!("cannot resolve '{refspec}': {e}")))?;
    let commit = obj
        .peel_to_commit()
        .map_err(|e| RatchetError::Other(format!("'{refspec}' is not a commit: {e}")))?;
    Ok(commit.id())
}
