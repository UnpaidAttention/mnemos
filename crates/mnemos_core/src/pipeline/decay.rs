use crate::error::{MnemosError, Result};
use crate::storage::Storage;
use crate::tier::Tier;
use chrono::{DateTime, Utc};
use libsql::params;
use std::str::FromStr;

/// Tunable decay parameters (half-lives in days per decaying tier, plus the
/// strength floor below which working/episodic memories are invalidated).
#[derive(Debug, Clone)]
pub struct DecayConfig {
    pub working_half_life_days: f64,
    pub episodic_half_life_days: f64,
    pub semantic_half_life_days: f64,
    pub floor: f64,
}

impl Default for DecayConfig {
    fn default() -> Self {
        Self {
            working_half_life_days: 1.0,
            episodic_half_life_days: 7.0,
            semantic_half_life_days: 90.0,
            floor: 0.05,
        }
    }
}

/// Outcome of a decay pass.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct DecayStats {
    pub scanned: usize,
    pub decayed: usize,
    /// Working/episodic memory ids that fell below the floor and should be
    /// invalidated by the caller (`Vault::run_decay`).
    pub to_invalidate: Vec<String>,
}

fn half_life_for(tier: Tier, cfg: &DecayConfig) -> Option<f64> {
    match tier {
        Tier::Working => Some(cfg.working_half_life_days),
        Tier::Episodic => Some(cfg.episodic_half_life_days),
        Tier::Semantic => Some(cfg.semantic_half_life_days),
        // procedural & reflection are not subject to time decay
        _ => None,
    }
}

fn parse_ts(s: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| MnemosError::Validation(format!("bad timestamp '{s}': {e}")))
}

/// Apply Ebbinghaus decay to every active, decaying-tier memory.
///
/// `strength' = strength * 0.5 ^ (idle_days / effective_half_life)` where
/// `effective_half_life = half_life * (1 + importance)` (important memories
/// fade slower). Updates the `strength` column; returns the ids of
/// working/episodic memories that dropped below `cfg.floor`.
pub async fn decay_pass(
    storage: &Storage,
    now: DateTime<Utc>,
    cfg: &DecayConfig,
) -> Result<DecayStats> {
    let conn = storage.conn()?;
    let mut rows = conn
        .query(
            "SELECT id, tier, strength, last_accessed, importance
               FROM memories WHERE invalid_at IS NULL",
            (),
        )
        .await?;

    let mut updates: Vec<(String, f64)> = Vec::new();
    let mut stats = DecayStats::default();
    while let Some(r) = rows.next().await? {
        stats.scanned += 1;
        let id: String = r.get(0)?;
        let tier = Tier::from_str(&r.get::<String>(1)?)?;
        let strength: f64 = r.get(2)?;
        let last = parse_ts(&r.get::<String>(3)?)?;
        let importance: f64 = r.get(4)?;
        let Some(hl) = half_life_for(tier, cfg) else {
            continue;
        };
        let idle_days = (now - last).num_seconds() as f64 / 86_400.0;
        if idle_days <= 0.0 {
            continue;
        }
        let eff_hl = hl * (1.0 + importance);
        let new_strength = (strength * 0.5_f64.powf(idle_days / eff_hl)).clamp(0.0, 1.0);
        if (new_strength - strength).abs() < 1e-6 {
            continue;
        }
        updates.push((id.clone(), new_strength));
        stats.decayed += 1;
        if matches!(tier, Tier::Working | Tier::Episodic) && new_strength < cfg.floor {
            stats.to_invalidate.push(id);
        }
    }
    drop(rows);

    let (conn, _guard) = storage.write_conn().await?;
    for (id, s) in updates {
        conn.execute(
            "UPDATE memories SET strength = ? WHERE id = ?",
            params![s, id],
        )
        .await?;
    }
    Ok(stats)
}
