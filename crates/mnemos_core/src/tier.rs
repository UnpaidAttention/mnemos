use crate::error::{MnemosError, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Tier {
    Working,
    Episodic,
    #[default]
    Semantic,
    Procedural,
    Reflection,
}

impl Tier {
    pub fn as_str(self) -> &'static str {
        match self {
            Tier::Working => "working",
            Tier::Episodic => "episodic",
            Tier::Semantic => "semantic",
            Tier::Procedural => "procedural",
            Tier::Reflection => "reflection",
        }
    }

    /// Directory name on disk. `Reflection` maps to `reflections/` (pluralized)
    /// for human-friendliness; all others match `as_str`.
    pub fn dir_name(self) -> &'static str {
        match self {
            Tier::Reflection => "reflections",
            other => other.as_str(),
        }
    }

    pub fn all() -> &'static [Tier] {
        &[
            Tier::Working,
            Tier::Episodic,
            Tier::Semantic,
            Tier::Procedural,
            Tier::Reflection,
        ]
    }

    /// Default weight used by retrieval ranking. Tunable later via config.
    pub fn default_weight(self) -> f64 {
        match self {
            Tier::Working => 2.0,
            Tier::Procedural => 1.5,
            Tier::Reflection => 1.2,
            Tier::Semantic => 1.0,
            Tier::Episodic => 0.8,
        }
    }
}

impl fmt::Display for Tier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Tier {
    type Err = MnemosError;
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "working" => Ok(Tier::Working),
            "episodic" => Ok(Tier::Episodic),
            "semantic" => Ok(Tier::Semantic),
            "procedural" => Ok(Tier::Procedural),
            "reflection" | "reflections" => Ok(Tier::Reflection),
            other => Err(MnemosError::Validation(format!("unknown tier: {other}"))),
        }
    }
}
