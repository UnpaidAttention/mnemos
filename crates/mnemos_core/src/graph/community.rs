//! Single-level Louvain modularity community detection. Hand-rolled,
//! deterministic. Returns a contiguous community id (`0..k`) per node index.

use crate::graph::MemoryGraph;
use std::collections::HashMap;

/// Detect communities by greedy local modularity optimization. Each node starts
/// in its own community; nodes repeatedly move to the neighboring community that
/// most increases modularity until no move helps. Deterministic: nodes are
/// considered in index order and ties favor the lower community id.
pub fn louvain(graph: &MemoryGraph) -> Vec<usize> {
    let n = graph.node_count();
    if n == 0 {
        return vec![];
    }
    let m = graph.total_weight();
    if m <= 0.0 {
        // No edges: every node is its own community.
        return renumber(&(0..n).collect::<Vec<_>>());
    }
    let two_m = 2.0 * m;
    let mut comm: Vec<usize> = (0..n).collect();
    let mut sigma_tot: Vec<f64> = (0..n).map(|i| graph.degree(i)).collect();

    let mut improved = true;
    let mut guard = 0;
    while improved && guard < 100 {
        improved = false;
        guard += 1;
        for i in 0..n {
            let ki = graph.degree(i);
            let ci = comm[i];

            // Weight from i to each neighboring community.
            let mut k_i_to: HashMap<usize, f64> = HashMap::new();
            for &(j, w) in graph.neighbors(i) {
                if j == i {
                    continue;
                }
                *k_i_to.entry(comm[j]).or_insert(0.0) += w;
            }

            // Tentatively remove i from its community.
            sigma_tot[ci] -= ki;

            // Baseline: returning to ci.
            let mut best_comm = ci;
            let mut best_gain =
                k_i_to.get(&ci).copied().unwrap_or(0.0) - sigma_tot[ci] * ki / two_m;

            // Evaluate neighbor communities in deterministic (sorted) order.
            let mut cands: Vec<usize> = k_i_to.keys().copied().collect();
            cands.sort_unstable();
            for c in cands {
                if c == ci {
                    continue;
                }
                let gain = k_i_to[&c] - sigma_tot[c] * ki / two_m;
                if gain > best_gain + 1e-12 {
                    best_gain = gain;
                    best_comm = c;
                }
            }

            sigma_tot[best_comm] += ki;
            if best_comm != ci {
                comm[i] = best_comm;
                improved = true;
            }
        }
    }
    renumber(&comm)
}

/// Map arbitrary community labels to contiguous ids `0..k` (first-seen order).
fn renumber(comm: &[usize]) -> Vec<usize> {
    let mut map: HashMap<usize, usize> = HashMap::new();
    let mut next = 0;
    comm.iter()
        .map(|&c| {
            *map.entry(c).or_insert_with(|| {
                let v = next;
                next += 1;
                v
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::MemoryGraph;

    #[test]
    fn separates_two_triangles() {
        // Two dense triangles joined by a single weak bridge → two communities.
        let mut g = MemoryGraph::new();
        for (a, b) in [("A", "B"), ("B", "C"), ("A", "C")] {
            g.add_edge(a, b, 1.0);
        }
        for (a, b) in [("D", "E"), ("E", "F"), ("D", "F")] {
            g.add_edge(a, b, 1.0);
        }
        g.add_edge("C", "D", 0.3); // weak bridge

        let comm = louvain(&g);
        let c = |name: &str| comm[g.index_of(name).unwrap()];
        assert_eq!(c("A"), c("B"));
        assert_eq!(c("B"), c("C"));
        assert_eq!(c("D"), c("E"));
        assert_eq!(c("E"), c("F"));
        assert_ne!(c("A"), c("D"), "the two triangles are distinct communities");
        let distinct: std::collections::BTreeSet<usize> = comm.iter().copied().collect();
        assert_eq!(distinct.len(), 2);
    }

    #[test]
    fn isolated_nodes_are_singletons() {
        let mut g = MemoryGraph::new();
        g.add_mention("m", "X"); // node with no edges
        g.add_mention("m", "Y");
        let comm = louvain(&g);
        assert_eq!(comm.len(), 2);
        assert_ne!(comm[0], comm[1]);
    }

    #[test]
    fn empty_graph_empty_result() {
        assert!(louvain(&MemoryGraph::new()).is_empty());
    }
}
