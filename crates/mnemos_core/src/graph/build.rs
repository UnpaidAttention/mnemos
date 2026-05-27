//! Build a [`MemoryGraph`] from storage.

use crate::error::Result;
use crate::graph::MemoryGraph;
use crate::storage::Storage;

impl MemoryGraph {
    /// Build the graph from active entity edges plus mentions by still-valid
    /// memories. Invalid memories' mentions are excluded so PPR only ranks
    /// memories that are currently valid.
    pub async fn load(storage: &Storage) -> Result<Self> {
        let mut g = MemoryGraph::new();
        let conn = storage.conn()?;

        let mut edges = conn
            .query(
                "SELECT source_entity_id, target_entity_id, weight
                   FROM entity_edges WHERE invalid_at IS NULL",
                (),
            )
            .await?;
        while let Some(r) = edges.next().await? {
            let a: String = r.get(0)?;
            let b: String = r.get(1)?;
            let w: f64 = r.get(2)?;
            g.add_edge(&a, &b, w.max(0.0));
        }
        drop(edges);

        let mut mentions = conn
            .query(
                "SELECT em.memory_id, em.entity_id
                   FROM entity_mentions em
                   JOIN memories m ON m.id = em.memory_id
                  WHERE m.invalid_at IS NULL",
                (),
            )
            .await?;
        while let Some(r) = mentions.next().await? {
            let memory_id: String = r.get(0)?;
            let entity_id: String = r.get(1)?;
            g.add_mention(&memory_id, &entity_id);
        }

        Ok(g)
    }
}
