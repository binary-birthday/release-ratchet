use git2::Repository;

/// Get the remote URL for "origin", normalized to HTTPS.
/// Returns None if no remote or URL is unparseable.
pub fn get_remote_url(repo: &Repository) -> Option<String> {
    let remote = repo.find_remote("origin").ok()?;
    let url = remote.url()?;
    normalize_to_https(url)
}

fn normalize_to_https(raw: &str) -> Option<String> {
    let url = raw.trim();

    // SSH: git@github.com:user/repo.git
    if let Some(rest) = url.strip_prefix("git@") {
        let colon = rest.find(':')?;
        let host = &rest[..colon];
        let path = rest[colon + 1..].trim_end_matches(".git").trim_end_matches('/');
        return Some(format!("https://{host}/{path}"));
    }

    // SSH: ssh://git@github.com/user/repo.git
    if let Some(rest) = url.strip_prefix("ssh://") {
        let at = rest.find('@')?;
        let host_and_path = &rest[at + 1..];
        let trimmed = host_and_path.trim_end_matches(".git").trim_end_matches('/');
        return Some(format!("https://{trimmed}"));
    }

    // HTTPS: https://github.com/user/repo.git
    if url.starts_with("https://") || url.starts_with("http://") {
        let trimmed = url.trim_end_matches(".git").trim_end_matches('/');
        return Some(trimmed.to_string());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_url() {
        assert_eq!(
            normalize_to_https("git@github.com:user/repo.git"),
            Some("https://github.com/user/repo".into())
        );
    }

    #[test]
    fn ssh_url_no_dot_git() {
        assert_eq!(
            normalize_to_https("git@gitlab.com:org/project"),
            Some("https://gitlab.com/org/project".into())
        );
    }

    #[test]
    fn ssh_protocol_url() {
        assert_eq!(
            normalize_to_https("ssh://git@bitbucket.org/team/repo.git"),
            Some("https://bitbucket.org/team/repo".into())
        );
    }

    #[test]
    fn https_url() {
        assert_eq!(
            normalize_to_https("https://github.com/user/repo.git"),
            Some("https://github.com/user/repo".into())
        );
    }

    #[test]
    fn https_url_no_dot_git() {
        assert_eq!(
            normalize_to_https("https://github.com/user/repo"),
            Some("https://github.com/user/repo".into())
        );
    }

    #[test]
    fn unparseable() {
        assert_eq!(normalize_to_https("not-a-url"), None);
        assert_eq!(normalize_to_https(""), None);
    }
}
