//! Conservative body evidence, evaluated only in the inferred part frame.
//! The complete rule and abstentions are specified in docs/face-accounting.md.
use std::collections::BTreeMap;

use crate::Part;
use crate::kernel::geom::{Curve, Surface};
use crate::recognition::BodyReason;

pub(crate) fn evidence(part: &Part) -> BTreeMap<usize, BodyReason> {
    let mut out = BTreeMap::new();
    for (solid, shell) in part.solids.iter().enumerate() {
        // Freeform bounds can be sampled: they cannot establish a supporting plane.
        if shell.faces.iter().any(|&f| {
            !matches!(
                part.faces[f].surface,
                Surface::Plane { .. }
                    | Surface::Cylinder { .. }
                    | Surface::Cone { .. }
                    | Surface::Sphere { .. }
                    | Surface::Torus { .. }
            ) || part.face_edges(f).iter().any(|&e| {
                !matches!(
                    part.edges[e].curve,
                    Curve::Line { .. } | Curve::Circle { .. } | Curve::Ellipse { .. }
                )
            })
        }) || !part.solid_is_geometrically_valid(solid)
        {
            continue;
        }
        let bounds = part.solid_bounds(solid);
        let tol = 1e-7 * bounds.max_extent().max(1.0);
        if (0..3).any(|k| {
            !bounds.min[k].is_finite()
                || !bounds.max[k].is_finite()
                || bounds.max[k] - bounds.min[k] <= 100.0 * tol
        }) {
            continue;
        }
        for &f in &shell.faces {
            let face = &part.faces[f];
            if face.solid != Some(solid) || !matches!(face.surface, Surface::Plane { .. }) {
                continue;
            }
            let Some(normal) = part.face_normal(f, 0.0, 0.0) else {
                continue;
            };
            let Some(axis) = (0..3).find(|&k| {
                normal[k].abs() >= 1.0 - 1e-12
                    && (0..3).filter(|&j| j != k).all(|j| normal[j].abs() <= 1e-9)
            }) else {
                continue;
            };
            let at = if normal[axis] > 0.0 {
                bounds.max[axis]
            } else {
                bounds.min[axis]
            };
            let transverse: Vec<_> = (0..3).filter(|&j| j != axis).collect();
            // An entire rectangular box side is stronger evidence than merely reaching
            // the extrema: a notched side, boss cap or a chamfer cannot pass this test.
            let rectangle = face.loops.iter().any(|lp| {
                if lp.vertex.is_some() || lp.edges.len() != 4 {
                    return false;
                }
                let mut corners = Vec::new();
                for &(e, forward) in &lp.edges {
                    let edge = &part.edges[e];
                    if !matches!(edge.curve, Curve::Line { .. }) {
                        return false;
                    }
                    let corner = |p: [f64; 3]| -> Option<usize> {
                        if (p[axis] - at).abs() > tol {
                            return None;
                        }
                        let mut code = 0;
                        for (bit, &j) in transverse.iter().enumerate() {
                            if (p[j] - bounds.min[j]).abs() <= tol {
                            } else if (p[j] - bounds.max[j]).abs() <= tol {
                                code |= 1 << bit;
                            } else {
                                return None;
                            }
                        }
                        Some(code)
                    };
                    let (Some(start), Some(end)) = (corner(edge.start), corner(edge.end)) else {
                        return false;
                    };
                    if (start ^ end).count_ones() != 1 {
                        return false;
                    }
                    corners.push(if forward { (start, end) } else { (end, start) });
                }
                (0..4).all(|i| corners[i].1 == corners[(i + 1) % 4].0)
                    && (0..4).all(|code| corners.iter().filter(|c| c.0 == code).count() == 1)
            });
            if rectangle {
                out.insert(f, BodyReason::StockBoxSide);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frames;
    use crate::kernel::step::Placement;
    use std::path::{Path, PathBuf};

    fn normalized_evidence(part: &Part) -> BTreeMap<usize, BodyReason> {
        frames::infer_part_frame(part)
            .ok()
            .and_then(|f| part.transformed(&f.normalisation()).ok())
            .map_or_else(BTreeMap::new, |p| evidence(&p))
    }

    fn rotation(axis: usize, degrees: f64, translation: [f64; 3]) -> Placement {
        let (s, c) = degrees.to_radians().sin_cos();
        let (i, j) = ((axis + 1) % 3, (axis + 2) % 3);
        let mut p = crate::kernel::step::IDENTITY;
        p[i][i] = c;
        p[j][j] = c;
        p[i][j] = -s;
        p[j][i] = s;
        for k in 0..3 {
            p[k][3] = translation[k];
        }
        p
    }

    #[test]
    fn body_evidence_survives_z30_x30_and_z17_x23_on_goldens_and_corpus() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let mut paths: Vec<_> = std::fs::read_dir(&fixtures)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("golden_")
                    && p.extension().is_some_and(|e| e == "step")
            })
            .collect();
        if let Some(root) = std::env::var_os("QUIDDITY_CORPUS").filter(|r| r != "off") {
            let corpus: serde_json::Value =
                serde_json::from_str(include_str!("../tests/fixtures/corpus.json")).unwrap();
            paths.extend(
                corpus["files"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|e| PathBuf::from(&root).join(e["file"].as_str().unwrap())),
            );
        } else {
            assert!(
                std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
                "QUIDDITY_CORPUS required"
            );
        }
        let t = [137.0, -83.0, 49.0];
        let z17 = rotation(2, 17.0, [0.0; 3]);
        let x23 = rotation(0, 23.0, t);
        let composed = std::array::from_fn(|i| {
            std::array::from_fn(|j| {
                (0..3).map(|k| x23[i][k] * z17[k][j]).sum::<f64>()
                    + if j == 3 { x23[i][3] } else { 0.0 }
            })
        });
        let motions = [
            ("Z30", rotation(2, 30.0, t)),
            ("X30", rotation(0, 30.0, t)),
            ("Z17+X23", composed),
        ];
        let mut body_count = 0;
        for path in paths {
            let part = crate::read_step_file(&path).unwrap();
            let expected = normalized_evidence(&part);
            body_count += expected.len();
            for (name, motion) in &motions {
                let moved = part.transformed(motion).unwrap();
                assert_eq!(
                    normalized_evidence(&moved),
                    expected,
                    "{} {name}",
                    path.display()
                );
            }
        }
        assert!(body_count > 0, "invariance must exercise positive evidence");
    }

    #[test]
    fn hole_skin_is_body_but_pocket_floors_and_partial_boss_caps_are_not() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let hole =
            crate::read_step_file(&fixtures.join("golden_simple_through_hole.step")).unwrap();
        let body = normalized_evidence(&hole);
        // This fixture also has a boss rising above the block. Only the bottom
        // spans the complete solid box; the four short sides and boss cap abstain.
        assert_eq!(body.keys().copied().collect::<Vec<_>>(), vec![4]);
        assert!(!body.contains_key(&8));
        assert!(
            body.keys()
                .all(|&f| matches!(hole.faces[f].surface, Surface::Plane { .. }))
        );
        for name in [
            "golden_blind_pockets_and_pocket_patterns.step",
            "golden_plates_pads_levels_and_slanted_steps.step",
        ] {
            let part = crate::read_step_file(&fixtures.join(name)).unwrap();
            let frame = frames::infer_part_frame(&part).unwrap();
            let part = part.transformed(&frame.normalisation()).unwrap();
            let body = evidence(&part);
            let mut excluded = 0;
            for (f, face) in part.faces.iter().enumerate() {
                if !matches!(face.surface, Surface::Plane { .. }) {
                    continue;
                }
                let solid = face.solid.unwrap();
                let b = part.solid_bounds(solid);
                let fb = part.face_bounds(f);
                // Any plane strictly inside a solid extent, or a partial cap,
                // is excluded even if no recognizer ever claims it.
                let covers_side = (0..3).any(|k| {
                    (fb.max[k] - fb.min[k]).abs() < 1e-6
                        && ((fb.min[k] - b.min[k]).abs() < 1e-6
                            || (fb.min[k] - b.max[k]).abs() < 1e-6)
                        && (0..3).filter(|&j| j != k).all(|j| {
                            (fb.min[j] - b.min[j]).abs() < 1e-6
                                && (fb.max[j] - b.max[j]).abs() < 1e-6
                        })
                });
                if !covers_side {
                    assert!(!body.contains_key(&f), "{name} face {f}");
                    excluded += 1;
                }
            }
            assert!(excluded > 0, "{name} must exercise negative evidence");
        }
        let shell = Part::new(hole.faces.clone(), hole.edges.clone(), vec![]);
        assert!(evidence(&shell).is_empty());
    }
}
