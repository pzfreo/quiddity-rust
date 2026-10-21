//! Physical trim lengths and source identities for independent raw-STEP checks.
// Internal diagnostic: use the exact engine implementation without exporting caller-space APIs.
#[path = "../src/engine.rs"]
mod engine;
pub use engine::*;
extern crate self as quiddity;
use quiddity::features::effective_surfaces::{arc_length, physical_boundary_length};

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let part = quiddity::read_step_file(std::path::Path::new(&args[1])).expect("read STEP");
    let faces: Vec<_> = args[2..]
        .iter()
        .map(|s| {
            let index: usize = s.parse().expect("face index");
            let mut uses = std::collections::BTreeMap::<usize, usize>::new();
            for lp in &part.faces[index].loops {
                for &(edge, _) in &lp.edges {
                    *uses.entry(edge).or_default() += 1;
                }
            }
            let edges: Vec<_> = uses
                .iter()
                .filter(|&(_, &count)| count == 1)
                .map(|(&edge, _)| {
                    serde_json::json!({
                        "edge": edge, "entity": part.edge_source(edge).map(|s| s.entity),
                        "length": arc_length(&part, edge),
                    })
                })
                .collect();
            serde_json::json!({
                "face": index, "perimeter": physical_boundary_length(&part, index),
                "edges": edges,
            })
        })
        .collect();
    println!("{}", serde_json::json!({"file": args[1], "faces": faces}));
}
