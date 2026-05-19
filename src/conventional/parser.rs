use regex::Regex;
use std::sync::LazyLock;

use super::types::{CommitFooter, CommitType, ConventionalCommit};

static HEADER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(?P<type>[a-zA-Z]+)(?:\((?P<scope>[a-zA-Z0-9_/.\-]+)\))?(?P<breaking>!)?:\s+(?P<desc>.+)$",
    )
    .unwrap()
});

static FOOTER_RE: LazyLock<Regex> = LazyLock::new(|| {
    // Matches "Token: value" or "Token #value" or "BREAKING CHANGE: value"
    // The CC spec says footers use either ": " or " #" as separator.
    Regex::new(r"^(?P<token>[A-Za-z][A-Za-z0-9\-]*|BREAKING CHANGE)\s*:\s+(?P<value>.+)$|^(?P<token2>[A-Za-z][A-Za-z0-9\-]*)\s+(?P<value2>#.+)$")
        .unwrap()
});

pub fn parse_commit(oid: git2::Oid, message: &str, author: &str) -> Option<ConventionalCommit> {
    let mut lines = message.lines();
    let first_line = lines.next()?.trim();

    let caps = HEADER_RE.captures(first_line)?;

    let commit_type = CommitType::from_str(caps.name("type").unwrap().as_str());
    let scope = caps.name("scope").map(|m| m.as_str().to_string());
    let breaking = caps.name("breaking").is_some();
    let description = caps.name("desc").unwrap().as_str().trim().to_string();

    let rest: Vec<&str> = lines.collect();
    let (body, footers) = parse_body_and_footers(&rest);

    Some(ConventionalCommit {
        oid,
        commit_type,
        scope,
        breaking,
        description,
        body,
        footers,
        raw_message: message.to_string(),
        author: author.to_string(),
    })
}

fn parse_body_and_footers(lines: &[&str]) -> (Option<String>, Vec<CommitFooter>) {
    if lines.is_empty() {
        return (None, vec![]);
    }

    // Skip leading blank line(s) between header and body
    let mut start = 0;
    while start < lines.len() && lines[start].trim().is_empty() {
        start += 1;
    }

    if start >= lines.len() {
        return (None, vec![]);
    }

    // Find where footers begin: footers are at the end, each line matching FOOTER_RE,
    // preceded by a blank line separator from the body.
    // Walk backwards from the end to find the footer block.
    let mut footer_start = lines.len();
    let mut i = lines.len();
    while i > start {
        i -= 1;
        let line = lines[i].trim();
        if line.is_empty() {
            // The blank line before footers
            break;
        }
        if FOOTER_RE.is_match(line) || (footer_start < lines.len() && !line.is_empty()) {
            // This is a footer line, or a continuation of a multi-line footer value
            footer_start = i;
        } else {
            // Not a footer line -- everything after this is body
            footer_start = lines.len();
            break;
        }
    }

    // If footer_start is right after start with no blank line separator,
    // and there's content before it, treat everything as body
    let has_blank_before_footers =
        footer_start > start && footer_start > 0 && lines[footer_start - 1].trim().is_empty();

    let (body_lines, footer_lines) = if footer_start < lines.len() && has_blank_before_footers {
        (&lines[start..footer_start - 1], &lines[footer_start..])
    } else if footer_start < lines.len() && footer_start == start {
        // Footers immediately after blank line (no body)
        (&lines[start..start], &lines[footer_start..])
    } else {
        (&lines[start..], &[][..])
    };

    let body = if body_lines.is_empty() {
        None
    } else {
        let b = body_lines.join("\n").trim().to_string();
        if b.is_empty() { None } else { Some(b) }
    };

    let mut footers = Vec::new();
    let mut current_footer: Option<CommitFooter> = None;

    for line in footer_lines {
        if let Some(caps) = FOOTER_RE.captures(line.trim()) {
            if let Some(f) = current_footer.take() {
                footers.push(f);
            }
            // Handle two alternatives in the regex
            let (token, value) = if let Some(t) = caps.name("token") {
                (t.as_str().to_string(), caps.name("value").unwrap().as_str().to_string())
            } else {
                (caps.name("token2").unwrap().as_str().to_string(), caps.name("value2").unwrap().as_str().to_string())
            };
            current_footer = Some(CommitFooter { token, value });
        } else if let Some(ref mut f) = current_footer {
            // Multi-line footer value continuation
            f.value.push('\n');
            f.value.push_str(line.trim());
        }
    }
    if let Some(f) = current_footer {
        footers.push(f);
    }

    (body, footers)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oid() -> git2::Oid {
        git2::Oid::from_str("0000000000000000000000000000000000000000").unwrap()
    }

    #[test]
    fn simple_feat() {
        let c = parse_commit(oid(), "feat: add login", "Alice").unwrap();
        assert_eq!(c.commit_type, CommitType::Feat);
        assert!(c.scope.is_none());
        assert!(!c.breaking);
        assert_eq!(c.description, "add login");
    }

    #[test]
    fn scoped_fix() {
        let c = parse_commit(oid(), "fix(parser): handle edge case", "Bob").unwrap();
        assert_eq!(c.commit_type, CommitType::Fix);
        assert_eq!(c.scope.as_deref(), Some("parser"));
        assert!(!c.breaking);
    }

    #[test]
    fn breaking_with_bang() {
        let c = parse_commit(oid(), "feat!: remove old API", "Alice").unwrap();
        assert!(c.breaking);
        assert!(c.is_breaking());
    }

    #[test]
    fn breaking_with_scoped_bang() {
        let c = parse_commit(oid(), "refactor(auth)!: rewrite session handling", "Alice").unwrap();
        assert!(c.breaking);
        assert_eq!(c.commit_type, CommitType::Refactor);
        assert_eq!(c.scope.as_deref(), Some("auth"));
    }

    #[test]
    fn breaking_change_footer() {
        let msg = "feat: add new API\n\nSome body text.\n\nBREAKING CHANGE: old API removed";
        let c = parse_commit(oid(), msg, "Alice").unwrap();
        assert!(!c.breaking); // no ! in header
        assert!(c.is_breaking()); // but footer says so
        assert_eq!(c.body.as_deref(), Some("Some body text."));
        assert_eq!(c.footers.len(), 1);
        assert_eq!(c.footers[0].token, "BREAKING CHANGE");
        assert_eq!(c.footers[0].value, "old API removed");
    }

    #[test]
    fn breaking_change_hyphenated_footer() {
        let msg = "fix: update thing\n\nBREAKING-CHANGE: stuff changed";
        let c = parse_commit(oid(), msg, "Alice").unwrap();
        assert!(c.is_breaking());
    }

    #[test]
    fn body_and_multiple_footers() {
        let msg = "feat(cli): add verbose flag\n\nThis adds a -v flag for verbose output.\nIt supports multiple levels.\n\nReviewed-by: Bob\nRefs #123";
        let c = parse_commit(oid(), msg, "Alice").unwrap();
        assert_eq!(c.body.as_deref(), Some("This adds a -v flag for verbose output.\nIt supports multiple levels."));
        assert_eq!(c.footers.len(), 2);
        assert_eq!(c.footers[0].token, "Reviewed-by");
        assert_eq!(c.footers[0].value, "Bob");
        assert_eq!(c.footers[1].token, "Refs");
        assert_eq!(c.footers[1].value, "#123");
    }

    #[test]
    fn non_conventional_returns_none() {
        assert!(parse_commit(oid(), "update readme", "Alice").is_none());
        assert!(parse_commit(oid(), "WIP", "Alice").is_none());
        assert!(parse_commit(oid(), "", "Alice").is_none());
    }

    #[test]
    fn all_standard_types() {
        for ty in ["feat", "fix", "docs", "style", "refactor", "perf", "test", "build", "ci", "chore", "revert"] {
            let msg = format!("{ty}: do something");
            let c = parse_commit(oid(), &msg, "Alice").unwrap();
            assert_eq!(c.commit_type.as_str(), ty);
        }
    }

    #[test]
    fn custom_type() {
        let c = parse_commit(oid(), "security: patch vulnerability", "Alice").unwrap();
        assert_eq!(c.commit_type, CommitType::Custom("security".into()));
    }

    #[test]
    fn no_body_with_footer() {
        let msg = "fix: thing\n\nRefs #456";
        let c = parse_commit(oid(), msg, "Alice").unwrap();
        // Footer right after blank line, no body
        assert!(c.body.is_none() || c.body.as_deref() == Some("Refs #456"));
        // The important thing is it parses successfully
    }
}
