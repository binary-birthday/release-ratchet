use std::collections::BTreeMap;

use chrono::Local;
use semver::Version;

use crate::config::Config;
use crate::conventional::types::ConventionalCommit;

pub fn generate_section(
    version: &Version,
    commits: &[ConventionalCommit],
    config: &Config,
) -> String {
    generate_section_with_date(version, commits, config, None)
}

pub fn generate_section_with_date(
    version: &Version,
    commits: &[ConventionalCommit],
    config: &Config,
    date_override: Option<&str>,
) -> String {
    let today = Local::now().format("%Y-%m-%d").to_string();
    let date = date_override.unwrap_or(&today);
    let mut out = format!("## [{version}] - {date}\n");

    // Collect breaking changes across all types
    let breaking: Vec<&ConventionalCommit> = commits.iter().filter(|c| c.is_breaking()).collect();

    if !breaking.is_empty() {
        out.push_str("\n### BREAKING CHANGES\n\n");
        for c in &breaking {
            out.push_str(&format_entry(c));
        }
    }

    // Group commits by their changelog heading
    // Use BTreeMap for deterministic ordering
    let mut groups: BTreeMap<String, Vec<&ConventionalCommit>> = BTreeMap::new();

    // Define the display order for built-in headings
    let heading_order = [
        "Features",
        "Bug Fixes",
        "Performance",
        "Reverts",
    ];

    for commit in commits {
        if let Some(heading) = config.changelog_heading_for_type(&commit.commit_type) {
            groups.entry(heading).or_default().push(commit);
        }
    }

    // Output in defined order first, then any custom headings alphabetically
    for heading in &heading_order {
        let heading_str = heading.to_string();
        if let Some(entries) = groups.remove(&heading_str) {
            out.push_str(&format!("\n### {heading}\n\n"));
            for c in entries {
                out.push_str(&format_entry(c));
            }
        }
    }

    // Remaining custom headings
    for (heading, entries) in &groups {
        out.push_str(&format!("\n### {heading}\n\n"));
        for c in entries {
            out.push_str(&format_entry(c));
        }
    }

    out
}

fn format_entry(commit: &ConventionalCommit) -> String {
    let scope_prefix = match &commit.scope {
        Some(s) => format!("**{s}**: "),
        None => String::new(),
    };
    let mut entry = format!(
        "- {scope_prefix}{} ({})\n",
        commit.description,
        commit.short_oid()
    );
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
        let section = generate_section(&Version::new(1, 1, 0), &commits, &config);
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
        let section = generate_section(&Version::new(2, 0, 0), &commits, &config);
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
        let section = generate_section(&Version::new(1, 0, 1), &commits, &config);
        // Should only have the version header, no type sections
        assert!(!section.contains("###"));
    }
}
