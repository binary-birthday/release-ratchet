use std::path::Path;

use crate::error::RatchetError;

const CHANGELOG_HEADER: &str = "# Changelog\n\nAll notable changes to this project will be documented in this file.\n";

pub fn update_changelog(path: &Path, new_section: &str) -> Result<String, RatchetError> {
    let existing = if path.exists() {
        std::fs::read_to_string(path)
            .map_err(|e| RatchetError::Changelog(format!("failed to read {}: {e}", path.display())))?
    } else {
        CHANGELOG_HEADER.to_string()
    };

    Ok(insert_section(&existing, new_section))
}

pub fn write_changelog(path: &Path, contents: &str) -> Result<(), RatchetError> {
    std::fs::write(path, contents)
        .map_err(|e| RatchetError::Changelog(format!("failed to write {}: {e}", path.display())))
}

fn insert_section(existing: &str, new_section: &str) -> String {
    // Find the first "## [" which marks the start of an existing version section.
    // Insert the new section before it.
    if let Some(pos) = existing.find("\n## [") {
        let (before, after) = existing.split_at(pos + 1); // +1 to keep the newline
        format!("{before}{new_section}\n{after}")
    } else {
        // No existing version sections; append after the header
        let trimmed = existing.trim_end();
        format!("{trimmed}\n\n{new_section}")
    }
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
}
