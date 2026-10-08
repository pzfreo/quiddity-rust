//! Which neighbours meet a face along one of its wires (`quiddity._wire_seed`): traversal and
//! acceptance stay with the callers.

use std::collections::BTreeSet;

use super::graph::shared_occurrences;
use crate::kernel::brep::Part;

/// The neighbours of *opening* that share an edge of *wire* with it through an exactly paired
/// occurrence (`wire_seed` over `FaceGraph.neighbours_by_occurrence_edge`).
pub fn wire_seed(part: &Part, opening: usize, wire: &[usize]) -> BTreeSet<usize> {
    part.neighbours(opening)
        .into_iter()
        .filter(|&n| {
            shared_occurrences(part, opening, n)
                .iter()
                .any(|e| wire.contains(e))
        })
        .collect()
}

/// The edges of each of the face's inner loops (`face.inner_wires()`): every loop but the outer
/// one. A face whose outer loop cannot be told has none read, so nothing is seeded from it.
pub fn inner_wires(part: &Part, face: usize) -> Vec<Vec<usize>> {
    let loops = &part.faces[face].loops;
    if loops.len() < 2 {
        return Vec::new();
    }
    let Some(outer) = part.outer_loop(face) else {
        return Vec::new();
    };
    loops
        .iter()
        .enumerate()
        .filter(|&(l, lp)| l != outer && !lp.edges.is_empty())
        .map(|(_, lp)| lp.edges.iter().map(|&(e, _)| e).collect())
        .collect()
}
