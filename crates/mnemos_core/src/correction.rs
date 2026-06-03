//! Correction value type: the structured "wrong → right → why → trigger" lesson
//! captured when an AI tool is corrected. Stored as a Procedural-tier
//! `MemoryType::Correction` memory; this module owns its body format, the
//! required-`why` validation, the anti-weaponization guard, and trigger→tags.

/// A correction captured from a tool/user, before it becomes a memory.
#[derive(Debug, Clone, PartialEq)]
pub struct Correction {
    pub wrong: String,
    pub right: String,
    pub why: String,
    pub trigger: Option<String>,
}

#[derive(Debug, PartialEq)]
pub enum CorrectionError {
    MissingWhy,
    Weaponized(String),
}

impl std::fmt::Display for CorrectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CorrectionError::MissingWhy => {
                write!(
                    f,
                    "a correction requires a non-empty `why` (the reason the fix is correct)"
                )
            }
            CorrectionError::Weaponized(m) => write!(f, "correction rejected: {m}"),
        }
    }
}

const MIN_WHY_LEN: usize = 8;

/// Keywords that indicate the "right" path would weaken safety/correctness.
const WEAPONIZED_PATTERNS: &[&str] = &[
    "skip the test",
    "skip tests",
    "disable the test",
    "disable validation",
    "bypass validation",
    "ignore the error",
    "remove the check",
    "disable auth",
    "skip verification",
    "comment out the test",
    "turn off validation",
];

impl Correction {
    /// Validate: `why` must be present/substantive, and `right` must not
    /// describe weakening a safety/validation/test step.
    pub fn validate(&self) -> Result<(), CorrectionError> {
        if self.why.trim().len() < MIN_WHY_LEN {
            return Err(CorrectionError::MissingWhy);
        }
        let hay = self.right.to_lowercase();
        if let Some(p) = WEAPONIZED_PATTERNS.iter().find(|p| hay.contains(*p)) {
            return Err(CorrectionError::Weaponized(format!(
                "the corrected approach appears to disable a safeguard (\"{p}\"); \
                 record this as a spec change, not a correction"
            )));
        }
        Ok(())
    }

    /// Render the structured markdown body stored in the memory.
    pub fn to_body(&self) -> String {
        let trigger = self.trigger.as_deref().unwrap_or("");
        format!(
            "**Wrong:** {}\n\n**Right:** {}\n\n**Why:** {}\n\n**Trigger:** {}",
            self.wrong.trim(),
            self.right.trim(),
            self.why.trim(),
            trigger.trim(),
        )
    }

    /// Lowercased word tags derived from the trigger (for recall + clustering).
    /// Empty when no trigger. Dedupes, drops tokens shorter than 3 chars.
    pub fn trigger_tags(&self) -> Vec<String> {
        let mut tags: Vec<String> = self
            .trigger
            .as_deref()
            .unwrap_or("")
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| t.len() >= 3)
            .map(|t| t.to_lowercase())
            .collect();
        tags.sort();
        tags.dedup();
        tags
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(wrong: &str, right: &str, why: &str, trig: Option<&str>) -> Correction {
        Correction {
            wrong: wrong.into(),
            right: right.into(),
            why: why.into(),
            trigger: trig.map(Into::into),
        }
    }

    #[test]
    fn rejects_missing_why() {
        assert_eq!(
            c("did x", "do y", "", None).validate(),
            Err(CorrectionError::MissingWhy)
        );
        assert_eq!(
            c("did x", "do y", "short", None).validate(),
            Err(CorrectionError::MissingWhy)
        );
    }

    #[test]
    fn accepts_with_substantive_why() {
        assert!(c(
            "used tabs",
            "use spaces",
            "the repo enforces spaces in CI",
            None
        )
        .validate()
        .is_ok());
    }

    #[test]
    fn rejects_weaponized_right() {
        let e = c(
            "tests failed",
            "just skip the tests to ship faster",
            "deadline pressure",
            None,
        )
        .validate();
        assert!(matches!(e, Err(CorrectionError::Weaponized(_))));
    }

    #[test]
    fn body_has_all_sections() {
        let b = c("a", "b", "because reasons here", Some("editing config")).to_body();
        assert!(b.contains("**Wrong:** a") && b.contains("**Right:** b"));
        assert!(
            b.contains("**Why:** because reasons here")
                && b.contains("**Trigger:** editing config")
        );
    }

    #[test]
    fn trigger_tags_tokenize_and_dedupe() {
        let tags = c(
            "a",
            "b",
            "because reasons here",
            Some("Rust error handling, error"),
        )
        .trigger_tags();
        assert!(tags.contains(&"error".to_string()) && tags.contains(&"handling".to_string()));
        assert_eq!(tags.iter().filter(|t| *t == "error").count(), 1);
    }
}
