//! v2-Feature 2 — `neurogrim explain version <ver>` (2026-05-09).
//!
//! Parses the workspace's `CHANGELOG.md` (Keep-a-Changelog format) and
//! emits a structured per-version summary suitable for both human
//! reading and agent consumption (machine-parseable JSON or prose).
//!
//! The CHANGELOG is bundled at compile time via `include_str!` so the
//! binary is self-contained — operators don't need the file at runtime.
//! The drift sensor for "bundled CHANGELOG vs. on-disk CHANGELOG"
//! remains an aspirational future feature; current contract is "the
//! shipped binary reflects the version it was built from."

const BUNDLED_CHANGELOG: &str = include_str!("../../../../CHANGELOG.md");

/// One version's slot in the changelog. `version` is the literal token
/// inside the brackets (e.g., `5.0.0`, `Unreleased`); date is the
/// trailing `YYYY-MM-DD` when present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionEntry {
    pub version: String,
    pub date: Option<String>,
    /// Sections parsed from `### ...` subheadings — empty vec when the
    /// entry has prose-only content with no Keep-a-Changelog subsections.
    pub sections: Vec<VersionSection>,
    /// The raw markdown of the entry's body (everything between this
    /// version's heading and the next `## [...]` heading). Useful when
    /// the operator wants to render the original prose verbatim.
    pub raw_body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionSection {
    /// `Added` / `Removed` / `Changed` / `Fixed` / `Deprecated` /
    /// `Security` / `Migration` / etc. — exact-cased per the source.
    pub heading: String,
    /// Bullet items (`- ...` or `* ...`) extracted from the section.
    /// Empty when the section is prose-only.
    pub items: Vec<String>,
    /// Raw markdown body of the section (everything between this
    /// `### Heading` and the next `### Heading` or `## [...]`).
    pub raw: String,
}

/// Find the entry for `version` (matches the bracket-token literally).
/// Returns `None` when the version isn't in the bundled CHANGELOG.
pub fn parse_changelog_entry(text: &str, version: &str) -> Option<VersionEntry> {
    let lines: Vec<&str> = text.lines().collect();
    let target_token = version.trim();

    // Scan for `## [VERSION] - DATE` or `## [VERSION]` heading.
    let mut header_idx: Option<usize> = None;
    let mut header_date: Option<String> = None;
    let mut header_version: Option<String> = None;
    for (i, line) in lines.iter().enumerate() {
        if !line.starts_with("## [") {
            continue;
        }
        // Parse: `## [TOKEN] - DATE` or `## [TOKEN]`
        let after_open = match line.strip_prefix("## [") {
            Some(s) => s,
            None => continue,
        };
        let close = match after_open.find(']') {
            Some(p) => p,
            None => continue,
        };
        let token = &after_open[..close];
        if token != target_token {
            continue;
        }
        // Date suffix
        let rest = after_open[close + 1..].trim();
        let date = rest
            .strip_prefix('-')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        header_idx = Some(i);
        header_version = Some(token.to_string());
        header_date = date;
        break;
    }
    let start = header_idx?;
    let version = header_version?; // unwrap-safe given Some(start)

    // Find the next `## [...]` heading or EOF.
    let mut end = lines.len();
    for (i, line) in lines.iter().enumerate().skip(start + 1) {
        if line.starts_with("## [") {
            end = i;
            break;
        }
    }
    let raw_body = lines[start + 1..end].join("\n").trim().to_string();

    // Parse `### Heading` subsections within the body.
    let body_lines: Vec<&str> = lines[start + 1..end].iter().copied().collect();
    let mut sections: Vec<VersionSection> = Vec::new();
    let mut i = 0;
    while i < body_lines.len() {
        let line = body_lines[i];
        if let Some(heading) = line.strip_prefix("### ") {
            let heading = heading.trim().to_string();
            // Body of this subsection until next ### or end
            let mut j = i + 1;
            while j < body_lines.len() && !body_lines[j].starts_with("### ") {
                j += 1;
            }
            let raw = body_lines[i + 1..j].join("\n").trim().to_string();
            let items = extract_bullets(&raw);
            sections.push(VersionSection { heading, items, raw });
            i = j;
        } else {
            i += 1;
        }
    }

    Some(VersionEntry {
        version,
        date: header_date,
        sections,
        raw_body,
    })
}

/// List every `## [TOKEN] - ...` token in order. Used by error messages
/// when the requested version isn't found.
pub fn list_changelog_versions(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        if let Some(after) = line.strip_prefix("## [") {
            if let Some(close) = after.find(']') {
                out.push(after[..close].to_string());
            }
        }
    }
    out
}

/// Bundled-CHANGELOG entry point — what the `explain version` CLI
/// path calls.
pub fn bundled_entry(version: &str) -> Option<VersionEntry> {
    parse_changelog_entry(BUNDLED_CHANGELOG, version)
}

/// Bundled CHANGELOG version list (for "no entry for X — available: …" errors).
pub fn bundled_versions() -> Vec<String> {
    list_changelog_versions(BUNDLED_CHANGELOG)
}

/// Extract bullet items from a body. Handles `-` and `*` markers,
/// continuation indentation, and skips blank lines. Each returned
/// item is a single-line trimmed string.
fn extract_bullets(body: &str) -> Vec<String> {
    let mut items: Vec<String> = Vec::new();
    let mut current: Option<String> = None;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            if let Some(prev) = current.take() {
                items.push(prev);
            }
            current = Some(rest.trim().to_string());
        } else if !trimmed.is_empty() {
            // Continuation of the current bullet. Append a space + the
            // continuation text (preserves multi-line bullet semantics
            // without keeping the indentation noise).
            if let Some(prev) = current.as_mut() {
                if !prev.is_empty() {
                    prev.push(' ');
                }
                prev.push_str(trimmed.trim());
            }
        }
    }
    if let Some(prev) = current {
        items.push(prev);
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"# Changelog

## [Unreleased]

## [5.0.0] - 2026-05-04

*"Everything is Lego" — finishes things.*

### Added
- Trait surfaces at four high-leverage seams
- SDK extraction at `crates/neurogrim-sdk/`

### Removed
- Closed-set BackendKind enum (V5-MOD-3)

## [3.1.0] - 2026-05-04

Additive doc pass.

### Added
- §9.8 trait surface recommendation
"#;

    #[test]
    fn parse_finds_5_0_0() {
        let entry = parse_changelog_entry(FIXTURE, "5.0.0").expect("5.0.0 parses");
        assert_eq!(entry.version, "5.0.0");
        assert_eq!(entry.date.as_deref(), Some("2026-05-04"));
        assert!(entry.raw_body.contains("Everything is Lego"));
        assert_eq!(entry.sections.len(), 2);
        assert_eq!(entry.sections[0].heading, "Added");
        assert_eq!(entry.sections[0].items.len(), 2);
        assert_eq!(entry.sections[1].heading, "Removed");
    }

    #[test]
    fn parse_finds_unreleased_with_no_date() {
        let entry = parse_changelog_entry(FIXTURE, "Unreleased").expect("Unreleased parses");
        assert_eq!(entry.version, "Unreleased");
        assert!(entry.date.is_none());
    }

    #[test]
    fn parse_returns_none_for_missing_version() {
        assert!(parse_changelog_entry(FIXTURE, "99.99.99").is_none());
    }

    #[test]
    fn parse_does_not_overrun_into_next_version() {
        let entry = parse_changelog_entry(FIXTURE, "5.0.0").expect("5.0.0 parses");
        // The 3.1.0 entry's "trait surface recommendation" should NOT
        // be in the 5.0.0 body.
        assert!(!entry.raw_body.contains("trait surface recommendation"));
    }

    #[test]
    fn list_versions_ordered() {
        let versions = list_changelog_versions(FIXTURE);
        assert_eq!(versions, vec!["Unreleased", "5.0.0", "3.1.0"]);
    }

    #[test]
    fn bundled_entry_finds_a_real_release() {
        // The bundled CHANGELOG must have at least one parseable entry.
        // Concrete check: 5.0.0 was the v5 ship per the workspace
        // version pin.
        let entry = bundled_entry("5.0.0");
        assert!(
            entry.is_some(),
            "bundled CHANGELOG must contain 5.0.0 — bundled versions: {:?}",
            bundled_versions()
        );
    }

    #[test]
    fn bullet_extraction_handles_dash_and_star() {
        let body = "- alpha\n- beta\n* gamma";
        let bullets = extract_bullets(body);
        assert_eq!(bullets, vec!["alpha", "beta", "gamma"]);
    }

    #[test]
    fn bullet_extraction_joins_continuations() {
        let body = "- alpha\n  continued\n- beta";
        let bullets = extract_bullets(body);
        assert_eq!(bullets, vec!["alpha continued", "beta"]);
    }
}
