use crate::error::{MnemosError, Result};
use crate::types::Memory;
use std::path::Path;

const DELIM: &str = "---";

/// Parse a markdown string with YAML frontmatter into a [`Memory`] (frontmatter
/// fields) and the body text.
///
/// The `Memory.body` field is populated with the same body string that is
/// returned as the second tuple element. The caller may prefer to use the
/// returned `String` directly.
///
/// Parsing rules:
/// - The file must begin with `---` (optionally preceded by a UTF-8 BOM).
/// - A newline must follow the opening `---`.
/// - A closing `---` must appear on its own line (preceded by `\n`).
/// - Leading blank lines between the closing delimiter and the body are trimmed.
pub fn parse_frontmatter(text: &str) -> Result<(Memory, String)> {
    // Strip optional UTF-8 BOM.
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);

    // Require the file to start with the opening delimiter.
    let rest = text
        .strip_prefix(DELIM)
        .ok_or_else(|| MnemosError::Validation("missing opening '---' delimiter".into()))?;

    // Skip optional carriage return before the mandatory newline.
    let rest = rest
        .trim_start_matches('\r')
        .strip_prefix('\n')
        .ok_or_else(|| MnemosError::Validation("expected newline after opening '---'".into()))?;

    // Find the closing delimiter, which must be on its own line.
    let end_idx = rest
        .find("\n---")
        .ok_or_else(|| MnemosError::Validation("missing closing '---' delimiter".into()))?;

    let yaml_part = &rest[..end_idx];
    // Skip "\n---" (4 bytes) to reach the content after the closing delimiter.
    let after = &rest[end_idx + 4..];
    // Trim leading newlines between the closing delimiter and the body.
    let body = after.trim_start_matches(['\r', '\n']).to_string();

    let mut mem: Memory = serde_yaml::from_str(yaml_part)?;
    mem.body = body.clone();
    Ok((mem, body))
}

/// Serialize a [`Memory`] back to a markdown string with YAML frontmatter.
///
/// The YAML block contains all fields except `body` (which is `#[serde(skip)]`
/// on the type). The body is appended after the closing `---` separator.
pub fn serialize_with_frontmatter(mem: &Memory) -> Result<String> {
    let yaml = serde_yaml::to_string(mem)?;
    Ok(format!("---\n{yaml}---\n\n{}", mem.body))
}

/// Like [`parse_frontmatter`] but enriches validation errors with the file
/// path for better diagnostics.
pub fn parse_frontmatter_at(text: &str, path: &Path) -> Result<(Memory, String)> {
    parse_frontmatter(text).map_err(|e| match e {
        MnemosError::Validation(reason) => MnemosError::InvalidFrontmatter {
            path: path.to_path_buf(),
            reason,
        },
        other => other,
    })
}
