//! Personalized PageRank (random walk with restart) over the entity graph,
//! via power iteration. Hand-rolled; deterministic.
//!
//! Convention: `alpha` is the standard PageRank damping (walk) factor — at each
//! step a walker continues along edges with probability `alpha` and teleports
//! back to the seed set with probability `1 - alpha`. `alpha = 0.85` therefore
//! gives strong multi-hop spreading (the HippoRAG property), while still pinning
//! mass to the seeds' connected component.

use crate::graph::MemoryGraph;
use crate::retrieval::rrf::RankedId;
use std::collections::HashMap;

/// Personalized PageRank. Returns a score per node index. The restart
/// distribution is uniform over `seeds`; mass at dangling (edgeless) nodes is
/// redistributed to the restart set. With no seeds, returns zeros.
///
/// Nodes unreachable from the seed set receive zero mass (teleport only ever
/// targets the seeds), so PPR localizes retrieval to the seeds' connected
/// component.
pub fn personalized_pagerank(
    graph: &MemoryGraph,
    seeds: &[usize],
    alpha: f64,
    iterations: usize,
) -> Vec<f64> {
    let n = graph.node_count();
    if n == 0 || seeds.is_empty() {
        return vec![0.0; n];
    }
    let mut restart = vec![0.0; n];
    let seed_mass = 1.0 / seeds.len() as f64;
    for &s in seeds {
        if s < n {
            restart[s] += seed_mass;
        }
    }
    let mut r = restart.clone();
    for _ in 0..iterations {
        let mut next = vec![0.0; n];
        let mut dangling = 0.0;
        for (i, &ri) in r.iter().enumerate() {
            let deg = graph.degree(i);
            if deg <= 0.0 {
                dangling += ri;
                continue;
            }
            for &(j, w) in graph.neighbors(i) {
                next[j] += ri * (w / deg);
            }
        }
        // r' = (1-alpha)*restart + alpha*(walk + dangling*restart)
        for (i, slot) in next.iter_mut().enumerate() {
            *slot = (1.0 - alpha) * restart[i] + alpha * (*slot + dangling * restart[i]);
        }
        r = next;
    }
    r
}

/// Rank memories by the summed PPR score of the entities they mention.
/// Deterministic: sorted by score descending, then memory id ascending.
pub fn ppr_rank_memories(graph: &MemoryGraph, scores: &[f64]) -> Vec<RankedId> {
    let mut acc: HashMap<&str, f64> = HashMap::new();
    for i in 0..graph.node_count() {
        let s = scores.get(i).copied().unwrap_or(0.0);
        if s <= 0.0 {
            continue;
        }
        for mem in graph.memories_for_entity(i) {
            *acc.entry(mem.as_str()).or_insert(0.0) += s;
        }
    }
    let mut scored: Vec<(&str, f64)> = acc.into_iter().collect();
    scored.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(b.0))
    });
    scored
        .into_iter()
        .enumerate()
        .map(|(i, (id, _))| RankedId {
            id: id.to_string(),
            rank: i + 1,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::MemoryGraph;

    /// Path component A—B—C (with memories) plus a disconnected component X—Y.
    fn two_component_graph() -> MemoryGraph {
        let mut g = MemoryGraph::new();
        g.add_edge("A", "B", 1.0);
        g.add_edge("B", "C", 1.0);
        g.add_edge("X", "Y", 1.0); // separate component, unreachable from A
        g.add_mention("memA", "A");
        g.add_mention("memC", "C");
        g.add_mention("memX", "X");
        g
    }

    #[test]
    fn ppr_localizes_to_the_seed_component() {
        // Seeded on A: every node reachable from A gets positive mass; nodes in
        // the disconnected X—Y component get exactly zero (teleport only targets
        // the seed). Robust to bipartite power-iteration oscillation, and the
        // property retrieval actually relies on.
        let g = two_component_graph();
        let seed = g.index_of("A").unwrap();
        let scores = personalized_pagerank(&g, &[seed], 0.85, 30);

        assert!(scores[g.index_of("A").unwrap()] > 0.0);
        assert!(scores[g.index_of("B").unwrap()] > 0.0);
        assert!(
            scores[g.index_of("C").unwrap()] > 0.0,
            "multi-hop node reachable"
        );
        assert_eq!(
            scores[g.index_of("X").unwrap()],
            0.0,
            "other component excluded"
        );
        assert_eq!(
            scores[g.index_of("Y").unwrap()],
            0.0,
            "other component excluded"
        );
    }

    #[test]
    fn no_seeds_yields_zero_vector() {
        let g = two_component_graph();
        let scores = personalized_pagerank(&g, &[], 0.85, 30);
        assert!(scores.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn ranks_reachable_memories_and_excludes_unreachable() {
        let g = two_component_graph();
        let seed = g.index_of("A").unwrap();
        let scores = personalized_pagerank(&g, &[seed], 0.85, 30);
        let ranked = ppr_rank_memories(&g, &scores);

        let ids: Vec<&str> = ranked.iter().map(|r| r.id.as_str()).collect();
        assert!(ids.contains(&"memA"), "seed memory ranked");
        assert!(ids.contains(&"memC"), "multi-hop memory ranked");
        assert!(!ids.contains(&"memX"), "unreachable memory excluded");
    }
}
