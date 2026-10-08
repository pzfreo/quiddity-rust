//! Countersink recognition (`quiddity.countersinks`): a flaring cone seated on a bore mouth.

use serde::{Deserialize, Serialize};

use super::Context;
use super::evidence::{self, EvidenceError, Occurrence};
use super::policy;
use super::turned::cone_rims;
use crate::kernel::brep::Part;
use crate::kernel::geom::{self, Surface, V3};
use crate::kernel::py;

/// Does this cone sit on that bore? A fraction of the drill radius.
const MINOR_MATCH_FRAC: f64 = 0.0167;
/// How far the opening may sit off the bore's axis, as a fraction of the drill diameter.
const COAXIAL_FRAC: f64 = 0.0167;
/// A near-flat cone is a draft, relief or washer face, not a screw seat.
const MAX_INCLUDED_ANGLE: f64 = 160.0;
/// Too little flare is an edge break, not a seat.
const MIN_MAJOR_RATIO: f64 = 1.5;
const HOLE_DIA_FRAC: f64 = 0.0333;
const HOLE_AXIS_FRAC: f64 = 0.0333;
const HOLE_MOUTH_FRAC: f64 = 0.0833;

/// `axis` points from the wide opening into the part; `location` is the opening (major
/// circle) centre; `included_angle` is the full cone angle in degrees.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CounterSink {
    pub axis: V3,
    pub location: V3,
    pub major_diameter: f64,
    pub drill_diameter: f64,
    pub included_angle: f64,
    pub depth: f64,
}

/// What [`countersink_matches_hole`] reads of a hole.
pub struct HoleMouth {
    pub axis: V3,
    pub location: V3,
    pub diameter: f64,
    pub depth: f64,
    pub through: bool,
}

/// The evidence path: each countersink with its cone face, published only if every one is
/// owned by a valid solid.
pub fn discover_verified(ctx: &Context<'_>) -> Result<Vec<Occurrence<CounterSink>>, EvidenceError> {
    evidence::verified(ctx.part, discover(ctx))
}

/// `recognise_countersinks`.
pub fn recognise_countersinks(part: &Part) -> Vec<CounterSink> {
    discover(&Context::new(part))
        .into_iter()
        .map(|o| o.record)
        .collect()
}

/// Whether the countersink is seated at this bore's mouth: its minor circle sits at a bore end
/// on the bore's axis, with a matching drill diameter.
pub fn countersink_matches_hole(cs: &CounterSink, hole: &HoleMouth) -> bool {
    let minor = [0, 1, 2].map(|i| cs.location[i] + cs.depth * cs.axis[i]);
    let offset = geom::sub(minor, hole.location);
    let axial = py::sum((0..3).map(|i| offset[i] * hole.axis[i]));
    let perpendicular = py::hypot(&[0, 1, 2].map(|i| offset[i] - axial * hole.axis[i]));
    if perpendicular > policy::length_tol(hole.diameter, HOLE_AXIS_FRAC)
        || (cs.drill_diameter - hole.diameter).abs()
            > policy::length_tol(hole.diameter, HOLE_DIA_FRAC)
    {
        return false;
    }
    let mouth = policy::length_tol(hole.diameter, HOLE_MOUTH_FRAC);
    axial.abs() <= mouth || (hole.through && (axial - hole.depth).abs() <= mouth)
}

/// Whether the cone's material-side normal points into its axis: a seat bounds a void, an
/// external shaft transition with the same analytic cone faces away (`_opens_into_void`).
fn opens_into_void(part: &Part, face: usize) -> bool {
    let Surface::Cone { frame, .. } = part.faces[face].surface else {
        return false;
    };
    let Some((u0, u1, v0, v1)) = part.uv_bounds(face) else {
        return false;
    };
    let (u, v) = (0.5 * (u0 + u1), 0.5 * (v0 + v1));
    let sample = part.faces[face].surface.value(u, v);
    let Some(normal) = part.face_normal(face, u, v) else {
        return false;
    };
    let offset = geom::sub(sample, frame.origin);
    let radial = geom::sub(offset, geom::scale(frame.z, geom::dot(offset, frame.z)));
    geom::dot(radial, normal) < 0.0
}

fn dist_to_line(point: V3, line_point: V3, line_dir: V3) -> f64 {
    let v = geom::sub(point, line_point);
    geom::norm(geom::sub(v, geom::scale(line_dir, geom::dot(v, line_dir))))
}

pub fn discover(ctx: &Context<'_>) -> Vec<Occurrence<CounterSink>> {
    let part = ctx.part;
    let cones: Vec<usize> = (0..part.faces.len())
        .filter(|&f| matches!(part.faces[f].surface, Surface::Cone { .. }))
        .collect();
    if cones.is_empty() {
        return Vec::new();
    }
    let cylinders: Vec<(f64, V3, V3)> = part
        .faces
        .iter()
        .filter_map(|f| match f.surface {
            Surface::Cylinder { frame, radius } => Some((radius, frame.origin, frame.z)),
            _ => None,
        })
        .collect();
    let mut out = Vec::new();
    for face in cones {
        let Some(((minor_r, minor_pt), (major_r, opening), included_angle)) = cone_rims(part, face)
        else {
            continue; // a drill-point cone (one circle and an apex)
        };
        if major_r < MIN_MAJOR_RATIO * minor_r || included_angle > MAX_INCLUDED_ANGLE {
            continue;
        }
        if !opens_into_void(part, face) {
            continue;
        }
        let along = geom::sub(minor_pt, opening);
        let length = match geom::norm(along) {
            l if l > 0.0 => l,
            _ => 1.0,
        };
        let axis = along.map(|c| c / length);
        let seated = cylinders.iter().any(|&(r, lp, ld)| {
            (r - minor_r).abs() <= policy::length_tol(minor_r, MINOR_MATCH_FRAC)
                && geom::dot(axis, ld).abs() > 1.0 - 1e-3
                && dist_to_line(opening, lp, ld) <= policy::length_tol(2.0 * minor_r, COAXIAL_FRAC)
        });
        if !seated {
            continue;
        }
        let r4 = |x: f64| py::round_to(x, 4);
        out.push(Occurrence {
            record: CounterSink {
                axis: axis.map(r4),
                location: opening.map(r4),
                major_diameter: r4(2.0 * major_r),
                drill_diameter: r4(2.0 * minor_r),
                included_angle,
                depth: r4(length),
            },
            defining: vec![face],
            context: vec![],
        });
    }
    out.sort_by(|a, b| {
        let key = |o: &Occurrence<CounterSink>| {
            [
                o.record.location[0],
                o.record.location[1],
                o.record.location[2],
                o.record.major_diameter,
            ]
        };
        py::tuple_order(&key(a), &key(b))
    });
    out
}
