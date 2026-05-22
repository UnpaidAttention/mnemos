use crate::tier::Tier;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MemoryType {
    Fact,
    Episode,
    Reflection,
    Rule,
    Identity,
    Project,
    Entity,
    CommunitySummary,
}

/// Provenance link: which session and chunks the memory was derived from.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub session: Option<String>,
    #[serde(default)]
    pub chunks: Vec<String>,
}

/// One memory = one markdown file. The struct mirrors the YAML frontmatter
/// exactly; the body is held separately.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Memory {
    pub id: String,
    pub tier: Tier,
    #[serde(rename = "type")]
    pub kind: MemoryType,
    pub title: String,
    #[serde(skip)]
    pub body: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub entities: Vec<String>,
    #[serde(default)]
    pub links: Vec<String>,
    #[serde(default)]
    pub provenance: Vec<Provenance>,
    pub created_at: DateTime<Utc>,
    pub ingested_at: DateTime<Utc>,
    pub valid_at: DateTime<Utc>,
    #[serde(default)]
    pub invalid_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub superseded_by: Option<String>,
    pub strength: f64,
    pub importance: f64,
    pub last_accessed: DateTime<Utc>,
    pub access_count: u64,
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub source_tool: Option<String>,
    #[serde(default = "default_mnemos_version")]
    pub mnemos_version: u32,
}

fn default_mnemos_version() -> u32 {
    1
}

impl Memory {
    pub fn new_now(id: String, tier: Tier, kind: MemoryType, title: String, body: String) -> Self {
        let now = Utc::now();
        Self {
            id,
            tier,
            kind,
            title,
            body,
            tags: vec![],
            entities: vec![],
            links: vec![],
            provenance: vec![],
            created_at: now,
            ingested_at: now,
            valid_at: now,
            invalid_at: None,
            superseded_by: None,
            strength: 1.0,
            importance: 0.5,
            last_accessed: now,
            access_count: 0,
            workspace: None,
            source_tool: None,
            mnemos_version: 1,
        }
    }

    pub fn is_valid(&self, at: DateTime<Utc>) -> bool {
        self.valid_at <= at && self.invalid_at.map_or(true, |iv| at < iv)
    }
}

/// Raw conversation chunk preserved verbatim (anti-mem0 design).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chunk {
    pub id: String,
    pub session_id: String,
    pub speaker: Option<String>,
    pub ordinal: u32,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub source_tool: Option<String>,
    pub source_meta: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub source_tool: Option<String>,
    pub workspace: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub description: Option<String>,
    pub file_path: Option<String>,
    pub created_at: DateTime<Utc>,
}
