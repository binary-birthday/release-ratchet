/// Extract the section for a specific version from changelog content.
/// Version should be without prefix (e.g., "0.1.0" not "v0.1.0").
pub fn extract_section(content: &str, version: &str) -> Option<String> {
    let marker = format!("## [{version}]");
    let start = content.find(&marker)?;
    let rest = &content[start..];
    let end = rest[3..].find("\n## [").map(|i| i + 3).unwrap_or(rest.len());
    Some(rest[..end].trim_end().to_string())
}

/// Extract the most recent (topmost) version section from changelog content.
pub fn extract_latest_section(content: &str) -> Option<String> {
    let start = content.find("## [")?;
    let rest = &content[start..];
    let end = rest[3..].find("\n## [").map(|i| i + 3).unwrap_or(rest.len());
    Some(rest[..end].trim_end().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"# Changelog

All notable changes to this project will be documented in this file.

## [0.2.0] - 2026-05-18

### Features

- second feature (abc1234)

## [0.1.0] - 2026-05-17

### Features

- first feature (def5678)

### Bug Fixes

- a fix (aaa1111)
"#;

    #[test]
    fn extract_specific_version() {
        let section = extract_section(SAMPLE, "0.1.0").unwrap();
        assert!(section.starts_with("## [0.1.0]"));
        assert!(section.contains("first feature"));
        assert!(section.contains("a fix"));
        assert!(!section.contains("second feature"));
    }

    #[test]
    fn extract_latest() {
        let section = extract_latest_section(SAMPLE).unwrap();
        assert!(section.starts_with("## [0.2.0]"));
        assert!(section.contains("second feature"));
        assert!(!section.contains("first feature"));
    }

    #[test]
    fn extract_missing_version() {
        assert!(extract_section(SAMPLE, "9.9.9").is_none());
    }

    #[test]
    fn extract_from_empty() {
        assert!(extract_latest_section("# Changelog\n").is_none());
        assert!(extract_section("# Changelog\n", "0.1.0").is_none());
    }

    #[test]
    fn extract_single_version() {
        let single = "# Changelog\n\n## [1.0.0] - 2026-01-01\n\n### Features\n\n- thing\n";
        let section = extract_section(single, "1.0.0").unwrap();
        assert!(section.contains("thing"));
        let latest = extract_latest_section(single).unwrap();
        assert_eq!(section, latest);
    }
}
