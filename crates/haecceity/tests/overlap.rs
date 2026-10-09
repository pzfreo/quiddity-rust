//! The common area of two coplanar planar faces ([`haecceity::overlap`], upstream need U5 of
//! specify-core-rust), on `tests/fixtures/stack.step` (specify-core-rust's
//! `tests/fixtures/parts/stack.step`, 94a70f9: a base, a cover on it and a pin through the
//! cover, each placed in the assembly), against areas computed by hand.

mod common;

use std::f64::consts::PI;

use haecceity::geom::Surface;
use haecceity::overlap::{LOOP_GAP, common_area, common_area_placed};
use haecceity::step::{Placement, StepFile};
use haecceity::{Part, geom};

struct Placed {
    name: String,
    part: Part,
    placement: Placement,
}

fn stack() -> Vec<Placed> {
    let bytes = std::fs::read(common::fixtures().join("stack.step")).unwrap();
    let file = StepFile::read(&bytes).unwrap();
    file.parts
        .iter()
        .enumerate()
        .map(|(i, def)| {
            assert_eq!(def.placements.len(), 1, "{}", def.name);
            Placed {
                name: def.name.clone(),
                part: file.part(i).unwrap(),
                placement: def.placements[0].placement,
            }
        })
        .collect()
}

fn place(placement: &Placement, p: [f64; 3], w: f64) -> [f64; 3] {
    placement.map(|row| row[0] * p[0] + row[1] * p[1] + row[2] * p[2] + w * row[3])
}

/// Every pair of planar faces of two placed parts that face each other in one plane (within
/// 1e-6 mm) and overlap: (face of `a`, face of `b`, common area).
fn contacts(a: &Placed, b: &Placed) -> Vec<(usize, usize, f64)> {
    let planes = |p: &Placed| -> Vec<(usize, [f64; 3], [f64; 3])> {
        (0..p.part.faces.len())
            .filter_map(|f| match p.part.faces[f].surface {
                Surface::Plane { frame } => {
                    let sense = if p.part.faces[f].reversed { -1.0 } else { 1.0 };
                    Some((
                        f,
                        place(&p.placement, frame.origin, 1.0),
                        geom::scale(place(&p.placement, frame.z, 0.0), sense),
                    ))
                }
                _ => None,
            })
            .collect()
    };
    let mut out = Vec::new();
    for (fa, oa, na) in planes(a) {
        for (fb, ob, nb) in planes(b) {
            if geom::dot(na, nb) > -1.0 + 1e-9 || geom::dot(geom::sub(ob, oa), na).abs() > 1e-6 {
                continue;
            }
            let area = common_area_placed((&a.part, fa, &a.placement), (&b.part, fb, &b.placement))
                .unwrap();
            if area > 1e-9 {
                out.push((fa, fb, area));
            }
        }
    }
    out
}

/// The base's top and the cover's underside are each 60 x 40 with four holes; their common
/// face keeps the larger of each pair (the cover's Ø5.5 clearance holes over the Ø4.2 tap
/// drills, and the two Ø8 bores): 2400 − 4·π·2.75² − π·4² = 2254.701. The pin's Ø10 head on
/// the cover's top, round its Ø8 hole: π·(5² − 4²) = 28.274. Both to 1e-8 relative (the
/// circles' chords lose about 4e-9 of a circle).
#[test]
fn the_stacks_common_areas_are_the_exact_areas() {
    let parts = stack();
    let by_name = |name: &str| parts.iter().find(|p| p.name == name).unwrap();
    let (base, cover, pin) = (by_name("base"), by_name("cover"), by_name("pin"));
    let plate = 2400.0 - 4.0 * PI * 2.75f64.powi(2) - PI * 16.0;
    let annulus = PI * (25.0 - 16.0);
    assert_eq!(format!("{plate:.3} {annulus:.3}"), "2254.701 28.274");
    let close = |got: f64, want: f64| (got - want).abs() <= 1e-8 * want;
    let found = contacts(base, cover);
    assert!(
        found.len() == 1 && close(found[0].2, plate),
        "{found:?} against {plate}"
    );
    // The same pair the other way round.
    let (fb, fc, _) = found[0];
    let back = common_area_placed(
        (&cover.part, fc, &cover.placement),
        (&base.part, fb, &base.placement),
    )
    .unwrap();
    assert!(close(back, plate), "{back}");
    let found = contacts(cover, pin);
    assert!(
        found.len() == 1 && close(found[0].2, annulus),
        "{found:?} against {annulus}"
    );
    // The base and the pin do not touch.
    assert!(contacts(base, pin).is_empty());
    // A face against itself: its own area.
    let own = common_area((&cover.part, fc), (&cover.part, fc)).unwrap();
    let area = cover.part.face_mass(fc).unwrap()[0];
    assert!((own - area).abs() <= 1e-8 * area, "{own} vs {area}");
}

/// A face that is not planar, two planes that are not parallel, and a loop that does not close
/// are refused, naming the face.
#[test]
fn what_cannot_be_measured_is_refused() {
    let parts = stack();
    let cover = parts.iter().find(|p| p.name == "cover").unwrap();
    let part = &cover.part;
    let kind = |f: usize| part.faces[f].surface.kind();
    let curved = (0..part.faces.len())
        .find(|&f| kind(f) == geom::SurfaceType::Cylinder)
        .unwrap();
    let flat = (0..part.faces.len())
        .find(|&f| kind(f) == geom::SurfaceType::Plane)
        .unwrap();
    let why = common_area((part, flat), (part, curved)).unwrap_err();
    assert_eq!(why.reason, format!("face {curved} is not planar"));
    let normal = |f: usize| match part.faces[f].surface {
        Surface::Plane { frame } => frame.z,
        _ => unreachable!(),
    };
    let side = (0..part.faces.len())
        .find(|&f| {
            kind(f) == geom::SurfaceType::Plane
                && geom::norm(geom::cross(normal(f), normal(flat))) > 0.5
        })
        .unwrap();
    let why = common_area((part, flat), (part, side)).unwrap_err();
    assert_eq!(
        why.reason,
        format!("faces {flat} and {side} are not parallel")
    );
    // The flat face's first edge moved 0.1 mm (more than LOOP_GAP) off where its loop runs.
    let (e, _) = part.faces[flat].loops[0].edges[0];
    let mut edges = part.edges.clone();
    let off = [0.0, 0.0, 10.0 * LOOP_GAP];
    edges[e].start = geom::add(edges[e].start, off);
    edges[e].end = geom::add(edges[e].end, off);
    for p in &mut edges[e].samples {
        *p = geom::add(*p, off);
    }
    if let haecceity::geom::Curve::Line { origin, .. } = &mut edges[e].curve {
        *origin = geom::add(*origin, off);
    }
    let broken = Part::new(part.faces.clone(), edges, part.solids.clone());
    let why = common_area((&broken, flat), (part, flat)).unwrap_err();
    assert!(
        why.reason
            .starts_with(&format!("loop 0 of face {flat} does not close")),
        "{why}"
    );
}
