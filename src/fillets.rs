//! External edge fillet recognition (`quiddity.fillets`).
//!
//! A prismatic fillet is a partial cylinder rounding a convex 90° edge between two axis-aligned
//! walls; a turned fillet is a short external torus meeting a coaxial external cylinder.

use std::collections::HashMap;
use std::f64::consts::PI;

use serde::Serialize;

use crate::adjacency::{Adjacency, dominant_axis, nearest_axis_aligned_planes};
use crate::brep::Part;
use crate::classify::{Classifier, material_at};
use crate::cylinders::{CylinderEvidence, analyse_cylinders, coaxial_axis_lines};
use crate::geom::{self, AXIS_ALIGNED_COS, INTERIOR_PROBE_FRAC, Surface, SurfaceType, V3};

/// A minimum-evidence threshold, deliberately absolute (ADR 0008).
const MIN_RADIUS: f64 = 0.6;
const FILLET_MAX_EXTENT: f64 = PI * 1.05;
const TURNED_FILLET_MAX_EXTENT: f64 = PI / 2.0 * 1.05;
const COAXIAL_FRAC: f64 = 1e-4;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Fillet {
    pub axis: char,
    pub radius: f64,
    pub at: V3,
    pub turned: bool,
    pub side: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct FilletOptions {
    pub min_radius: Option<f64>,
    pub max_radius_frac: f64,
    /// `false` for a part already classified as rotational: only toroidal fillets are reported.
    pub include_cylindrical: bool,
}

impl Default for FilletOptions {
    fn default() -> Self {
        FilletOptions {
            min_radius: None,
            max_radius_frac: 0.45,
            include_cylindrical: true,
        }
    }
}

/// A fillet with the face that defines it and the faces consulted to prove it.
#[derive(Clone, Debug, PartialEq)]
pub struct FilletOccurrence {
    pub record: Fillet,
    pub face: usize,
    pub context: Vec<usize>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum EvidenceError {
    NoValidSolid,
}

impl std::fmt::Display for EvidenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "fillet defining face has no unambiguous valid solid")
    }
}

/// `recognise_fillets`: the records only.
pub fn recognise_fillets(part: &Part, opts: &FilletOptions) -> Vec<Fillet> {
    discover(part, opts).into_iter().map(|o| o.record).collect()
}

/// The evidence path: every occurrence's defining face and context must lie in one valid solid,
/// or nothing is returned.
pub fn discover_fillets(
    part: &Part,
    opts: &FilletOptions,
) -> Result<Vec<FilletOccurrence>, EvidenceError> {
    let found = discover(part, opts);
    for o in &found {
        let owner = part.faces[o.face].solid;
        let valid = owner.is_some_and(|s| part.solid_is_valid(s))
            && o.context.iter().all(|&c| part.faces[c].solid == owner);
        if !valid {
            return Err(EvidenceError::NoValidSolid);
        }
    }
    Ok(found)
}

/// The leader-tip anchor: the surface point at the middle of the face's trimmed UV range.
pub fn fillet_anchor(part: &Part, face: usize) -> Option<V3> {
    let (u0, u1, v0, v1) = part.uv_bounds(face)?;
    Some(
        part.faces[face]
            .surface
            .value(0.5 * (u0 + u1), 0.5 * (v0 + v1)),
    )
}

fn record(axis: usize, radius: f64, at: V3, turned: bool) -> Fillet {
    Fillet {
        axis: ['x', 'y', 'z'][axis],
        radius: geom::round3(radius),
        at: at.map(geom::round3),
        turned,
        side: "convex",
    }
}

/// The point just off the virtual sharp corner, toward the blend (`_bevel._near_corner`).
pub fn near_corner(
    centre: V3,
    edge_i: usize,
    neigh: &std::collections::BTreeMap<usize, f64>,
) -> V3 {
    let mut corner = [0.0; 3];
    corner[edge_i] = centre[edge_i];
    for (&axis, &coord) in neigh {
        corner[axis] = coord;
    }
    let step = INTERIOR_PROBE_FRAC;
    [0, 1, 2].map(|i| corner[i] + step * (centre[i] - corner[i]))
}

fn discover(part: &Part, opts: &FilletOptions) -> Vec<FilletOccurrence> {
    let bb = part.bounds();
    let min_radius = opts.min_radius.unwrap_or(MIN_RADIUS);
    let max_radius = opts.max_radius_frac * bb.max_extent();
    let adjacency = Adjacency::new(part);
    let mut classifier: Option<Classifier<'_>> = None;
    let mut out = Vec::new();

    for face in 0..part.faces.len() {
        if !opts.include_cylindrical {
            break;
        }
        let Surface::Cylinder { frame, radius } = part.faces[face].surface else {
            continue;
        };
        let Some((u0, u1, _, _)) = part.uv_bounds(face) else {
            continue;
        };
        if u1 - u0 >= FILLET_MAX_EXTENT || radius < min_radius || radius > max_radius {
            continue;
        }
        let comp = frame.z.map(f64::abs);
        if comp[0].max(comp[1]).max(comp[2]) <= AXIS_ALIGNED_COS {
            continue;
        }
        let edge_i = dominant_axis(comp);
        let centre = part.face_bounds(face).centre();
        let neigh = nearest_axis_aligned_planes(part, &adjacency, face, centre, edge_i, true);
        if (0..3).any(|j| j != edge_i && !neigh.contains_key(&j)) {
            continue;
        }
        let classifier = classifier.get_or_insert_with(|| Classifier::new(part));
        if material_at(classifier, near_corner(centre, edge_i, &neigh)) {
            continue;
        }
        let Some(at) = fillet_anchor(part, face) else {
            continue;
        };
        out.push(FilletOccurrence {
            record: record(edge_i, radius, at, false),
            face,
            context: vec![],
        });
    }

    let tori: Vec<usize> = (0..part.faces.len())
        .filter(|&f| part.faces[f].surface.kind() == SurfaceType::Torus)
        .collect();
    if !tori.is_empty() {
        let external: HashMap<usize, CylinderEvidence> = analyse_cylinders(part)
            .into_iter()
            .filter(|c| c.external)
            .map(|c| (c.face, c))
            .collect();
        for face in tori {
            let Surface::Torus { frame, minor, .. } = part.faces[face].surface else {
                unreachable!()
            };
            if minor < min_radius || minor > max_radius {
                continue;
            }
            let Some((_, _, v0, v1)) = part.uv_bounds(face) else {
                continue;
            };
            if (v1 - v0).abs() > TURNED_FILLET_MAX_EXTENT {
                continue;
            }
            let comp = frame.z.map(f64::abs);
            if comp[0].max(comp[1]).max(comp[2]) <= AXIS_ALIGNED_COS {
                continue;
            }
            let edge_i = dominant_axis(comp);
            let neighbours = adjacency.neighbours(part, face);
            let Some(context) = turned_context(part, face, &neighbours, &external) else {
                continue;
            };
            let Some(at) = fillet_anchor(part, face) else {
                continue;
            };
            out.push(FilletOccurrence {
                record: record(edge_i, minor, at, true),
                face,
                context,
            });
        }
    }
    out.sort_by(|a, b| {
        (a.record.axis, a.record.at)
            .partial_cmp(&(b.record.axis, b.record.at))
            .expect("finite anchors")
    });
    out
}

/// The material-side proof for a toroidal blend: the coaxial external cylinders it meets, plus
/// the transverse planes or spherical caps that make it a rounded edge rather than a bead.
/// `None` when the torus is not a turned edge fillet.
pub fn turned_context(
    part: &Part,
    torus: usize,
    neighbours: &[usize],
    external: &HashMap<usize, CylinderEvidence>,
) -> Option<Vec<usize>> {
    let Surface::Torus { frame, .. } = part.faces[torus].surface else {
        return None;
    };
    let coaxial: Vec<&CylinderEvidence> = neighbours
        .iter()
        .filter_map(|n| external.get(n))
        .filter(|c| {
            coaxial_axis_lines(
                frame.origin,
                frame.z,
                c.axis_point,
                c.direction,
                geom::length_tol(c.diameter, COAXIAL_FRAC),
            )
        })
        .collect();
    if coaxial.is_empty() {
        return None;
    }
    let mut transverse = Vec::new();
    let mut continuations = Vec::new();
    for &n in neighbours {
        match &part.faces[n].surface {
            Surface::Sphere { .. } => continuations.push(n),
            Surface::Plane { frame: plane }
                if geom::dot(plane.z, frame.z).abs() > AXIS_ALIGNED_COS =>
            {
                transverse.push(n)
            }
            _ => {}
        }
    }
    let mut diameters: Vec<f64> = coaxial.iter().map(|c| c.diameter).collect();
    diameters.sort_by(f64::total_cmp);
    diameters.dedup();
    let bridges_two_bands = diameters.len() >= 2;
    if transverse.is_empty() && !bridges_two_bands && continuations.is_empty() {
        return None;
    }
    let mut context: Vec<usize> = Vec::new();
    for f in coaxial
        .iter()
        .map(|c| c.face)
        .chain(transverse)
        .chain(continuations)
    {
        if !context.contains(&f) {
            context.push(f);
        }
    }
    Some(context)
}
