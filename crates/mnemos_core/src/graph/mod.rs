//! Dependency-free in-memory projection of the entity graph, backing
//! Personalized PageRank ([`ppr`]) and Louvain community detection
//! ([`community`]). Built from `entity_edges` + `entity_mentions` by
//! [`MemoryGraph::load`] ([`build`]).

pub mod build;
pub mod community;
pub mod ppr;

use std::collections::HashMap;

/// Entities are nodes (indexed `0..node_count`); edges are undirected and
/// weighted (active edges only); memory↔entity mentions are tracked both ways.
#[derive(Debug, Default, Clone)]
pub struct MemoryGraph {
    entity_ids: Vec<String>,
    index_of: HashMap<String, usize>,
    /// adj[i] = [(neighbor_index, weight), ...]
    adj: Vec<Vec<(usize, f64)>>,
    /// Sum of incident edge weights per node (PPR normalization + Louvain).
    degree: Vec<f64>,
    mem_to_entities: HashMap<String, Vec<usize>>,
    entity_to_mems: Vec<Vec<String>>,
}

impl MemoryGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn node_count(&self) -> usize {
        self.entity_ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entity_ids.is_empty()
    }

    pub fn index_of(&self, entity_id: &str) -> Option<usize> {
        self.index_of.get(entity_id).copied()
    }

    pub fn entity_id(&self, i: usize) -> &str {
        &self.entity_ids[i]
    }

    pub fn neighbors(&self, i: usize) -> &[(usize, f64)] {
        &self.adj[i]
    }

    pub fn degree(&self, i: usize) -> f64 {
        self.degree[i]
    }

    /// Total edge weight `m` = sum of degrees / 2 (used by Louvain modularity).
    pub fn total_weight(&self) -> f64 {
        self.degree.iter().sum::<f64>() / 2.0
    }

    pub fn memories_for_entity(&self, i: usize) -> &[String] {
        &self.entity_to_mems[i]
    }

    pub fn entities_for_memory(&self, memory_id: &str) -> Option<&Vec<usize>> {
        self.mem_to_entities.get(memory_id)
    }

    fn ensure_node(&mut self, entity_id: &str) -> usize {
        if let Some(&i) = self.index_of.get(entity_id) {
            return i;
        }
        let i = self.entity_ids.len();
        self.entity_ids.push(entity_id.to_string());
        self.index_of.insert(entity_id.to_string(), i);
        self.adj.push(Vec::new());
        self.degree.push(0.0);
        self.entity_to_mems.push(Vec::new());
        i
    }

    /// Add an undirected weighted edge (creating nodes as needed). A repeated
    /// edge accumulates its weight. Self-loops are ignored.
    pub fn add_edge(&mut self, a: &str, b: &str, weight: f64) {
        let ia = self.ensure_node(a);
        let ib = self.ensure_node(b);
        if ia == ib {
            return;
        }
        Self::accumulate(&mut self.adj[ia], ib, weight);
        Self::accumulate(&mut self.adj[ib], ia, weight);
        self.degree[ia] += weight;
        self.degree[ib] += weight;
    }

    fn accumulate(list: &mut Vec<(usize, f64)>, neighbor: usize, weight: f64) {
        if let Some(e) = list.iter_mut().find(|(n, _)| *n == neighbor) {
            e.1 += weight;
        } else {
            list.push((neighbor, weight));
        }
    }

    /// Record that `memory_id` mentions `entity_id` (creating the node as
    /// needed). Idempotent in both directions.
    pub fn add_mention(&mut self, memory_id: &str, entity_id: &str) {
        let i = self.ensure_node(entity_id);
        if !self.entity_to_mems[i].iter().any(|m| m == memory_id) {
            self.entity_to_mems[i].push(memory_id.to_string());
        }
        let v = self
            .mem_to_entities
            .entry(memory_id.to_string())
            .or_default();
        if !v.contains(&i) {
            v.push(i);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_nodes_edges_and_mentions() {
        let mut g = MemoryGraph::new();
        g.add_edge("A", "B", 1.0);
        g.add_edge("B", "C", 2.0);
        g.add_edge("A", "B", 1.0); // accumulates onto the existing edge
        g.add_mention("mem1", "A");
        g.add_mention("mem1", "A"); // idempotent
        g.add_mention("mem2", "C");

        assert_eq!(g.node_count(), 3);
        let a = g.index_of("A").unwrap();
        let b = g.index_of("B").unwrap();
        // A-B weight accumulated to 2.0; A's degree = 2.0 (only neighbor B)
        assert_eq!(g.degree(a), 2.0);
        // B touches A(2.0) + C(2.0) => degree 4.0
        assert_eq!(g.degree(b), 4.0);
        assert_eq!(g.memories_for_entity(a), &["mem1".to_string()]);
        assert_eq!(
            g.entities_for_memory("mem2").unwrap(),
            &[g.index_of("C").unwrap()]
        );
        assert!(!g.is_empty());
    }
}
