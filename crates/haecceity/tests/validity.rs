//! Per-face geometric validity (`haecceity::validity`) against OpenCascade's `BRepCheck_Analyzer`
//! over the corpus.
//!
//! Every solid of every corpus part, checked with `BRepCheck_Analyzer` after OpenCascade's
//! import, is valid but two: `inventory_refusal/14052.step.gz`, whose planar face 1 is
//! `BRepCheck_BadOrientationOfSubshape` (after the import's healing has split its circular hole
//! where, in the file, it crosses the outer loop, and turned the triangular hole's loop round;
//! the face rebuilt in OpenCascade from the file's loops is `BRepCheck_IntersectingWires`, the
//! fault the kernel finds), and `cadgenbench_inputs/cgb202.step.gz`, whose face 1654 is
//! `BRepCheck_UnorientableShape` with a `BRepCheck_SelfIntersectingWire`, made by the import's
//! healing (ShapeFix) re-orienting wires: the file's own loops are closed and consistently
//! oriented (`known_divergences.json`, this file's evidence entries, rust-correct). Every other
//! face, wire, edge, shell and solid status is `BRepCheck_NoError`. So the kernel must fault
//! 14052's face 1 and nothing else, however the part is placed.

mod common;

use haecceity::Part;
use haecceity::step::{Placement, read_step_file, read_step_file_placed};

/// The faces each corpus part's solids must fault, by solid; every other part's none.
const FAULTED: [(&str, usize, &[usize]); 1] = [("inventory_refusal/14052.step.gz", 0, &[1])];

/// A rigid motion moving every axis and the origin.
const MOVED: Placement = [
    [0.0, 0.0, 1.0, 123.456],
    [1.0, 0.0, 0.0, -78.9],
    [0.0, 1.0, 0.0, 41.3],
];

/// Each solid's faulted faces, with the solids holding none left out.
fn faulted(part: &Part) -> Vec<(usize, Vec<usize>)> {
    (0..part.solids.len())
        .map(|s| (s, part.bad_faces(s).to_vec()))
        .filter(|(_, bad)| !bad.is_empty())
        .collect()
}

#[test]
fn malformed_part_faults_its_crossing_face() {
    let Some(dir) = common::corpus_dir() else {
        assert!(std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none());
        return;
    };
    let part = read_step_file(&dir.join("inventory_refusal/14052.step.gz")).unwrap();
    assert_eq!(part.solids.len(), 1);
    // Topologically sound, so only the geometric check refuses it.
    assert!(part.solid_is_valid(0));
    assert_eq!(part.bad_faces(0), &[1]);
    assert!(!part.solid_is_geometrically_valid(0));
}

#[test]
fn faults_agree_with_opencascade_over_the_corpus() {
    let Some(dir) = common::corpus_dir() else {
        assert!(std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none());
        return;
    };
    let corpus = common::load("corpus.json");
    let files: Vec<String> = corpus["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["file"].as_str().unwrap().to_owned())
        .collect();
    let problems: Vec<String> = common::parallel::map(&files, |file| {
        let want: Vec<(usize, Vec<usize>)> = FAULTED
            .iter()
            .filter(|(f, _, _)| f == file)
            .map(|&(_, s, faces)| (s, faces.to_vec()))
            .collect();
        let path = dir.join(file);
        let mut out = Vec::new();
        for (placed, part) in [
            ("as read", read_step_file(&path)),
            ("moved", read_step_file_placed(&path, &MOVED)),
        ] {
            let got = faulted(&part.unwrap_or_else(|e| panic!("{file}: {e}")));
            if got != want {
                out.push(format!("{file} ({placed}): faulted {got:?}, want {want:?}"));
            }
        }
        out
    })
    .into_iter()
    .flatten()
    .collect();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
