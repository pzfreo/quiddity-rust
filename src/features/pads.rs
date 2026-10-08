//! Bounded principal-axis rectangular raised pads (`quiddity.pads`).
//!
//! A pad is a planar principal-axis top whose area fills its transverse rectangle, bounded on
//! both transverse axes (a full-span step is not a pad), standing on its own four perimeter walls
//! whose highest base is its local support, and whose top a certified material side shows facing
//! out of the solid along the pad's direction. Each solid is read along z, x and y, both ways. A
//! top whose rectangle is rounded at its four corners by convex single-face cylindrical blend
//! chains between its walls is the blended path: the walls are the top's neighbours, and the area
//! test allows for the four quarter-circle removals. Where one island reads as pads along several
//! axes, the unique shallowest reading owns it; a tie is refused.
//!
//! Python's `ValueError`s (a selected blend cycle whose bridges are not unique, and the evidence
//! path's refusals) are [`PadError`]s with its messages. A part without solids has no pads: every
//! proposal needs its top's material side, which only a valid solid certifies.
//! Python reads such a part as one source (`solids or [part]`), so on the blended path it could
//! still raise a collapse or bridge error before that check, where this returns nothing; no captured
//! call or corpus part has a solidless shape to show it.

use std::cmp::Ordering;
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::Context;
use super::effective_surfaces::EffectiveFaces;
use super::evidence::{Occurrence, common_valid_solid};
use super::experimental_geometry::{GeometryGraph, SurfaceKind};
use super::graph;
use super::policy::{AXIS_ALIGNED_COS, AXIS_ZERO_COS};
use super::solid_properties;
use crate::kernel::brep::Part;
use crate::kernel::geom::{Bounds, V3};
use crate::kernel::py;

/// `_TOL`: a minimum-evidence threshold, deliberately absolute (Python's ADR 0008); also the
/// pad-footprint minimum on both in-plane axes.
const TOL: f64 = 0.2;

/// The axes in the order they are searched, each with its two transverse axes
/// (`_TRANSVERSE_AXES`), by index.
const AXES: [(usize, [usize; 2]); 3] = [(2, [0, 1]), (0, [1, 2]), (1, [0, 2])];
const AXIS_NAMES: [&str; 3] = ["x", "y", "z"];

/// `RaisedPad`: a bounded rectangular island, its box rounded to 3 decimals, the principal
/// `axis` it rises along (`"x"`, `"y"` or `"z"`) and its `direction` (1 or -1) along it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RaisedPad {
    pub x0: f64,
    pub x1: f64,
    pub y0: f64,
    pub y1: f64,
    pub z0: f64,
    pub z1: f64,
    pub axis: String,
    pub direction: i32,
}

impl RaisedPad {
    /// `_record_bounds`.
    fn bounds(&self) -> [(f64, f64); 3] {
        [(self.x0, self.x1), (self.y0, self.y1), (self.z0, self.z1)]
    }

    fn axis_index(&self) -> usize {
        AXIS_NAMES
            .iter()
            .position(|&n| n == self.axis)
            .expect("a pad's axis is x, y or z")
    }

    /// `_axial_extent`: the attachment-to-terminal span.
    fn axial_extent(&self) -> f64 {
        let (lo, hi) = self.bounds()[self.axis_index()];
        hi - lo
    }
}

/// The dataclass order: field by field.
fn record_order(a: &RaisedPad, b: &RaisedPad) -> Ordering {
    py::tuple_order(
        &[a.x0, a.x1, a.y0, a.y1, a.z0, a.z1],
        &[b.x0, b.x1, b.y0, b.y1, b.z0, b.z1],
    )
    .then_with(|| a.axis.cmp(&b.axis))
    .then_with(|| a.direction.cmp(&b.direction))
}

/// Options named as `recognise_rectangular_pads`' keyword arguments.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PadOptions {
    /// The span and fit threshold; 0.2 mm when `None`.
    pub tol: Option<f64>,
}

/// Where Python raises `ValueError`, with its message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadError {
    /// The blend collapse refused the selection (its own message).
    Collapse(&'static str),
    NoUniqueBridges,
    NoUniqueBridge,
    LostProvenance,
    AmbiguousRole,
    NotFiveFaces,
    AmbiguousOccurrences,
    SharedTop,
    NoValidSolid,
    ProvenanceUnavailable,
}

impl std::fmt::Display for PadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            PadError::Collapse(message) => message,
            PadError::NoUniqueBridges => "selected Pad blend cycle has no unique logical bridges",
            PadError::NoUniqueBridge => "selected Pad blend chain has no unique logical bridge",
            PadError::LostProvenance => "selected Pad blend bridge lost original provenance",
            PadError::AmbiguousRole => "a Pad wall role has ambiguous maximal-base faces",
            PadError::NotFiveFaces => "a Pad requires five pairwise-distinct defining faces",
            PadError::AmbiguousOccurrences => {
                "equal Pad values have ambiguous defining occurrences"
            }
            PadError::SharedTop => "Pad occurrences share a defining top face",
            PadError::NoValidSolid => "Pad defining faces do not belong to one valid solid",
            PadError::ProvenanceUnavailable => {
                "Pad surface provenance became unavailable before issuance"
            }
        })
    }
}

impl std::error::Error for PadError {}

/// `_PadProposal`: the record, its top, and each perimeter role's faces.
#[derive(Clone, Debug)]
struct Proposal {
    record: RaisedPad,
    top: usize,
    walls: [Vec<usize>; 4],
}

impl Proposal {
    /// `_proposal_faces`: the top and every wall role's faces.
    fn faces(&self) -> BTreeSet<usize> {
        std::iter::once(self.top)
            .chain(self.walls.iter().flatten().copied())
            .collect()
    }
}

/// `_PlanarFace`: a face whose effective fact is a principal-axis plane, its box from its
/// vertices, its canonical normal and its area (`None` where the face's mass cannot be
/// computed: such a face can still be a wall, but never passes an area test).
struct Planar {
    face: usize,
    bounds: [(f64, f64); 3],
    normal: V3,
    area: Option<f64>,
}

/// `max(a, b)` as Python computes it: the first unless the second is greater, so a NaN first
/// argument stays (Rust's `f64::max` would drop it).
fn py_max(a: f64, b: f64) -> f64 {
    if b > a { b } else { a }
}

fn span(b: &Bounds, axis: usize) -> (f64, f64) {
    (b.min[axis], b.max[axis])
}

/// `_face_vertex_bounds`: the exact coordinate spans of the face's boundary vertices.
fn vertex_bounds(part: &Part, face: usize) -> [(f64, f64); 3] {
    let vertices = graph::face_vertices(part, face);
    [0, 1, 2].map(|i| {
        vertices
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
                (lo.min(v[i]), hi.max(v[i]))
            })
    })
}

/// `_planar_faces`: the solid's faces whose effective fact is a plane within `AXIS_ALIGNED_COS`
/// of a principal axis, in face order.
fn planar_faces(part: &Part, faces: &[usize], effective: &EffectiveFaces<'_, '_>) -> Vec<Planar> {
    faces
        .iter()
        .filter_map(|&face| {
            let fact = effective.fact(face).as_ref().ok()?;
            if fact.kind() != SurfaceKind::Plane {
                return None;
            }
            let p = fact.parameters();
            let normal = [p[0], p[1], p[2]];
            if normal.iter().fold(0.0f64, |m, c| m.max(c.abs())) < AXIS_ALIGNED_COS {
                return None;
            }
            Some(Planar {
                face,
                bounds: vertex_bounds(part, face),
                normal,
                area: part.face_mass(face).map(|m| m[0]),
            })
        })
        .collect()
}

/// `_pad_record`.
#[allow(clippy::too_many_arguments)]
fn pad_record(
    axis: usize,
    axis_sign: i32,
    [u_axis, v_axis]: [usize; 2],
    (u0, u1): (f64, f64),
    (v0, v1): (f64, f64),
    base: f64,
    top: f64,
) -> RaisedPad {
    let mut bounds = [(0.0, 0.0); 3];
    bounds[axis] = if py::order(top, base) == Ordering::Less {
        (top, base)
    } else {
        (base, top)
    };
    bounds[u_axis] = (u0, u1);
    bounds[v_axis] = (v0, v1);
    let r = |v: f64| py::round_to(v, 3);
    RaisedPad {
        x0: r(bounds[0].0),
        x1: r(bounds[0].1),
        y0: r(bounds[1].0),
        y1: r(bounds[1].1),
        z0: r(bounds[2].0),
        z1: r(bounds[2].1),
        axis: AXIS_NAMES[axis].to_owned(),
        direction: axis_sign,
    }
}

/// `_wall_role`: the faces at the role's maximal base among the walls normal to *axis* at
/// *pos*, reaching the top and spanning `lo..hi` across, with that base.
#[allow(clippy::too_many_arguments)]
fn wall_role(
    walls: &[&Planar],
    axis: usize,
    pos: f64,
    (lo, hi): (f64, f64),
    top: f64,
    tol: f64,
    height_axis: usize,
    axis_sign: i32,
) -> Option<(f64, Vec<usize>)> {
    let sign = f64::from(axis_sign);
    let cross_axis = 3 - axis - height_axis;
    let mut matches: Vec<(f64, usize)> = Vec::new();
    for wall in walls {
        if wall.normal[axis].abs() < AXIS_ALIGNED_COS {
            continue;
        }
        let (plane_lo, plane_hi) = wall.bounds[axis];
        let plane_pos = (plane_lo + plane_hi) / 2.0;
        let (cross_lo, cross_hi) = wall.bounds[cross_axis];
        let (height_lo, height_hi) = wall.bounds[height_axis];
        let wall_top = if axis_sign > 0 { height_hi } else { height_lo };
        let candidate_base = if axis_sign > 0 { height_lo } else { height_hi };
        if (plane_pos - pos).abs() <= tol
            && (wall_top - top).abs() <= tol
            && sign * top - tol > sign * candidate_base
            && cross_lo <= lo + tol
            && cross_hi >= hi - tol
        {
            matches.push((candidate_base, wall.face));
        }
    }
    if matches.is_empty() {
        return None;
    }
    let base = matches[py::first_max(&matches, |m| sign * m.0)].0;
    let faces = matches
        .iter()
        .filter(|m| m.0 == base)
        .map(|m| m.1)
        .collect();
    Some((base, faces))
}

/// `_touches_plan`: tolerance-inclusive contact in the pads' transverse plane.
fn touches_plan(a: &RaisedPad, b: &RaisedPad, tol: f64) -> bool {
    if a.axis != b.axis || a.direction != b.direction {
        return false;
    }
    let (ba, bb) = (a.bounds(), b.bounds());
    let axis = a.axis_index();
    (0..3)
        .filter(|&i| i != axis)
        .all(|i| ba[i].1.min(bb[i].1) - ba[i].0.max(bb[i].0) >= -tol)
}

/// `_tier_suppresses`: whether a raw top region touches the pad's base, as a tier below it.
fn tier_suppresses(pad: &RaisedPad, region: &RaisedPad, tol: f64) -> bool {
    let axial = pad.axis_index();
    let pad_span = pad.bounds()[axial];
    let region_span = region.bounds()[axial];
    let pad_base = if pad.direction > 0 {
        pad_span.0
    } else {
        pad_span.1
    };
    let region_top = if region.direction > 0 {
        region_span.1
    } else {
        region_span.0
    };
    (region_top - pad_base).abs() <= tol && touches_plan(pad, region, tol)
}

/// What one solid's reading shares across its six orientations.
struct Solid<'r, 'c, 'a> {
    part: &'a Part,
    effective: &'r EffectiveFaces<'c, 'a>,
    geometry: &'r GeometryGraph<'a>,
    faces: &'r [usize],
    planar: Vec<Planar>,
    bounds: Bounds,
    tol: f64,
}

/// A vertical face of the blended path: the face, its vertex box and its own normal.
type Vertical = (usize, [(f64, f64); 3], V3);

/// A rectangular top: its rounded rectangle and level, and its face.
struct Top {
    u: (f64, f64),
    v: (f64, f64),
    level: f64,
    face: usize,
}

impl Solid<'_, '_, '_> {
    /// Whether the top's certified material side faces out along the pad's direction: `None`
    /// when no side is certified (refused).
    fn faces_out(&self, face: usize, axis: usize, axis_sign: i32) -> Option<bool> {
        let surface = self.effective.surface_use(face, true).as_ref().ok()?;
        let side = surface.material_side.as_ref()?;
        Some(side.outward[axis] * f64::from(axis_sign) >= AXIS_ALIGNED_COS)
    }

    /// `_recognise_rectangular_pads_one`: the sharp path.
    fn sharp(&self, axis: usize, axis_sign: i32) -> Vec<Proposal> {
        let tol = self.tol;
        let sign = f64::from(axis_sign);
        let (_, transverse @ [u_axis, v_axis]) =
            AXES[AXES.iter().position(|a| a.0 == axis).unwrap()];
        let (bb_lo, bb_hi) = span(&self.bounds, axis);
        let (bb_u0, bb_u1) = span(&self.bounds, u_axis);
        let (bb_v0, bb_v1) = span(&self.bounds, v_axis);
        let mut tops: Vec<Top> = Vec::new();
        for planar in &self.planar {
            if planar.normal[axis].abs() < AXIS_ALIGNED_COS {
                continue;
            }
            let (u0, u1) = planar.bounds[u_axis];
            let (v0, v1) = planar.bounds[v_axis];
            let (top_lo, top_hi) = planar.bounds[axis];
            let level = (top_lo + top_hi) / 2.0;
            if u1 - u0 <= tol
                || v1 - v0 <= tol
                || sign * (level - if axis_sign > 0 { bb_lo } else { bb_hi }) <= tol
            {
                continue;
            }
            let rectangle = (u1 - u0) * (v1 - v0);
            // A face without an area fails the test rather than passing it.
            let Some(area) = planar.area else { continue };
            if (area - rectangle).abs() > py_max(tol * tol, 0.005 * rectangle) {
                continue;
            }
            let full_u = bb_u0 + tol >= u0 && bb_u1 - tol <= u1;
            let full_v = bb_v0 + tol >= v0 && bb_v1 - tol <= v1;
            if full_u || full_v {
                continue;
            }
            let r = |v: f64| py::round_to(v, 3);
            tops.push(Top {
                u: (r(u0), r(u1)),
                v: (r(v0), r(v1)),
                level: r(level),
                face: planar.face,
            });
        }

        // Each pad's base is recovered from its own four perimeter walls, not from a part-wide
        // level below the top.
        let walls: Vec<&Planar> = self
            .planar
            .iter()
            .filter(|p| p.normal[axis].abs() <= AXIS_ZERO_COS)
            .collect();
        let mut proposals: Vec<Proposal> = Vec::new();
        for top in &tops {
            let role = |along: usize, pos: f64, across: (f64, f64)| {
                wall_role(&walls, along, pos, across, top.level, tol, axis, axis_sign)
            };
            let roles = [
                role(u_axis, top.u.0, top.v),
                role(u_axis, top.u.1, top.v),
                role(v_axis, top.v.0, top.u),
                role(v_axis, top.v.1, top.u),
            ];
            let Some(roles) = roles.into_iter().collect::<Option<Vec<_>>>() else {
                continue;
            };
            // A pad at the envelope may have one exterior wall running down to the stock base:
            // the highest wall base is the local support.
            let base = roles[py::first_max(&roles, |r| sign * r.0)].0;
            let mut faces = roles.into_iter().map(|r| r.1);
            proposals.push(Proposal {
                record: pad_record(axis, axis_sign, transverse, top.u, top.v, base, top.level),
                top: top.face,
                walls: [(); 4].map(|_| faces.next().expect("four roles")),
            });
        }
        if proposals.is_empty() {
            return proposals;
        }

        let regions: Vec<RaisedPad> = tops
            .iter()
            .map(|t| pad_record(axis, axis_sign, transverse, t.u, t.v, t.level, t.level))
            .collect();
        let mut needed: BTreeSet<usize> = proposals.iter().map(|p| p.top).collect();
        needed.extend(
            tops.iter()
                .zip(&regions)
                .filter(|(_, region)| {
                    proposals
                        .iter()
                        .any(|p| tier_suppresses(&p.record, region, tol))
                })
                .map(|(t, _)| t.face),
        );

        // The material side is the expensive query: asked only of tops that can define or
        // suppress a proposal. An unverified ledge stays suppression context, so refusing it
        // never lets the tier above it become a pad.
        let mut suppressing: BTreeSet<usize> = BTreeSet::new();
        let mut certified: BTreeSet<usize> = BTreeSet::new();
        for top in &tops {
            if !needed.contains(&top.face) {
                continue;
            }
            match self.faces_out(top.face, axis, axis_sign) {
                None => {
                    suppressing.insert(top.face);
                }
                Some(false) => {}
                Some(true) => {
                    suppressing.insert(top.face);
                    certified.insert(top.face);
                }
            }
        }
        proposals.retain(|p| certified.contains(&p.top));

        // A tiered tower's ledges touch the pad at its recovered base; ledges at other levels
        // (on a sloped support) do not suppress it.
        let suppression: Vec<&RaisedPad> = tops
            .iter()
            .zip(&regions)
            .filter(|(t, _)| suppressing.contains(&t.face))
            .map(|(_, region)| region)
            .collect();
        proposals.retain(|p| {
            !suppression
                .iter()
                .any(|region| tier_suppresses(&p.record, region, tol))
        });
        proposals
    }

    /// `_recognise_blended_rectangular_pads_one`: one complete four-corner convex blend cycle
    /// around a rectangular top.
    fn blended(&self, axis: usize, axis_sign: i32) -> Result<Vec<Proposal>, PadError> {
        let tol = self.tol;
        let sign = f64::from(axis_sign);
        let geometry = self.geometry;
        let (_, transverse @ [u_index, v_index]) =
            AXES[AXES.iter().position(|a| a.0 == axis).unwrap()];
        let (bb_lo, bb_hi) = span(&self.bounds, axis);
        let (bb_u0, bb_u1) = span(&self.bounds, u_index);
        let (bb_v0, bb_v1) = span(&self.bounds, v_index);
        let local = |f: usize| self.faces.contains(&f);
        // The vertical faces: their own normals (`normal_at`, the face's sense), their vertex
        // boxes.
        let vertical: Vec<Vertical> = self
            .planar
            .iter()
            .filter_map(|p| {
                let n = geometry.normal(p.face)?;
                (n[axis].abs() <= AXIS_ZERO_COS).then_some((p.face, p.bounds, n))
            })
            .collect();
        let vertical_of = |f: usize| vertical.iter().find(|v| v.0 == f);
        let terminal = if axis_sign > 0 { 1 } else { 0 };
        let base_end = 1 - terminal;
        let pick = |s: (f64, f64), end: usize| if end == 1 { s.1 } else { s.0 };
        let mut eligible: Option<Vec<usize>> = None;
        let mut proposals = Vec::new();
        for planar in &self.planar {
            let top_face = planar.face;
            let (top_lo, top_hi) = planar.bounds[axis];
            let top = py::round_to((top_lo + top_hi) / 2.0, 3);
            if sign * top - tol <= sign * if axis_sign > 0 { bb_lo } else { bb_hi } {
                continue;
            }
            // Roles u0, u1, v0, v1; a role found twice refuses the top.
            let mut roles: [Option<usize>; 4] = [None; 4];
            let mut repeated = false;
            for n in geometry.neighbours(top_face) {
                let Some(&(face, bounds, normal)) = vertical_of(n) else {
                    continue;
                };
                let wall_top = pick(bounds[axis], terminal);
                if (wall_top - top).abs() > tol {
                    continue;
                }
                let role = if normal[u_index].abs() >= AXIS_ALIGNED_COS
                    && normal[v_index].abs() <= AXIS_ZERO_COS
                {
                    usize::from(normal[u_index] > 0.0)
                } else if normal[v_index].abs() >= AXIS_ALIGNED_COS
                    && normal[u_index].abs() <= AXIS_ZERO_COS
                {
                    2 + usize::from(normal[v_index] > 0.0)
                } else {
                    continue;
                };
                if roles[role].is_some() {
                    repeated = true;
                    break;
                }
                roles[role] = Some(face);
            }
            let (false, [Some(ru0), Some(ru1), Some(rv0), Some(rv1)]) = (repeated, roles) else {
                continue;
            };
            let ordered = [ru0, ru1, rv0, rv1];
            let mid = |f: usize, i: usize| {
                let (lo, hi) = geometry.bounds(f)[i];
                py::round_to((lo + hi) / 2.0, 3)
            };
            let (u0, u1) = (mid(ru0, u_index), mid(ru1, u_index));
            let (v0, v1) = (mid(rv0, v_index), mid(rv1, v_index));
            if u1 - u0 <= tol || v1 - v0 <= tol {
                continue;
            }
            let full_u = bb_u0 + tol >= u0 && bb_u1 - tol <= u1;
            let full_v = bb_v0 + tol >= v0 && bb_v1 - tol <= v1;
            if full_u || full_v {
                continue;
            }
            let cross = [
                (geometry.bounds(ru0)[v_index], (v0, v1)),
                (geometry.bounds(ru1)[v_index], (v0, v1)),
                (geometry.bounds(rv0)[u_index], (u0, u1)),
                (geometry.bounds(rv1)[u_index], (u0, u1)),
            ];
            if cross.iter().all(|(actual, expected)| {
                actual.0 <= expected.0 + tol && actual.1 >= expected.1 - tol
            }) {
                continue; // the sharp path owns uninterrupted wall roles
            }

            let eligible = eligible.get_or_insert_with(|| {
                let facts = geometry.blend_facts();
                (0..facts.len())
                    .filter(|&i| {
                        let chain = &facts[i];
                        if chain.side != super::blend_view::SmoothSide::Convex
                            || chain.blend_faces.len() != 1
                            || chain.supports.iter().any(|s| s.len() != 1)
                        {
                            return false;
                        }
                        let (left, right) = (chain.supports[0][0], chain.supports[1][0]);
                        if vertical_of(left).is_none() || vertical_of(right).is_none() {
                            return false;
                        }
                        if !chain.blend_faces.iter().all(|&f| local(f)) {
                            return false;
                        }
                        if geometry
                            .surface_fact(chain.blend_faces[0])
                            .is_none_or(|f| f.kind != SurfaceKind::Cylinder)
                        {
                            return false;
                        }
                        let ends = |f: usize| pick(geometry.bounds(f)[axis], terminal);
                        (ends(left) - ends(right)).abs() <= tol
                    })
                    .collect()
            });

            let pair = |a: usize, b: usize| if a < b { (a, b) } else { (b, a) };
            let chain_pair = |i: usize| {
                let c = &geometry.blend_facts()[i];
                pair(c.supports[0][0], c.supports[1][0])
            };
            let expected = [
                pair(ru0, rv0),
                pair(ru0, rv1),
                pair(ru1, rv0),
                pair(ru1, rv1),
            ];
            let by_pair: Vec<Vec<usize>> = expected
                .iter()
                .map(|&p| {
                    eligible
                        .iter()
                        .copied()
                        .filter(|&i| chain_pair(i) == p)
                        .collect()
                })
                .collect();
            if by_pair.iter().any(|chains| chains.len() != 1) {
                continue;
            }
            let selected: Vec<usize> = by_pair.iter().map(|chains| chains[0]).collect();

            let spans = ordered.map(|f| geometry.bounds(f)[axis]);
            if spans.iter().any(|&s| (pick(s, terminal) - top).abs() > tol) {
                continue;
            }
            let bases = spans.map(|s| pick(s, base_end));
            let base = bases[py::first_max(&bases, |b| sign * b)];
            if sign * top - tol <= sign * base {
                continue;
            }

            // Four quarter-circle removals explain the rounded top exactly; another trim or hole
            // cannot borrow the blend cycle's permission to become a pad.
            if self.part.faces[top_face].loops.len() != 1 {
                continue;
            }
            let facts = geometry.blend_facts();
            let removed = py::fsum(
                selected
                    .iter()
                    .map(|&i| (1.0 - std::f64::consts::PI / 4.0) * facts[i].radius.powi(2)),
            );
            let expected_area = (u1 - u0) * (v1 - v0) - removed;
            let Some(area) = planar.area else { continue };
            if (area - expected_area).abs() > py_max(tol * tol, 0.005 * expected_area) {
                continue;
            }

            let bridges = geometry
                .collapsed_bridges(&selected)
                .map_err(PadError::Collapse)?;
            if bridges.len() != 4 {
                return Err(PadError::NoUniqueBridges);
            }
            for &i in &selected {
                let chain = &facts[i];
                let wanted = chain_pair(i);
                let matching: Vec<_> = bridges
                    .iter()
                    .filter(|b| pair(b.supports.0, b.supports.1) == wanted)
                    .collect();
                let [bridge] = matching.as_slice() else {
                    return Err(PadError::NoUniqueBridge);
                };
                let mut faces: Vec<usize> = chain
                    .blend_faces
                    .iter()
                    .chain(chain.supports.iter().flatten())
                    .copied()
                    .collect();
                faces.sort_unstable();
                faces.dedup();
                let keys = |arcs: &[super::blend_view::Shared]| {
                    let mut keys: Vec<_> = arcs.iter().map(|a| (a.key(), a.edge)).collect();
                    keys.sort_unstable();
                    keys
                };
                if bridge.faces != faces || keys(&bridge.boundary) != keys(&chain.boundary) {
                    return Err(PadError::LostProvenance);
                }
            }

            if self.faces_out(top_face, axis, axis_sign) != Some(true) {
                continue;
            }
            proposals.push(Proposal {
                record: pad_record(axis, axis_sign, transverse, (u0, u1), (v0, v1), base, top),
                top: top_face,
                walls: ordered.map(|f| vec![f]),
            });
        }
        Ok(proposals)
    }
}

/// `_resolve_axis_ambiguity`: among proposals linked by shared faces, the readings along the
/// orientation whose shortest attachment span is the unique minimum (within *tol*); a tied
/// minimum is refused. Disjoint proposals stay independent.
fn resolve_axis_ambiguity(proposals: Vec<Proposal>, tol: f64) -> Vec<Proposal> {
    let evidence: Vec<BTreeSet<usize>> = proposals.iter().map(Proposal::faces).collect();
    let mut remaining: BTreeSet<usize> = (0..proposals.len()).collect();
    let mut selected = Vec::new();
    while let Some(first) = remaining.pop_first() {
        let mut component = vec![first];
        let mut frontier = vec![first];
        while let Some(current) = frontier.pop() {
            let neighbours: Vec<usize> = remaining
                .iter()
                .copied()
                .filter(|&i| !evidence[current].is_disjoint(&evidence[i]))
                .collect();
            for i in &neighbours {
                remaining.remove(i);
            }
            component.extend(&neighbours);
            frontier.extend(neighbours);
        }
        // Orientations in first-seen order, each with its proposals.
        let mut by_axis: Vec<((String, i32), Vec<usize>)> = Vec::new();
        for &i in &component {
            let r = &proposals[i].record;
            let key = (r.axis.clone(), r.direction);
            match by_axis.iter_mut().find(|(k, _)| *k == key) {
                Some((_, members)) => members.push(i),
                None => by_axis.push((key, vec![i])),
            }
        }
        let shortest = |members: &[usize]| {
            let extents: Vec<f64> = members
                .iter()
                .map(|&i| proposals[i].record.axial_extent())
                .collect();
            extents[py::first_min(&extents, |&e| e)]
        };
        let minima: Vec<f64> = by_axis.iter().map(|(_, m)| shortest(m)).collect();
        let minimum = minima[py::first_min(&minima, |&e| e)];
        let winners: Vec<usize> = (0..by_axis.len())
            .filter(|&k| (minima[k] - minimum).abs() <= tol)
            .collect();
        if let [winner] = winners.as_slice() {
            let mut members = by_axis[*winner].1.clone();
            members.sort_unstable();
            selected.extend(members);
        }
    }
    let mut proposals: Vec<Option<Proposal>> = proposals.into_iter().map(Some).collect();
    selected
        .into_iter()
        .map(|i| proposals[i].take().expect("each proposal selected once"))
        .collect()
}

/// Every solid's pads, each with the proposals that read it, in record order (equal records of
/// one solid are one occurrence).
fn occurrences<'a>(
    part: &'a Part,
    effective: &EffectiveFaces<'_, 'a>,
    opts: &PadOptions,
) -> Result<Vec<(RaisedPad, Vec<Proposal>)>, PadError> {
    let tol = opts.tol.unwrap_or(TOL);
    let geometry = GeometryGraph::new(part);
    let mut found: Vec<(RaisedPad, Vec<Proposal>)> = Vec::new();
    for (s, solid) in part.solids.iter().enumerate() {
        let reading = Solid {
            part,
            effective,
            geometry: &geometry,
            faces: &solid.faces,
            planar: planar_faces(part, &solid.faces, effective),
            bounds: solid_properties::bounding_box(part, s),
            tol,
        };
        let mut proposals = Vec::new();
        for (axis, _) in AXES {
            for axis_sign in [1, -1] {
                proposals.extend(reading.sharp(axis, axis_sign));
                proposals.extend(reading.blended(axis, axis_sign)?);
            }
        }
        let mut by_record: Vec<(RaisedPad, Vec<Proposal>)> = Vec::new();
        for proposal in resolve_axis_ambiguity(proposals, tol) {
            match by_record.iter_mut().find(|(r, _)| *r == proposal.record) {
                Some((_, group)) => group.push(proposal),
                None => by_record.push((proposal.record.clone(), vec![proposal])),
            }
        }
        found.extend(by_record);
    }
    // Stable: equal records of different solids stay in solid order.
    found.sort_by(|a, b| record_order(&a.0, &b.0));
    Ok(found)
}

/// `recognise_rectangular_pads`: bounded rectangular raised tops, each solid on its own, sorted
/// by record.
pub fn recognise_rectangular_pads(
    part: &Part,
    opts: &PadOptions,
) -> Result<Vec<RaisedPad>, PadError> {
    let ctx = Context::new(part);
    Ok(occurrences(part, &EffectiveFaces::new(&ctx), opts)?
        .into_iter()
        .map(|(record, _)| record)
        .collect())
}

/// `_discover_rectangular_pads` with an evidence writer: each pad defined by its top and its
/// four perimeter walls (one face per role), five distinct faces of one valid solid, no top
/// defining two pads, and equal records only where they read the same faces. Python's
/// `local_degradation` (skipping, not refusing, a pad off any valid solid) is not ported.
pub fn discover(
    ctx: &Context<'_>,
    opts: &PadOptions,
) -> Result<Vec<Occurrence<RaisedPad>>, PadError> {
    let part = ctx.part;
    let effective = EffectiveFaces::new(ctx);
    let mut used_tops: BTreeSet<usize> = BTreeSet::new();
    let mut found = Vec::new();
    for (record, alternatives) in occurrences(part, &effective, opts)? {
        let mut signatures: Vec<[usize; 5]> = Vec::new();
        for proposal in &alternatives {
            let mut signature = [proposal.top; 5];
            for (k, faces) in proposal.walls.iter().enumerate() {
                let distinct: BTreeSet<usize> = faces.iter().copied().collect();
                let &[face] = distinct.iter().collect::<Vec<_>>().as_slice() else {
                    return Err(PadError::AmbiguousRole);
                };
                signature[k + 1] = *face;
            }
            if signature.iter().collect::<BTreeSet<_>>().len() != 5 {
                return Err(PadError::NotFiveFaces);
            }
            if !signatures.contains(&signature) {
                signatures.push(signature);
            }
        }
        let [signature] = signatures.as_slice() else {
            return Err(PadError::AmbiguousOccurrences);
        };
        if used_tops.contains(&signature[0]) {
            return Err(PadError::SharedTop);
        }
        let mut ordered = signature.to_vec();
        ordered.sort_unstable();
        if common_valid_solid(part, &ordered).is_none() {
            return Err(PadError::NoValidSolid);
        }
        used_tops.insert(signature[0]);
        let top = signature[0];
        if ordered
            .iter()
            .any(|&f| effective.surface_use(f, f == top).is_err())
        {
            return Err(PadError::ProvenanceUnavailable);
        }
        found.push(Occurrence {
            record,
            defining: ordered,
            context: Vec::new(),
        });
    }
    Ok(found)
}
