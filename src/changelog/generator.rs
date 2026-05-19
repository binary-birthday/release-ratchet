use std::collections::BTreeMap;

use chrono::Local;
use semver::Version;

use crate::config::Config;
use crate::conventional::types::{CommitType, ConventionalCommit};

pub fn generate_section(
    version: &Version,
    commits: &[ConventionalCommit],
    config: &Config,
    remote_url: Option<&str>,
) -> String {
    generate_section_with_date(version, commits, config, None, remote_url)
}

pub fn generate_section_with_date(
    version: &Version,
    commits: &[ConventionalCommit],
    config: &Config,
    date_override: Option<&str>,
    remote_url: Option<&str>,
) -> String {
    let today = Local::now().format("%Y-%m-%d").to_string();
    let date = date_override.unwrap_or(&today);
    let mut out = format!("## [{version}] - {date}\n");

    // Collect breaking changes across all types
    let breaking: Vec<&ConventionalCommit> = commits.iter().filter(|c| c.is_breaking()).collect();

    if !breaking.is_empty() {
        out.push_str("\n### BREAKING CHANGES\n\n");
        for c in &breaking {
            out.push_str(&format_entry(c, remote_url));
        }
    }

    // Group commits by their changelog heading
    let mut groups: BTreeMap<String, Vec<&ConventionalCommit>> = BTreeMap::new();

    for commit in commits {
        if let Some(heading) = config.changelog_heading_for_type(&commit.commit_type) {
            groups.entry(heading).or_default().push(commit);
        }
    }

    // Output built-in headings in canonical order (derived from CommitType),
    // then any remaining custom headings in alphabetical order.
    let canonical_order: Vec<String> = [
        CommitType::Feat,
        CommitType::Fix,
        CommitType::Perf,
        CommitType::Revert,
    ]
    .iter()
    .filter_map(|ct| ct.default_changelog_heading().map(String::from))
    .collect();

    for heading in &canonical_order {
        if let Some(entries) = groups.remove(heading) {
            out.push_str(&format!("\n### {heading}\n\n"));
            for c in entries {
                out.push_str(&format_entry(c, remote_url));
            }
        }
    }

    for (heading, entries) in &groups {
        out.push_str(&format!("\n### {heading}\n\n"));
        for c in entries {
            out.push_str(&format_entry(c, remote_url));
        }
    }

    out
}

fn format_entry(commit: &ConventionalCommit, remote_url: Option<&str>) -> String {
    let scope_prefix = match &commit.scope {
        Some(s) => format!("**{s}**: "),
        None => String::new(),
    };
    let oid_ref = match remote_url {
        Some(base) => format!("[{}]({}/commit/{})", commit.short_oid(), base, commit.oid),
        None => format!("({})", commit.short_oid()),
    };
    let mut entry = format!("- {scope_prefix}{} {oid_ref}\n", commit.description);
    // Include body for breaking changes to provide migration context
    if commit.is_breaking() {
        if let Some(ref body) = commit.body {
            for line in body.lines() {
                entry.push_str(&format!("  {line}\n"));
            }
        }
        // Also include BREAKING CHANGE footer value if present
        for footer in &commit.footers {
            let upper = footer.token.to_uppercase();
            if upper == "BREAKING CHANGE" || upper == "BREAKING-CHANGE" {
                entry.push_str(&format!("  {}\n", footer.value));
            }
        }
    }
    entry
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conventional::types::CommitType;

    fn make_commit(commit_type: CommitType, scope: Option<&str>, desc: &str, breaking: bool) -> ConventionalCommit {
        ConventionalCommit {
            oid: git2::Oid::from_str("abcdef1234567890abcdef1234567890abcdef12").unwrap(),
            commit_type,
            scope: scope.map(String::from),
            breaking,
            description: desc.to_string(),
            body: None,
            footers: vec![],
            raw_message: String::new(),
            author: "Test".to_string(),
        }
    }

    #[test]
    fn generates_section_with_features_and_fixes() {
        let config = Config::default();
        let commits = vec![
            make_commit(CommitType::Feat, Some("auth"), "add OAuth support", false),
            make_commit(CommitType::Fix, None, "fix null pointer", false),
        ];
        let section = generate_section(&Version::new(1, 1, 0), &commits, &config, None);
        assert!(section.contains("## [1.1.0]"));
        assert!(section.contains("### Features"));
        assert!(section.contains("**auth**: add OAuth support"));
        assert!(section.contains("### Bug Fixes"));
        assert!(section.contains("fix null pointer"));
    }

    #[test]
    fn breaking_changes_section() {
        let config = Config::default();
        let commits = vec![
            make_commit(CommitType::Feat, Some("api"), "remove old endpoint", true),
        ];
        let section = generate_section(&Version::new(2, 0, 0), &commits, &config, None);
        assert!(section.contains("### BREAKING CHANGES"));
        assert!(section.contains("### Features"));
    }

    #[test]
    fn skips_types_without_changelog_heading() {
        let config = Config::default();
        let commits = vec![
            make_commit(CommitType::Chore, None, "update deps", false),
            make_commit(CommitType::Ci, None, "fix pipeline", false),
        ];
        let section = generate_section(&Version::new(1, 0, 1), &commits, &config, None);
        assert!(!section.contains("###"));
    }

    #[test]
    fn commit_links_with_remote() {
        let config = Config::default();
        let commits = vec![
            make_commit(CommitType::Feat, None, "add thing", false),
        ];
        let section = generate_section(
            &Version::new(1, 0, 0), &commits, &config,
            Some("https://github.com/user/repo"),
        );
        assert!(section.contains("[abcdef1](https://github.com/user/repo/commit/"));
    }

    #[test]
    fn no_commit_links_without_remote() {
        let config = Config::default();
        let commits = vec![
            make_commit(CommitType::Feat, None, "add thing", false),
        ];
        let section = generate_section(&Version::new(1, 0, 0), &commits, &config, None);
        assert!(section.contains("(abcdef1)"));
        assert!(!section.contains("[abcdef1]"));
    }
}
