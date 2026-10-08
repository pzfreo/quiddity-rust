//! Thin slabs of multi-plate prismatic parts (`quiddity.plates.recognise_plates`): the plate
//! and wall thicknesses of an L-, T- or U-bracket and kin.
//!
//! A plate along an axis is solid material between an outward −axis planar face group at the
//! low coordinate and an outward +axis group at the high one, adjacent along the axis (a pairing
//! that skips an intervening group would span an air gap). Each group must cover a fraction of
//! the body's cross-section, measured as the smallest envelope over the directions the body's
//! own supporting planes establish (so it does not change as the part rolls about the plate
//! normal), and the slab must be thin against the body's extent on that axis.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::Context;
use super::body::BodyKey;
use super::evidence::{self, EvidenceError, Occurrence};
use super::planes::unit_plane_normal;
use super::policy::{self, AXIS_ALIGNED_COS};
use super::turned::axis_letter;
use crate::kernel::brep::Part;
use crate::kernel::geom::{self, Bounds, Surface, V3};
use crate::kernel::py;

/// The minimum slab thickness and coordinate-clustering tolerance: a minimum-evidence
/// threshold, absolute by design (ADR 0008).
const TOL: f64 = 0.5;
/// Directions agreeing to nine decimal places of a degree are one direction.
const ORIENTED_ANGLE_DIGITS: usize = 9;

/// A recognised thin slab. `axis` is the thickness axis, `lo`/`hi` its bounding coordinates
/// along it, and `u`/`v` the area-weighted centre of its two face groups on the other two axes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Plate {
    pub axis: char,
    pub lo: f64,
    pub hi: f64,
    pub u: f64,
    pub v: f64,
    pub body_key: Option<BodyKey>,
}

impl Plate {
    pub fn thickness(&self) -> f64 {
        self.hi - self.lo
    }
}

/// Options named as `recognise_plates`' keyword arguments.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PlateOptions {
    /// Each face group covers at least this fraction of the body's cross-section.
    pub min_area_frac: f64,
    /// The slab is thinner than this fraction of the body's extent on its axis.
    pub max_thick_frac: f64,
    /// Minimum thickness and clustering tolerance; 0.5 mm by default.
    pub tol: Option<f64>,
}

impl Default for PlateOptions {
    fn default() -> Self {
        PlateOptions {
            min_area_frac: 0.4,
            max_thick_frac: 0.5,
            tol: None,
        }
    }
}

/// `recognise_plates`: deduplicated by (axis, lo, hi) within each body, sorted by axis, `lo`,
/// `hi`, `u` and `v`.
pub fn recognise_plates(part: &Part, opts: &PlateOptions) -> Vec<Plate> {
    super::records(discover(&Context::new(part), opts))
}

/// The evidence path: each plate's low and high face groups, all on one valid solid, with no
/// face defining two plates and no plate key bound to two different face groups.
pub fn discover_verified(
    ctx: &Context<'_>,
    opts: &PlateOptions,
) -> Result<Vec<Occurrence<Plate>>, EvidenceError> {
    let found = evidence::verified(ctx.part, discover(ctx, opts))?;
    let mut used: Vec<usize> = Vec::new();
    for o in &found {
        if o.defining.iter().any(|f| used.contains(f)) {
            return Err(EvidenceError::SharedEvidence);
        }
        used.extend(&o.defining);
    }
    Ok(found)
}

/// One planar face on one side: its plane coordinate, area, area-weighted centre on the two
/// other axes, and the face.
type Entry = (f64, f64, f64, f64, usize);

/// One side's faces at one coordinate cluster: total area, area-weighted centre sums on the
/// two other axes, and the faces.
struct Group {
    area: f64,
    u_sum: f64,
    v_sum: f64,
    faces: Vec<usize>,
}

/// One body: a solid, or the whole part when it has none.
struct Scope {
    faces: Vec<usize>,
    bounds: Bounds,
    key: Option<BodyKey>,
}

/// Every plate with its low and high face groups as defining faces, in the order
/// `recognise_plates` returns them.
pub fn discover(ctx: &Context<'_>, opts: &PlateOptions) -> Vec<Occurrence<Plate>> {
    let part = ctx.part;
    let tol = opts.tol.unwrap_or(TOL);
    let scopes: Vec<Scope> = if part.solids.is_empty() {
        vec![Scope {
            faces: (0..part.faces.len()).collect(),
            bounds: *ctx.bounds(),
            key: None,
        }]
    } else {
        let keys = ctx.body_keys(true);
        part.solids
            .iter()
            .enumerate()
            .map(|(s, solid)| Scope {
                faces: solid.faces.clone(),
                bounds: part.solid_bounds(s),
                key: keys[s].clone(),
            })
            .collect()
    };
    let mut out: Vec<Occurrence<Plate>> = Vec::new();
    for scope in &scopes {
        let mut seen: Vec<(char, f64, f64)> = Vec::new();
        for mut o in proposals(part, scope, opts, tol) {
            let key = (o.record.axis, o.record.lo, o.record.hi);
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            o.record.body_key = scope.key.clone();
            out.push(o);
        }
    }
    out.sort_by(|a, b| {
        let (a, b) = (&a.record, &b.record);
        a.axis
            .cmp(&b.axis)
            .then(py::order(a.lo, b.lo))
            .then(py::order(a.hi, b.hi))
            .then(py::order(a.u, b.u))
            .then(py::order(a.v, b.v))
    });
    out
}

/// `_plate_proposals`: one body's plates, sorted by axis, `lo` and `hi`.
fn proposals(part: &Part, scope: &Scope, opts: &PlateOptions, tol: f64) -> Vec<Occurrence<Plate>> {
    let b = &scope.bounds;
    let extents: V3 = std::array::from_fn(|i| b.max[i] - b.min[i]);
    let faces: Vec<usize> = scope
        .faces
        .iter()
        .copied()
        .filter(|&f| matches!(part.faces[f].surface, Surface::Plane { .. }))
        .collect();
    let mut out = Vec::new();
    for i in 0..3 {
        let oi: Vec<usize> = (0..3).filter(|&j| j != i).collect();
        // (location, area, u moment, v moment, face) per side: outward −axis, then +axis.
        let mut sides: [Vec<Entry>; 2] = [Vec::new(), Vec::new()];
        for &face in &faces {
            let Some(normal) = unit_plane_normal(part, face) else {
                continue;
            };
            let component = normal[i];
            if component.abs() < AXIS_ALIGNED_COS {
                continue;
            }
            let (Some([area, _]), Some(centre)) = (part.face_mass(face), part.face_centre(face))
            else {
                continue;
            };
            let Surface::Plane { frame } = part.faces[face].surface else {
                unreachable!("planar faces only");
            };
            sides[usize::from(component > 0.0)].push((
                frame.origin[i],
                area,
                centre[oi[0]] * area,
                centre[oi[1]] * area,
                face,
            ));
        }
        let [negative, positive] = sides.map(|side| group(&side, tol));
        let maximum_thickness = opts.max_thick_frac * extents[i];
        // No ordered thin opposed span: no area threshold could publish a plate here.
        if !negative.keys().any(|&low| {
            positive.keys().any(|&high| {
                tol < high.0 - low.0 && policy::clears_threshold(maximum_thickness, high.0 - low.0)
            })
        }) {
            continue;
        }
        let cross = oriented_cross_area(part, scope, &faces, i, extents);
        if cross <= 0.0 {
            continue;
        }
        let threshold = opts.min_area_frac * cross;
        let mut events: Vec<(f64, i8, &Group)> = negative
            .iter()
            .filter(|(_, g)| policy::clears_threshold(g.area, threshold))
            .map(|(c, g)| (c.0, -1, g))
            .chain(
                positive
                    .iter()
                    .filter(|(_, g)| policy::clears_threshold(g.area, threshold))
                    .map(|(c, g)| (c.0, 1, g)),
            )
            .collect();
        events.sort_by(|a, b| py::order(a.0, b.0).then(a.1.cmp(&b.1)));
        for w in events.windows(2) {
            let ((low, low_sign, low_group), (high, high_sign, high_group)) = (w[0], w[1]);
            if low_sign != -1 || high_sign != 1 {
                continue;
            }
            let thickness = high - low;
            if thickness <= tol || !policy::clears_threshold(maximum_thickness, thickness) {
                continue;
            }
            let combined_area = low_group.area + high_group.area;
            out.push(Occurrence {
                record: Plate {
                    axis: axis_letter(i),
                    lo: py::round_to(low, 3),
                    hi: py::round_to(high, 3),
                    u: (low_group.u_sum + high_group.u_sum) / combined_area,
                    v: (low_group.v_sum + high_group.v_sum) / combined_area,
                    body_key: None,
                },
                defining: low_group
                    .faces
                    .iter()
                    .chain(&high_group.faces)
                    .copied()
                    .collect(),
                context: vec![],
            });
        }
    }
    out.sort_by(|a, b| {
        let (a, b) = (&a.record, &b.record);
        a.axis
            .cmp(&b.axis)
            .then(py::order(a.lo, b.lo))
            .then(py::order(a.hi, b.hi))
    });
    out
}

/// A coordinate usable as an ordered map key (never NaN here).
#[derive(Clone, Copy, PartialEq)]
struct Coord(f64);
impl Eq for Coord {}
impl PartialOrd for Coord {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Coord {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        py::order(self.0, other.0)
    }
}

/// One side's faces clustered by plane coordinate (`cluster_coordinates`), keyed by each
/// cluster's lowest coordinate, with sums in cluster order.
fn group(side: &[Entry], tol: f64) -> BTreeMap<Coord, Group> {
    let locations: Vec<f64> = side.iter().map(|e| e.0).collect();
    let mut groups = BTreeMap::new();
    for cluster in policy::cluster_coordinates(&locations, tol) {
        let low = cluster
            .iter()
            .map(|&k| side[k].0)
            .fold(f64::INFINITY, f64::min);
        let sum = |f: fn(&Entry) -> f64| cluster.iter().fold(0.0, |acc, &k| acc + f(&side[k]));
        groups.insert(
            Coord(low),
            Group {
                area: sum(|e| e.1),
                u_sum: sum(|e| e.2),
                v_sum: sum(|e| e.3),
                faces: cluster.iter().map(|&k| side[k].4).collect(),
            },
        );
    }
    groups
}

/// `_oriented_cross_area`: the smallest cross-envelope of the body across axis *i*, over the
/// directions its supporting transverse planes establish; the coordinate envelope when there
/// are none.
fn oriented_cross_area(part: &Part, scope: &Scope, faces: &[usize], i: usize, extents: V3) -> f64 {
    let other: [usize; 2] = match i {
        0 => [1, 2],
        1 => [0, 2],
        _ => [0, 1],
    };
    let vertices: Vec<V3> = scope
        .faces
        .iter()
        .flat_map(|&f| &part.faces[f].loops)
        .flat_map(|lp| {
            lp.vertex.into_iter().chain(
                lp.edges
                    .iter()
                    .flat_map(|&(e, _)| [part.edges[e].start, part.edges[e].end]),
            )
        })
        .collect();
    let support_eps = extents.iter().copied().fold(1.0, f64::max) * 1e-9;
    let project = |p: V3, n: V3| p[0] * n[0] + p[1] * n[1] + p[2] * n[2];
    let mut extreme_projections: Vec<(V3, f64)> = Vec::new();
    let mut angles: Vec<f64> = Vec::new();
    for &face in faces {
        let Some(normal) = unit_plane_normal(part, face) else {
            continue;
        };
        if normal[i].abs() > 1.0 - AXIS_ALIGNED_COS {
            continue;
        }
        if normal[other[0]].hypot(normal[other[1]]) < AXIS_ALIGNED_COS {
            continue;
        }
        let Surface::Plane { frame } = part.faces[face].surface else {
            continue;
        };
        let plane_projection = project(frame.origin, normal);
        if !vertices.is_empty() {
            let extreme = match extreme_projections.iter().find(|(n, _)| *n == normal) {
                Some(&(_, e)) => e,
                None => {
                    let e = vertices
                        .iter()
                        .map(|&v| project(v, normal))
                        .fold(f64::NEG_INFINITY, f64::max);
                    extreme_projections.push((normal, e));
                    e
                }
            };
            if extreme > plane_projection + support_eps {
                continue; // a concave or internal wall establishes no envelope direction
            }
        }
        let angle = py::modulo(normal[other[1]].atan2(normal[other[0]]).to_degrees(), 90.0);
        let angle = py::modulo(py::round_to(angle, ORIENTED_ANGLE_DIGITS), 90.0);
        if !angles.contains(&angle) {
            angles.push(angle);
        }
    }
    if angles.is_empty() {
        return extents[other[0]] * extents[other[1]];
    }
    // `part.rotate(axis, sign·angle)`: the turned body's extent along a coordinate axis is the
    // body's extent along that axis turned back.
    let sign = if i == 1 { 1.0 } else { -1.0 };
    let mut axis = [0.0; 3];
    axis[i] = 1.0;
    angles
        .iter()
        .map(|&angle| {
            if angle == 0.0 {
                return extents[other[0]] * extents[other[1]];
            }
            let (s, c) = (-sign * angle).to_radians().sin_cos();
            let size = |j: usize| {
                let mut e = [0.0; 3];
                e[j] = 1.0;
                let d = geom::add(geom::scale(e, c), geom::scale(geom::cross(axis, e), s));
                let (lo, hi) = part.extent_along(&scope.faces, d);
                hi - lo
            };
            size(other[0]) * size(other[1])
        })
        .fold(f64::INFINITY, f64::min)
}
