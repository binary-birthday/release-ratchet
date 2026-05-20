use std::path::Path;

use crate::error::RatchetError;

const CHANGELOG_HEADER: &str = "# Changelog\n\nAll notable changes to this project will be documented in this file.\n";

pub fn update_changelog(
    path: &Path,
    new_section: &str,
    remote_url: Option<&str>,
    tag_prefix: &str,
) -> Result<String, RatchetError> {
    let existing = if path.exists() {
        std::fs::read_to_string(path)
            .map_err(|e| RatchetError::Changelog(format!("failed to read {}: {e}", path.display())))?
    } else {
        CHANGELOG_HEADER.to_string()
    };

    let mut result = insert_section(&existing, new_section);

    if let Some(base_url) = remote_url {
        result = append_compare_links(&result, base_url, tag_prefix);
    }

    Ok(result)
}

pub fn write_changelog(path: &Path, contents: &str) -> Result<(), RatchetError> {
    std::fs::write(path, contents)
        .map_err(|e| RatchetError::Changelog(format!("failed to write {}: {e}", path.display())))
}

fn insert_section(existing: &str, new_section: &str) -> String {
    // Normalize CRLF to LF for consistent matching
    let normalized = existing.replace("\r\n", "\n");
    let existing = &normalized;

    // Find the first "## [" which marks the start of an existing version section.
    // Insert the new section before it.
    if let Some(pos) = existing.find("\n## [") {
        let (before, after) = existing.split_at(pos + 1);
        format!("{before}{new_section}\n{after}")
    } else if existing.starts_with("## [") {
        // Changelog starts directly with a version heading (no header/preamble).
        // Insert the new section before it.
        format!("{new_section}\n{existing}")
    } else {
        // No existing version sections; append after the header
        let trimmed = existing.trim_end();
        format!("{trimmed}\n\n{new_section}")
    }
}

fn append_compare_links(content: &str, base_url: &str, tag_prefix: &str) -> String {
    use regex::Regex;

    // Extract all version numbers from ## [X.Y.Z] headings
    let re = Regex::new(r"## \[(\d+\.\d+\.\d+(?:-[\w.\-]+)?)\]").unwrap();
    let versions: Vec<&str> = re.captures_iter(content).map(|c| c.get(1).unwrap().as_str()).collect();

    if versions.is_empty() {
        return content.to_string();
    }

    // Strip any existing link block (lines starting with [ at the end of file)
    let trimmed = content.trim_end();
    let mut lines: Vec<&str> = trimmed.lines().collect();
    while let Some(last) = lines.last() {
        if (last.starts_with('[') && last.contains("]: http")) || last.trim().is_empty() {
            lines.pop();
        } else {
            break;
        }
    }

    let mut result = lines.join("\n");
    result.push_str("\n\n");

    // Generate compare links: newest first
    for (i, version) in versions.iter().enumerate() {
        if i + 1 < versions.len() {
            let prev = versions[i + 1];
            result.push_str(&format!(
                "[{version}]: {base_url}/compare/{tag_prefix}{prev}...{tag_prefix}{version}\n"
            ));
        } else {
            // Oldest version: link to the tag itself
            result.push_str(&format!(
                "[{version}]: {base_url}/releases/tag/{tag_prefix}{version}\n"
            ));
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_into_empty_changelog() {
        let result = insert_section(CHANGELOG_HEADER, "## [1.0.0] - 2026-01-01\n\n### Features\n\n- thing\n");
        assert!(result.contains("# Changelog"));
        assert!(result.contains("## [1.0.0]"));
    }

    #[test]
    fn insert_before_existing_section() {
        let existing = "# Changelog\n\n## [1.0.0] - 2025-01-01\n\n### Features\n\n- old thing\n";
        let result = insert_section(existing, "## [1.1.0] - 2026-01-01\n\n### Features\n\n- new thing\n");
        let pos_new = result.find("## [1.1.0]").unwrap();
        let pos_old = result.find("## [1.0.0]").unwrap();
        assert!(pos_new < pos_old);
    }

    #[test]
    fn insert_before_existing_section_without_header() {
        // Changelog that starts directly with a version heading (no "# Changelog" preamble).
        let existing = "## [1.0.0] - 2025-01-01\n\n### Features\n\n- old thing\n";
        let result = insert_section(existing, "## [1.1.0] - 2026-01-01\n\n### Features\n\n- new thing\n");
        let pos_new = result.find("## [1.1.0]").unwrap();
        let pos_old = result.find("## [1.0.0]").unwrap();
        assert!(pos_new < pos_old, "new section should come before old section, got:\n{result}");
    }
}
