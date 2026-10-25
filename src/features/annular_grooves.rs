//! Blind annular face grooves: a planar annulus between two complete coaxial cylindrical
//! walls, both rising to the same open plane. This is a face-seal recess, not a turned
//! external groove or a nominal O-ring size. Interrupted walls and treated rims are refused.

use std::collections::BTreeMap;

use serde::Serialize;

use super::Context;
use super::evidence::{self, EvidenceStatus, Occurrence};
use super::graph::normal;
use super::regions::length_tolerance;
use crate::kernel::brep::{Arc, Part};
use crate::kernel::geom::{self, Curve, Surface, V3};

/// A rectangular-section annular recess, in millimetres. `axis` is the unit direction from
/// `floor_center` towards the mouth; both cylinder walls have the same `depth`. No nominal
/// seal dimensions, machining process or intended function are inferred.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AnnularGroove {
    pub axis: V3,
    pub floor_center: V3,
    pub inner_radius: f64,
    pub outer_radius: f64,
    pub depth: f64,
}

#[derive(Clone, Copy)]
struct Circle {
    edge: usize,
    center: V3,
    radius: f64,
}

fn circle(part: &Part, edge: usize, axis: V3) -> Option<Circle> {
    let e = &part.edges[edge];
    let Curve::Circle { frame, radius } = e.curve else {
        return None;
    };
    (e.is_closed() && radius > 0.0 && geom::dot(frame.z, axis).abs() >= 1.0 - 1e-9).then_some(
        Circle {
            edge,
            center: frame.origin,
            radius,
        },
    )
}

/// A complete wall has exactly two circular rims and a straight seam used twice, in
/// opposite senses. Nothing else may interrupt it. Its far rim meets an outward-facing
/// planar mouth convexly; its floor rim meets the annulus concavely.
fn wall(
    part: &Part,
    floor: usize,
    rim: Circle,
    axis: V3,
    outer: bool,
) -> Option<(usize, usize, f64)> {
    let faces = &part.edge_faces()[rim.edge];
    let [a, b] = faces[..] else { return None };
    let face = if a == floor {
        b
    } else if b == floor {
        a
    } else {
        return None;
    };
    let f = &part.faces[face];
    let Surface::Cylinder { frame, radius } = f.surface else {
        return None;
    };
    let tolerance = length_tolerance(&[radius, rim.radius]);
    let offset = geom::sub(rim.center, frame.origin);
    if f.reversed != outer
        || geom::dot(frame.z, axis).abs() < 1.0 - 1e-9
        || geom::norm(geom::cross(offset, axis)) > tolerance
        || (radius - rim.radius).abs() > tolerance
        || part.arc(floor, face) != Some(Arc::Concave)
    {
        return None;
    }
    let [lp] = &f.loops[..] else { return None };
    if lp.edges.len() != 4 {
        return None;
    }
    let mut uses: BTreeMap<usize, Vec<bool>> = BTreeMap::new();
    for &(edge, forward) in &lp.edges {
        uses.entry(edge).or_default().push(forward);
    }
    if uses.len() != 3 || uses.get(&rim.edge)?.len() != 1 {
        return None;
    }
    let mut other_rim = None;
    let mut seam = false;
    for (&edge, directions) in &uses {
        if edge == rim.edge {
            continue;
        }
        match &part.edges[edge].curve {
            Curve::Circle { .. } if directions.len() == 1 => {
                other_rim = Some(circle(part, edge, axis)?);
            }
            Curve::Line { dir, .. }
                if directions.len() == 2
                    && directions[0] != directions[1]
                    && geom::dot(geom::unit(*dir)?, axis).abs() >= 1.0 - 1e-9 =>
            {
                seam = true;
            }
            _ => return None,
        }
    }
    if !seam {
        return None;
    }
    let other = other_rim?;
    let delta = geom::sub(other.center, rim.center);
    let depth = geom::dot(delta, axis);
    if depth <= tolerance
        || geom::norm(geom::cross(delta, axis)) > tolerance
        || (other.radius - rim.radius).abs() > tolerance
    {
        return None;
    }
    let [a, b] = part.edge_faces()[other.edge][..] else {
        return None;
    };
    let mouth = if a == face {
        b
    } else if b == face {
        a
    } else {
        return None;
    };
    if !matches!(part.faces[mouth].surface, Surface::Plane { .. })
        || geom::dot(normal(part, mouth)?, axis) < 1.0 - 1e-9
        || part.arc(face, mouth) != Some(Arc::Convex)
    {
        return None;
    }
    Some((face, mouth, depth))
}

fn from_floor(part: &Part, floor: usize) -> Option<Occurrence<AnnularGroove>> {
    if !matches!(part.faces[floor].surface, Surface::Plane { .. }) {
        return None;
    }
    let [a, b] = &part.faces[floor].loops[..] else {
        return None;
    };
    let ([(ea, _)], [(eb, _)]) = (&a.edges[..], &b.edges[..]) else {
        return None;
    };
    let axis = normal(part, floor)?;
    let (a, b) = (circle(part, *ea, axis)?, circle(part, *eb, axis)?);
    let (inner, outer) = if a.radius < b.radius { (a, b) } else { (b, a) };
    let tolerance = length_tolerance(&[outer.radius]);
    if outer.radius - inner.radius <= tolerance
        || geom::dist(inner.center, outer.center) > tolerance
    {
        return None;
    }
    let (iw, im, depth) = wall(part, floor, inner, axis, false)?;
    let (ow, om, other_depth) = wall(part, floor, outer, axis, true)?;
    if (depth - other_depth).abs() > length_tolerance(&[depth, other_depth]) {
        return None;
    }
    let mut defining = vec![floor, iw, ow];
    defining.sort_unstable();
    defining.dedup();
    let mut context = vec![im, om];
    context.sort_unstable();
    context.dedup();
    if defining.len() != 3
        || context.iter().any(|f| defining.contains(f))
        || evidence::strict_status(part, &defining, &context) != EvidenceStatus::Verified
    {
        return None;
    }
    Some(Occurrence {
        record: AnnularGroove {
            axis,
            floor_center: outer.center,
            inner_radius: inner.radius,
            outer_radius: outer.radius,
            depth: 0.5 * (depth + other_depth),
        },
        defining,
        context,
    })
}

internal_entry! {
/// Complete annular recesses on one strictly valid solid; competing defining claims are all
/// refused. Floor traversal order is retained, as with the other intrinsic families.
pub fn discover(ctx: &Context<'_>) -> Vec<Occurrence<AnnularGroove>> {
    let found: Vec<_> = (0..ctx.part.faces.len()).filter_map(|f| from_floor(ctx.part, f)).collect();
    let mut owners = BTreeMap::<usize, usize>::new();
    for o in &found { for &face in &o.defining { *owners.entry(face).or_default() += 1; } }
    found.into_iter().filter(|o| o.defining.iter().all(|f| owners[f] == 1)).collect()
}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::brep::{Edge, Face, Loop, Solid};
    use crate::kernel::geom::Frame;
    use crate::kernel::step::IDENTITY;
    use std::f64::consts::TAU;

    /// A turned cylinder with an independently constructed 5..8 mm radial, 4 mm deep
    /// face groove. The core and outer land share the same mouth plane.
    fn seal_housing() -> Part {
        let frame = |z| Frame {
            origin: [0., 0., z],
            x: [1., 0., 0.],
            y: [0., 1., 0.],
            z: [0., 0., 1.],
        };
        let rings = [
            (12., 0.),
            (12., 10.),
            (8., 10.),
            (8., 6.),
            (5., 10.),
            (5., 6.),
        ];
        let mut edges: Vec<_> = rings
            .iter()
            .enumerate()
            .map(|(i, &(radius, z))| {
                let curve = Curve::Circle {
                    frame: frame(z),
                    radius,
                };
                Edge {
                    start: curve.value(0.),
                    end: curve.value(0.),
                    samples: (0..=32)
                        .map(|j| curve.value(TAU * j as f64 / 32.))
                        .collect(),
                    curve,
                    vertices: (i, i),
                    same_sense: true,
                }
            })
            .collect();
        for (bottom, top) in [(0, 1), (3, 2), (5, 4)] {
            let (start, end) = (edges[bottom].start, edges[top].start);
            edges.push(Edge {
                curve: Curve::Line {
                    origin: start,
                    dir: [0., 0., 1.],
                },
                start,
                end,
                vertices: (bottom, top),
                same_sense: true,
                samples: vec![start, end],
            });
        }
        let face = |surface, reversed, boundaries: Vec<Vec<(usize, bool)>>| Face {
            surface,
            reversed,
            solid: Some(0),
            pcurves: Vec::new(),
            loops: boundaries
                .into_iter()
                .map(|edges| Loop {
                    edges,
                    vertex: None,
                })
                .collect(),
        };
        let cylinder = |radius| Surface::Cylinder {
            frame: frame(0.),
            radius,
        };
        let plane = |z| Surface::Plane { frame: frame(z) };
        let faces = vec![
            face(
                cylinder(12.),
                false,
                vec![vec![(0, true), (6, true), (1, false), (6, false)]],
            ),
            face(plane(10.), false, vec![vec![(1, true)], vec![(2, false)]]),
            face(
                cylinder(8.),
                true,
                vec![vec![(3, false), (7, true), (2, true), (7, false)]],
            ),
            face(plane(6.), false, vec![vec![(3, true)], vec![(5, false)]]),
            face(
                cylinder(5.),
                false,
                vec![vec![(5, true), (8, true), (4, false), (8, false)]],
            ),
            face(plane(10.), false, vec![vec![(4, true)]]),
            face(plane(0.), true, vec![vec![(0, false)]]),
        ];
        Part::new(
            faces,
            edges,
            vec![Solid {
                faces: (0..7).collect(),
            }],
        )
    }

    #[test]
    fn face_seal_groove_has_measured_dimensions_and_strict_evidence_after_motion() {
        let base = seal_housing();
        assert!(base.solid_is_geometrically_valid(0));
        let angle: f64 = 0.37;
        let (s, c) = angle.sin_cos();
        for motion in [
            IDENTITY,
            [[c, 0., s, 123.456], [0., 1., 0., -78.9], [-s, 0., c, 41.3]],
        ] {
            let part = base.transformed(&motion).unwrap();
            let found = discover(&Context::new(&part));
            assert_eq!(found.len(), 1);
            let o = &found[0];
            assert_eq!(o.defining, [2, 3, 4]);
            assert_eq!(o.context, [1, 5]);
            assert_eq!(o.record.inner_radius, 5.);
            assert_eq!(o.record.outer_radius, 8.);
            assert!((o.record.depth - 4.).abs() < 1e-9);
            for i in 0..3 {
                assert!((o.record.axis[i] - motion[i][2]).abs() < 1e-9);
                assert!(
                    (o.record.floor_center[i] - (6. * motion[i][2] + motion[i][3])).abs() < 1e-9
                );
            }
            let recognition = crate::correspondence::recognise(&part);
            let doc = crate::recognition::document(&recognition, part.faces.len());
            let records: Vec<_> = doc
                .records
                .iter()
                .filter(|r| r.family == "annular_grooves")
                .collect();
            assert_eq!(records.len(), 1);
            assert_eq!(records[0].faces, [2, 3, 4]);
            assert_eq!(records[0].evidence_status, EvidenceStatus::Verified);
            assert_eq!(doc.schema_version, "quiddity-rust/recognition/6");
            let axis: V3 = serde_json::from_value(records[0].parameters["axis"].clone()).unwrap();
            let center: V3 =
                serde_json::from_value(records[0].parameters["floor_center"].clone()).unwrap();
            assert!(geom::dist(axis, o.record.axis) < 1e-9);
            assert!(geom::dist(center, o.record.floor_center) < 1e-9);
        }
    }

    #[test]
    fn reversed_material_missing_core_and_interrupted_walls_do_not_become_grooves() {
        for fault in 0..4 {
            let mut part = seal_housing();
            match fault {
                0 => part.faces[2].reversed = false,
                1 => part.faces[3].loops.pop().map(|_| ()).unwrap(),
                2 => part.faces[2].loops[0].edges.pop().map(|_| ()).unwrap(),
                _ => part.faces[4].solid = None,
            }
            assert!(discover(&Context::new(&part)).is_empty(), "fault {fault}");
        }
    }
}
