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

    // SSH: ssh://[user@]host[:port]/path
    if let Some(rest) = url.strip_prefix("ssh://") {
        let host_and_path = if let Some(at) = rest.find('@') {
            &rest[at + 1..]
        } else {
            rest
        };
        // Strip port if present (host:port/path → host/path)
        let normalized = if let Some(colon) = host_and_path.find(':') {
            if let Some(slash) = host_and_path[colon..].find('/') {
                format!("{}{}", &host_and_path[..colon], &host_and_path[colon + slash..])
            } else {
                host_and_path.to_string()
            }
        } else {
            host_and_path.to_string()
        };
        let trimmed = normalized.trim_end_matches(".git").trim_end_matches('/');
        return Some(format!("https://{trimmed}"));
    }

    // git:// protocol (legacy)
    if let Some(rest) = url.strip_prefix("git://") {
        let trimmed = rest.trim_end_matches(".git").trim_end_matches('/');
        return Some(format!("https://{trimmed}"));
    }

    // HTTPS/HTTP: strip credentials and .git suffix
    if url.starts_with("https://") || url.starts_with("http://") {
        let stripped = strip_credentials(url);
        let trimmed = stripped.trim_end_matches(".git").trim_end_matches('/');
        return Some(trimmed.to_string());
    }

    None
}

/// Strip user:pass@ from HTTPS URLs.
fn strip_credentials(url: &str) -> String {
    // https://user:pass@host/path → https://host/path
    if let Some(proto_end) = url.find("://") {
        let after_proto = &url[proto_end + 3..];
        if let Some(at) = after_proto.find('@') {
            return format!("{}{}", &url[..proto_end + 3], &after_proto[at + 1..]);
        }
    }
    url.to_string()
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
    fn ssh_protocol_with_port() {
        assert_eq!(
            normalize_to_https("ssh://git@gitlab.example.com:2222/user/repo.git"),
            Some("https://gitlab.example.com/user/repo".into())
        );
    }

    #[test]
    fn ssh_protocol_no_user() {
        assert_eq!(
            normalize_to_https("ssh://github.com/user/repo.git"),
            Some("https://github.com/user/repo".into())
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
    fn https_with_credentials() {
        assert_eq!(
            normalize_to_https("https://token:x-oauth-basic@github.com/user/repo.git"),
            Some("https://github.com/user/repo".into())
        );
    }

    #[test]
    fn git_protocol() {
        assert_eq!(
            normalize_to_https("git://github.com/user/repo.git"),
            Some("https://github.com/user/repo".into())
        );
    }

    #[test]
    fn unparseable() {
        assert_eq!(normalize_to_https("not-a-url"), None);
        assert_eq!(normalize_to_https(""), None);
    }
}
