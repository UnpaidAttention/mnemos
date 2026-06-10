//! Co-mention edge inference: creates edges between entities that are
//! mentioned in the same memory. This is purely database-driven — no LLM
//! call required — and produces the highest-quality connectivity signal
//! for the knowledge graph.

use crate::error::Result;
use crate::storage::entity_ops::upsert_edge;
use crate::storage::Storage;
use chrono::{DateTime, Utc};
use libsql::params;

/// Create edges between all entities co-mentioned in the given memory.
///
/// For each pair (E1, E2) where both are mentioned in `memory_id`, an edge
/// with relation `"co-mentioned in"` is upserted. If the edge already exists
/// (from a prior backfill or real-time run), its weight is incremented and
/// the memory is appended to its provenance list.
///
/// Returns the number of edges created or reinforced.
pub async fn create_co_mention_edges(
    storage: &Storage,
    memory_id: &str,
    valid_at: DateTime<Utc>,
) -> Result<usize> {
    let conn = storage.conn()?;

    // Fetch all entity IDs mentioned in this memory
    let mut rows = conn
        .query(
            "SELECT entity_id FROM entity_mentions WHERE memory_id = ? ORDER BY entity_id",
            params![memory_id.to_string()],
        )
        .await?;

    let mut entity_ids: Vec<String> = Vec::new();
    while let Some(r) = rows.next().await? {
        entity_ids.push(r.get::<String>(0)?);
    }
    drop(rows);

    // For fewer than 2 entities, no pairs exist
    if entity_ids.len() < 2 {
        return Ok(0);
    }

    // Create edges for every unique pair (i, j) where i < j
    // This avoids duplicate (A→B, B→A) edges
    let mut count = 0usize;
    for i in 0..entity_ids.len() {
        for j in (i + 1)..entity_ids.len() {
            upsert_edge(
                storage,
                &entity_ids[i],
                &entity_ids[j],
                "co-mentioned in",
                memory_id,
                valid_at,
            )
            .await?;
            count += 1;
        }
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Paths;
    use crate::storage::entity_ops::{link_entity_mention, upsert_entity};
    use crate::vault::{RememberOpts, Vault};
    use tempfile::TempDir;

    #[tokio::test]
    async fn co_mention_creates_edges_for_pairs() {
        let tmp = TempDir::new().unwrap();
        let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();

        // Create 3 entities
        let e1 = upsert_entity(v.storage(), "Rust", "tool", None).await.unwrap();
        let e2 = upsert_entity(v.storage(), "Mnemos", "project", None).await.unwrap();
        let e3 = upsert_entity(v.storage(), "libsql", "tool", None).await.unwrap();

        // Create a memory and link all 3 entities to it
        let mem = v.remember("Mnemos uses Rust and libsql", RememberOpts::default()).await.unwrap();
        link_entity_mention(v.storage(), &mem, &e1).await.unwrap();
        link_entity_mention(v.storage(), &mem, &e2).await.unwrap();
        link_entity_mention(v.storage(), &mem, &e3).await.unwrap();

        // Run co-mention
        let count = create_co_mention_edges(v.storage(), &mem, Utc::now()).await.unwrap();

        // 3 entities → 3 pairs: (e1,e2), (e1,e3), (e2,e3)
        assert_eq!(count, 3);

        // Verify edges exist
        let conn = v.storage().conn().unwrap();
        let mut rows = conn
            .query(
                "SELECT COUNT(*) FROM entity_edges WHERE relation = 'co-mentioned in'",
                (),
            )
            .await
            .unwrap();
        let edge_count: i64 = rows.next().await.unwrap().unwrap().get(0).unwrap();
        assert_eq!(edge_count, 3);
    }

    #[tokio::test]
    async fn co_mention_single_entity_creates_no_edges() {
        let tmp = TempDir::new().unwrap();
        let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();

        let e1 = upsert_entity(v.storage(), "Solo", "tool", None).await.unwrap();
        let mem = v.remember("Only one entity here", RememberOpts::default()).await.unwrap();
        link_entity_mention(v.storage(), &mem, &e1).await.unwrap();

        let count = create_co_mention_edges(v.storage(), &mem, Utc::now()).await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn co_mention_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();

        let e1 = upsert_entity(v.storage(), "A", "x", None).await.unwrap();
        let e2 = upsert_entity(v.storage(), "B", "x", None).await.unwrap();
        let mem = v.remember("both A and B", RememberOpts::default()).await.unwrap();
        link_entity_mention(v.storage(), &mem, &e1).await.unwrap();
        link_entity_mention(v.storage(), &mem, &e2).await.unwrap();

        // Run twice
        create_co_mention_edges(v.storage(), &mem, Utc::now()).await.unwrap();
        create_co_mention_edges(v.storage(), &mem, Utc::now()).await.unwrap();

        // Should still be 1 edge (upsert reinforces, doesn't duplicate)
        let conn = v.storage().conn().unwrap();
        let mut rows = conn
            .query(
                "SELECT COUNT(*) FROM entity_edges WHERE relation = 'co-mentioned in'",
                (),
            )
            .await
            .unwrap();
        let edge_count: i64 = rows.next().await.unwrap().unwrap().get(0).unwrap();
        assert_eq!(edge_count, 1);
    }
}
