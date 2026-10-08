//! Original-source proof of a three-support channel ending on a native bore
//! (`quiddity._cylindrical_channels`): two planar walls facing each other across the width and
//! one planar floor, all concave to one another, convex to a native concave cylinder whose axis
//! lies across the run and to a planar stock face at the far end. The removed cell (the
//! channel's footprint swept from the far plane to the bore) must be covered by the original
//! supports, hold no material, and open into air beyond either end and out of the mouth.
//!
//! Python builds the cell as a box less the bore cylinder (`Shape.cut`). Over a footprint that
//! lies strictly inside the bore's single-valued branch the cut is exactly the footprint swept
//! from the far plane up to that branch, so it is built so here ([`capped_prism`], shared with
//! the pocket and passage proofs): its far face is the expected far rectangle and its planar
//! faces lie on the far, floor, mouth and wall planes by construction, which are two of Python's
//! checks. A probe beyond an end is the cell's cap swept by the probe thickness, which is what
//! Python's translated copy less the cell (`Shape.cut`) leaves while the cell is thicker than
//! the probe; the probe out of the mouth is measured as the translated copy less its common
//! part with the cell, as `plane_envelope_passages` does. An unanswered probe, or a cell the
//! kernel cannot build, proves nothing, so refuses.

use std::collections::BTreeSet;

use super::Context;
use super::analytic_surfaces::SurfaceKind;
use super::effective_surfaces::{EffectiveFaces, SurfaceProvenance};
use super::entry_treatments::material_fraction;
use super::evidence::common_valid_solid;
use super::graph::{is_planar, normal};
use super::support_patches::covered_patch;
use crate::kernel::brep::{Arc, Edge, Face, Loop, Part, Solid};
use crate::kernel::geom::{self, Curve, Frame, Surface, V3};
use crate::kernel::rays::RayCaster;
use crate::kernel::sampling::sample_edge;
use crate::kernel::volume::{Probe, common_volume};

/// Per axis `(low, high)`.
pub type Bounds3 = [(f64, f64); 3];

/// A proved channel (`CylindricalChannelProof`): the floor and walls sorted by face, the bore,
/// the far stock face, the solid, the axes and open side it was asked about, the channel box
/// (its width between the wall planes), the run interval from the far plane to the branch over
/// the footprint's centre, which run end is the bore (0 low, 1 high), the bore's axis point,
/// direction and radius, and the cell's volume.
#[derive(Clone, Debug, PartialEq)]
pub struct CylindricalChannelProof {
    pub supports: Vec<usize>,
    pub cylinder: usize,
    pub planar_context: usize,
    pub owner: usize,
    pub run_axis: String,
    pub width_axis: String,
    pub open_sign: i32,
    pub bounds: Bounds3,
    pub run_interval: (f64, f64),
    pub cylindrical_end: usize,
    pub axis_point: V3,
    pub axis_direction: V3,
    pub radius: f64,
    pub volume: f64,
}

/// One end of a [`capped_prism`]: the plane across the run at a height, or one branch of a
/// cylinder whose axis lies across the run (the larger root of the height along the run for
/// `sign` 1, the smaller for -1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Cap {
    Plane(f64),
    Branch {
        centre: V3,
        axis: V3,
        radius: f64,
        sign: f64,
    },
}

impl Cap {
    /// The cap's height along *run* over the point *foot* (only its part across the run is
    /// read); `None` where a branch does not reach.
    pub(crate) fn height(&self, run: V3, foot: V3) -> Option<f64> {
        match *self {
            Cap::Plane(at) => Some(at),
            Cap::Branch {
                centre,
                axis,
                radius,
                sign,
            } => {
                // The quadratic in the height of `foot + h run` (measured from the plane through
                // the origin across the run) on which the point is `radius` from the axis,
                // written in the offset across the axis: Python's form subtracts the squared
                // offset along the axis from the squared distance, which loses the height to
                // cancellation far along the axis.
                let delta = geom::sub(across(run, foot), centre);
                let k = geom::dot(axis, run);
                let off = geom::sub(delta, geom::scale(axis, geom::dot(delta, axis)));
                let a = 1.0 - k * k;
                let b = 2.0 * geom::dot(off, run);
                let c = geom::dot(off, off) - radius * radius;
                let discriminant = b * b - 4.0 * a * c;
                (discriminant > 0.0 && a > 0.0)
                    .then(|| (-b + sign * discriminant.sqrt()) / (2.0 * a))
            }
        }
    }

    /// The cap moved by *offset*.
    pub(crate) fn moved(&self, run: V3, offset: V3) -> Cap {
        match *self {
            Cap::Plane(at) => Cap::Plane(at + geom::dot(offset, run)),
            Cap::Branch {
                centre,
                axis,
                radius,
                sign,
            } => Cap::Branch {
                centre: geom::add(centre, offset),
                axis,
                radius,
                sign,
            },
        }
    }
}

/// *point* moved along *run* onto the plane through the origin across it.
fn across(run: V3, point: V3) -> V3 {
    geom::sub(point, geom::scale(run, geom::dot(point, run)))
}

/// The curve where the side through feet *fp* → *fq* meets *cap*, from its point *p* to *q*,
/// with whether that walk follows the curve's parameter: a line on a plane; on a branch the
/// conic the side's plane cuts from the cylinder (a circle, an ellipse, or a line where the
/// side runs along the axis or the arc between the points is straight to round-off).
fn cap_curve(cap: &Cap, run: V3, fp: V3, fq: V3, p: V3, q: V3) -> Option<(Curve, bool)> {
    let line = (
        Curve::Line {
            origin: p,
            dir: geom::sub(q, p),
        },
        true,
    );
    let Cap::Branch {
        centre,
        axis,
        radius,
        ..
    } = *cap
    else {
        return Some(line);
    };
    let middle_foot = geom::scale(geom::add(fp, fq), 0.5);
    let middle = geom::add(
        across(run, middle_foot),
        geom::scale(run, cap.height(run, middle_foot)?),
    );
    let chord = geom::sub(q, p);
    let offset = geom::sub(middle, p);
    let along = geom::dot(offset, chord) / geom::dot(chord, chord);
    let sagitta = geom::norm(geom::sub(offset, geom::scale(chord, along)));
    // An ellipse this flat evaluates no better than the chord it stands in for.
    if sagitta <= 1e-9 * (radius + geom::norm(chord)) {
        return Some(line);
    }
    let n = geom::unit(geom::cross(geom::sub(fq, fp), run))?;
    let ka = geom::dot(n, axis);
    if ka.abs() < 1e-12 {
        return None;
    }
    let origin = geom::add(
        centre,
        geom::scale(axis, geom::dot(geom::sub(p, centre), n) / ka),
    );
    let curve = if 1.0 - ka.abs() <= 1e-12 {
        let x = geom::unit(geom::sub(p, origin))?;
        Curve::Circle {
            frame: Frame {
                origin,
                x,
                y: geom::cross(n, x),
                z: n,
            },
            radius,
        }
    } else {
        // Minor axis across the cylinder's axis, major axis along it tilted into the plane.
        let minor = geom::unit(geom::cross(axis, n))?;
        let major = geom::cross(n, minor);
        Curve::Ellipse {
            frame: Frame {
                origin,
                x: major,
                y: minor,
                z: geom::cross(major, minor),
            },
            major: radius / ka.abs(),
            minor: radius,
        }
    };
    let turn = |t: f64| t.rem_euclid(std::f64::consts::TAU);
    let start = curve.parameter(p);
    let forward = turn(curve.parameter(middle) - start) < turn(curve.parameter(q) - start);
    Some((curve, forward))
}

/// The face of *cap* (`low` facing against the run, otherwise along it) bounded by *edges*.
fn cap_face(
    cap: &Cap,
    run: V3,
    low: bool,
    at: V3,
    x: V3,
    edges: Vec<(usize, bool)>,
) -> Option<Face> {
    let outward = if low { geom::scale(run, -1.0) } else { run };
    let loops = vec![Loop {
        edges,
        vertex: None,
    }];
    let (surface, reversed) = match *cap {
        Cap::Plane(_) => (
            Surface::Plane {
                frame: Frame {
                    origin: at,
                    x,
                    y: geom::cross(outward, x),
                    z: outward,
                },
            },
            false,
        ),
        Cap::Branch {
            centre,
            axis,
            radius,
            sign,
        } => {
            // The seam opposite the branch, so the face's loop never crosses it.
            let up = geom::unit(geom::sub(run, geom::scale(axis, geom::dot(run, axis))))?;
            let x = geom::scale(up, -sign);
            let surface = Surface::Cylinder {
                frame: Frame {
                    origin: centre,
                    x,
                    y: geom::cross(axis, x),
                    z: axis,
                },
                radius,
            };
            let (u, v) = surface.parameters(at, None)?;
            let natural = surface.normal(u, v)?;
            (surface, geom::dot(natural, outward) < 0.0)
        }
    };
    Some(Face {
        surface,
        reversed,
        loops,
        solid: Some(0),
        pcurves: Vec::new(),
    })
}

/// The polygon *footprint* (only its points' parts across *run* are read) swept along *run*
/// from *low* to *high*, as a one-solid part: its two caps, a plane through each footprint
/// edge, and their exact edges. `None` when the footprint is degenerate, a cap does not reach
/// over a corner, or the caps meet or cross at one.
pub(crate) fn capped_prism(run: V3, footprint: &[V3], low: Cap, high: Cap) -> Option<Part> {
    let n = footprint.len();
    if n < 3 {
        return None;
    }
    let mut feet: Vec<V3> = footprint.iter().map(|&p| across(run, p)).collect();
    let twice_area: f64 = (0..n)
        .map(|i| geom::dot(geom::cross(feet[i], feet[(i + 1) % n]), run))
        .sum();
    if twice_area.abs() <= 1e-300 {
        return None;
    }
    // Counter-clockwise about the run.
    if twice_area < 0.0 {
        feet.reverse();
    }
    let mut bottom = Vec::with_capacity(n);
    let mut top = Vec::with_capacity(n);
    for &f in &feet {
        let (lo, hi) = (low.height(run, f)?, high.height(run, f)?);
        if hi - lo <= 0.0 {
            return None;
        }
        bottom.push(geom::add(f, geom::scale(run, lo)));
        top.push(geom::add(f, geom::scale(run, hi)));
    }
    // Edges: bottom i (corner i to i + 1), top i, then the riser at each corner; vertices:
    // bottom corners, then top corners.
    let mut edges: Vec<Edge> = Vec::with_capacity(3 * n);
    for (cap, corners, level) in [(&low, &bottom, 0), (&high, &top, n)] {
        for i in 0..n {
            let j = (i + 1) % n;
            let (start, end) = (corners[i], corners[j]);
            let (curve, same_sense) = cap_curve(cap, run, feet[i], feet[j], start, end)?;
            edges.push(Edge {
                samples: sample_edge(&curve, start, end, same_sense, false),
                curve,
                start,
                end,
                vertices: (level + i, level + j),
                same_sense,
            });
        }
    }
    for i in 0..n {
        let (start, end) = (bottom[i], top[i]);
        edges.push(Edge {
            curve: Curve::Line {
                origin: start,
                dir: geom::sub(end, start),
            },
            start,
            end,
            vertices: (i, n + i),
            same_sense: true,
            samples: vec![start, end],
        });
    }
    let x = geom::unit(geom::sub(feet[1], feet[0]))?;
    let middle = |e: &Edge| e.samples[e.samples.len() / 2];
    let mut faces = vec![
        cap_face(
            &low,
            run,
            true,
            middle(&edges[0]),
            x,
            (0..n).rev().map(|i| (i, false)).collect(),
        )?,
        cap_face(
            &high,
            run,
            false,
            middle(&edges[n]),
            x,
            (0..n).map(|i| (n + i, true)).collect(),
        )?,
    ];
    for i in 0..n {
        let j = (i + 1) % n;
        let along = geom::unit(geom::sub(feet[j], feet[i]))?;
        let z = geom::unit(geom::cross(along, run))?;
        // Parameters height first, then along the side: the coverage test scans lines of
        // constant second parameter, across which each cap curve is single-valued, so a
        // sampled curve errs across the line by no more than its chord tolerance (scanned the
        // other way, a nearly level arc's sag would be magnified at its crest).
        faces.push(Face {
            surface: Surface::Plane {
                frame: Frame {
                    origin: bottom[i],
                    x: run,
                    y: geom::cross(z, run),
                    z,
                },
            },
            reversed: false,
            loops: vec![Loop {
                edges: vec![
                    (i, true),
                    (2 * n + j, true),
                    (n + i, false),
                    (2 * n + i, false),
                ],
                vertex: None,
            }],
            solid: Some(0),
            pcurves: Vec::new(),
        });
    }
    let solid = Solid {
        faces: (0..faces.len()).collect(),
    };
    let part = Part::new(faces, edges, vec![solid]);
    part.solid_is_valid(0).then_some(part)
}

/// The side faces of a [`capped_prism`] (every face after its two caps).
pub(crate) fn sides(cell: &Part) -> Vec<(&Part, usize)> {
    (2..cell.faces.len()).map(|f| (cell, f)).collect()
}

/// Whether *supports* cover the original planar face *patch*, which contains the run
/// (`covered_patch`). The kernel scans a planar patch along lines of constant second frame
/// parameter and widens supports across them by the chord tolerance, so the answer depends on
/// how the plane is parametrised: scanned at constant height, a nearly level arc's sampled
/// chords stray across the line by far more than that tolerance near its crest. The patch is
/// asked as a copy of the face whose plane is parametrised height first ([`capped_prism`]'s
/// sides are too), across which every cap curve is single-valued; the face is the same.
pub(crate) fn covered_along(run: V3, patch: (&Part, usize), supports: &[(&Part, usize)]) -> bool {
    let (part, face) = patch;
    let Some(copy) = along_run(part, face, run) else {
        return covered_patch(patch, supports);
    };
    covered_patch((&copy, 0), supports)
}

/// *face* of *part* alone, its plane parametrised with *run* first; `None` unless the face is
/// planar and contains the run.
fn along_run(part: &Part, face: usize, run: V3) -> Option<Part> {
    let source = &part.faces[face];
    let Surface::Plane { frame } = source.surface else {
        return None;
    };
    if geom::dot(frame.z, run).abs() > 1e-9 {
        return None;
    }
    let x = geom::unit(geom::sub(
        run,
        geom::scale(frame.z, geom::dot(run, frame.z)),
    ))?;
    let used = part.face_edges(face);
    let edges: Vec<Edge> = used.iter().map(|&e| part.edges[e].clone()).collect();
    let renumber = |e: usize| used.iter().position(|&u| u == e).expect("face edge");
    let copy = Face {
        surface: Surface::Plane {
            frame: Frame {
                origin: frame.origin,
                x,
                y: geom::cross(frame.z, x),
                z: frame.z,
            },
        },
        reversed: source.reversed,
        loops: source
            .loops
            .iter()
            .map(|lp| Loop {
                edges: lp.edges.iter().map(|&(e, f)| (renumber(e), f)).collect(),
                vertex: lp.vertex,
            })
            .collect(),
        solid: None,
        pcurves: Vec::new(),
    };
    Some(Part::new(vec![copy], edges, Vec::new()))
}

/// A part's volume (`Shape.volume`); `None` where the kernel cannot integrate it.
pub(crate) fn volume(part: &Part) -> Option<f64> {
    part.solid_mass(0).map(|m| m.0)
}

/// The material *owner* holds inside *probe* (`probe_volume`).
pub(crate) fn material(ctx: &Context<'_>, owner: usize, probe: &Part) -> Option<f64> {
    common_volume(
        ctx.solid_classifier(owner),
        &Probe::Solid(RayCaster::for_solid(probe, 0)),
    )
}

/// Whether *probe* is built and holds no more than 1e-9 of its volume in material; an unbuilt
/// or unanswered probe proves nothing.
pub(crate) fn empty(ctx: &Context<'_>, owner: usize, probe: Option<&Part>) -> bool {
    probe
        .and_then(|p| material_fraction(ctx, owner, p))
        .is_some_and(|f| f <= 1e-9)
}

/// What Python's translated copy of the cell less the cell leaves beyond one end, for a move of
/// *by* along the run: the end's cap swept by that distance. `None` (the probe proves nothing)
/// where the cell is not that thick at a footprint corner, where the copy's far end would
/// reach back into the cell.
pub(crate) fn end_probe(run: V3, footprint: &[V3], low: Cap, high: Cap, by: f64) -> Option<Part> {
    for &p in footprint {
        if high.height(run, p)? - low.height(run, p)? <= by.abs() {
            return None;
        }
    }
    let offset = geom::scale(run, by);
    if by > 0.0 {
        capped_prism(run, footprint, high, high.moved(run, offset))
    } else {
        capped_prism(run, footprint, low.moved(run, offset), low)
    }
}

/// The convex polygon *points* less the half-plane where `(p − origin) · normal > 0`
/// (Sutherland–Hodgman).
fn clip(points: &[V3], origin: V3, normal: V3) -> Vec<V3> {
    let side = |p: V3| geom::dot(geom::sub(p, origin), normal);
    let mut out = Vec::new();
    for i in 0..points.len() {
        let (p, q) = (points[i], points[(i + 1) % points.len()]);
        let (sp, sq) = (side(p), side(q));
        if sp <= 0.0 {
            out.push(p);
        }
        if (sp <= 0.0) != (sq <= 0.0) {
            let t = sp / (sp - sq);
            out.push(geom::add(p, geom::scale(geom::sub(q, p), t)));
        }
    }
    out
}

/// The axis-aligned box *bounds*' rectangle across coordinate *axis* (the footprint the box
/// sweeps along it), counter-clockwise or not.
fn rectangle(bounds: &Bounds3, axis: usize) -> [V3; 4] {
    let (a, b) = ((axis + 1) % 3, (axis + 2) % 3);
    let corner = |i: usize, j: usize| {
        let mut p = [0.0; 3];
        p[a] = if i == 0 { bounds[a].0 } else { bounds[a].1 };
        p[b] = if j == 0 { bounds[b].0 } else { bounds[b].1 };
        p
    };
    [corner(0, 0), corner(1, 0), corner(1, 1), corner(0, 1)]
}

/// The probe out of the channel's mouth: the cell over *footprint* moved by *offset* across the
/// run, less its common part with the cell. That common part is the footprints' overlap swept
/// between the higher of the two low caps and the lower of the two high caps; a moved branch
/// and its original cross where their offsets across the axis are equal and opposite, so the
/// overlap is split there. Answered as `(volume, material)`.
fn shifted_probe(
    ctx: &Context<'_>,
    owner: usize,
    run: V3,
    footprint: &[V3],
    caps: (Cap, Cap),
    offset: V3,
) -> Option<(f64, f64)> {
    let (low, high) = caps;
    let moved: Vec<V3> = footprint.iter().map(|&p| geom::add(p, offset)).collect();
    let shifted = capped_prism(run, &moved, low.moved(run, offset), high.moved(run, offset))?;
    // The overlap of two convex footprints: the footprint clipped by each moved edge's line.
    let n = moved.len();
    let centre = geom::scale(
        moved.iter().fold([0.0; 3], |s, &p| geom::add(s, p)),
        1.0 / n as f64,
    );
    let mut overlap: Vec<V3> = footprint.iter().map(|&p| across(run, p)).collect();
    for i in 0..n {
        let (p, q) = (across(run, moved[i]), across(run, moved[(i + 1) % n]));
        let mut outward = geom::cross(geom::sub(q, p), run);
        if geom::dot(outward, geom::sub(across(run, centre), p)) > 0.0 {
            outward = geom::scale(outward, -1.0);
        }
        overlap = clip(&overlap, p, outward);
    }
    let mut pieces = vec![overlap];
    for cap in [low, high] {
        let Cap::Branch { centre, axis, .. } = cap else {
            continue;
        };
        let transverse = geom::unit(geom::cross(axis, run))?;
        let delta = geom::dot(offset, transverse);
        if delta.abs() <= 1e-15 {
            continue;
        }
        let middle = geom::add(centre, geom::scale(transverse, 0.5 * delta));
        pieces = pieces
            .iter()
            .flat_map(|piece| {
                [
                    clip(piece, middle, transverse),
                    clip(piece, middle, geom::scale(transverse, -1.0)),
                ]
            })
            .collect();
    }
    let (mut common_volume, mut common_material) = (0.0, 0.0);
    for piece in pieces.iter().filter(|p| p.len() >= 3) {
        let sum = piece.iter().fold([0.0; 3], |s, &p| geom::add(s, p));
        let inside = geom::scale(sum, 1.0 / piece.len() as f64);
        let pick = |a: Cap, b: Cap, higher: bool| -> Option<Cap> {
            let (ha, hb) = (a.height(run, inside)?, b.height(run, inside)?);
            Some(if (ha >= hb) == higher { a } else { b })
        };
        let lower_cap = pick(low, low.moved(run, offset), true)?;
        let upper_cap = pick(high, high.moved(run, offset), false)?;
        let cell = capped_prism(run, piece, lower_cap, upper_cap)?;
        common_volume += volume(&cell)?;
        common_material += material(ctx, owner, &cell)?;
    }
    Some((
        volume(&shifted)? - common_volume,
        material(ctx, owner, &shifted)? - common_material,
    ))
}

/// The face's box as `(low, high)` per axis (`face.bounding_box()`).
fn bounds_of(part: &Part, face: usize) -> Bounds3 {
    let b = part.face_bounds(face);
    [0, 1, 2].map(|i| (b.min[i], b.max[i]))
}

/// Whether the face lies at *position* along *axis* (`_at`).
fn at(part: &Part, face: usize, axis: usize, position: f64) -> bool {
    let (lo, hi) = bounds_of(part, face)[axis];
    (lo - position).abs().max((hi - position).abs()) <= 1e-6
}

/// A native cylinder's axis point, direction and radius when its face is concave (its outward
/// normal at `position_at(0.37, 0.41)` points at the axis) — the bores both the channel and the
/// passage proofs end on.
pub(crate) fn native_bore(
    ctx: &Context<'_>,
    surfaces: &EffectiveFaces<'_, '_>,
    face: usize,
) -> Option<(V3, V3, f64)> {
    let fact = surfaces.fact(face).as_ref().ok()?;
    if fact.kind() != SurfaceKind::Cylinder || fact.provenance() != SurfaceProvenance::Native {
        return None;
    }
    let p = fact.parameters();
    let (centre, axis, radius) = ([p[0], p[1], p[2]], [p[3], p[4], p[5]], p[6]);
    let (point, _) = ctx.part.position_at(face, 0.37, 0.41)?;
    let delta = geom::sub(point, centre);
    let radial = geom::unit(geom::sub(delta, geom::scale(axis, geom::dot(delta, axis))))?;
    let outward = ctx.part.normal_at_point(face, point)?;
    (geom::dot(outward, radial) <= -1.0 + 1e-6).then_some((centre, axis, radius))
}

/// The lowest and highest the branch rises over the footprint (whose part across the run is
/// read): at its corners, or where it crosses the branch's crest above the axis.
pub(crate) fn branch_extent(cap: &Cap, run: V3, footprint: &[V3]) -> Option<(f64, f64)> {
    let mut heights: Vec<f64> = footprint
        .iter()
        .map(|&p| cap.height(run, p))
        .collect::<Option<_>>()?;
    if let Cap::Branch { centre, axis, .. } = *cap {
        let transverse = geom::unit(geom::cross(axis, run))?;
        let q = |p: V3| geom::dot(geom::sub(p, centre), transverse);
        for i in 0..footprint.len() {
            let (p, r) = (footprint[i], footprint[(i + 1) % footprint.len()]);
            let (qp, qr) = (q(p), q(r));
            if (qp <= 0.0) != (qr <= 0.0) {
                let crest = geom::add(p, geom::scale(geom::sub(r, p), qp / (qp - qr)));
                heights.push(cap.height(run, crest)?);
            }
        }
    }
    let lo = heights.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = heights.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    Some((lo, hi))
}

/// The channel proof (`prove_cylindrical_channel`): `None` for anything outside the contract,
/// including an axis name outside `x`, `y`, `z` (where Python raises and swallows).
pub fn prove(
    ctx: &Context<'_>,
    surfaces: &EffectiveFaces<'_, '_>,
    defining: &BTreeSet<usize>,
    constituent: &BTreeSet<usize>,
    run_axis: &str,
    width_axis: &str,
    open_sign: i32,
) -> Option<CylindricalChannelProof> {
    let part = ctx.part;
    if defining.len() != 2 || !defining.is_subset(constituent) {
        return None;
    }
    let index = |axis: &str| ["x", "y", "z"].iter().position(|&a| a == axis);
    let (r, w) = (index(run_axis)?, index(width_axis)?);
    if r == w || !matches!(open_sign, -1 | 1) {
        return None;
    }
    let d = (0..3).find(|i| *i != r && *i != w)?;
    if defining.iter().any(|&n| !is_planar(part, n)) {
        return None;
    }
    // Walls by the sign of their normal across the width: index 0 for +1, 1 for -1.
    let mut walls: [Option<usize>; 2] = [None, None];
    for &node in defining {
        let normal = normal(part, node)?;
        if normal[w].abs() < 1.0 - 1e-8 {
            return None;
        }
        let slot = &mut walls[usize::from(normal[w] <= 0.0)];
        if slot.is_some() {
            return None;
        }
        *slot = Some(node);
    }
    let walls = [walls[0]?, walls[1]?];
    let open = f64::from(open_sign);
    // Legacy rounded bounds can include an exterior coplanar-facing stock face. Evidence is a
    // candidate hint, not authority to publish that extra face.
    let floors: Vec<usize> = constituent
        .difference(defining)
        .copied()
        .filter(|&node| {
            is_planar(part, node)
                && normal(part, node).is_some_and(|n| n[d] * open >= 1.0 - 1e-8)
                && walls
                    .iter()
                    .all(|&wall| part.arc(node, wall) == Some(Arc::Concave))
        })
        .collect();
    let &[floor] = floors.as_slice() else {
        return None;
    };
    let mut supports = vec![walls[0], walls[1], floor];
    supports.sort_unstable();
    let owner = common_valid_solid(part, &supports)?;
    let wall_bounds = walls.map(|wall| bounds_of(part, wall));
    let mut bounds: Bounds3 = [0, 1, 2].map(|i| {
        (
            wall_bounds[0][i].0.max(wall_bounds[1][i].0),
            wall_bounds[0][i].1.min(wall_bounds[1][i].1),
        )
    });
    let middle = |b: (f64, f64)| (b.0 + b.1) / 2.0;
    bounds[w] = (middle(wall_bounds[0][w]), middle(wall_bounds[1][w]));
    if bounds.iter().any(|(lo, hi)| hi - lo <= 1e-6) {
        return None;
    }
    let (floor_at, mouth) = if open_sign == 1 {
        (bounds[d].0, bounds[d].1)
    } else {
        (bounds[d].1, bounds[d].0)
    };
    if !at(part, floor, d, floor_at)
        || !at(part, walls[0], w, bounds[w].0)
        || !at(part, walls[1], w, bounds[w].1)
    {
        return None;
    }
    let mut contexts: BTreeSet<usize> = part.neighbours(supports[0]).into_iter().collect();
    for &node in &supports[1..] {
        let theirs: BTreeSet<usize> = part.neighbours(node).into_iter().collect();
        contexts = contexts.intersection(&theirs).copied().collect();
    }
    let contexts: Vec<usize> = contexts
        .into_iter()
        .filter(|n| !supports.contains(n))
        .filter(|&n| {
            supports
                .iter()
                .all(|&s| part.arc(s, n) == Some(Arc::Convex))
        })
        .collect();
    let run: V3 = std::array::from_fn(|i| f64::from(u8::from(i == r)));
    let corners: Vec<V3> = (0..8)
        .map(|k| {
            std::array::from_fn(|i| {
                if k >> i & 1 == 0 {
                    bounds[i].0
                } else {
                    bounds[i].1
                }
            })
        })
        .collect();
    for &cylinder_node in &contexts {
        let Some(fact) = surfaces.fact(cylinder_node).as_ref().ok() else {
            continue;
        };
        if fact.kind() != SurfaceKind::Cylinder || fact.provenance() != SurfaceProvenance::Native {
            continue;
        }
        let p = fact.parameters();
        let (centre, axis, radius) = ([p[0], p[1], p[2]], [p[3], p[4], p[5]], p[6]);
        if geom::dot(axis, run).abs() > 1e-8 {
            continue;
        }
        // A zero-area corner tangent can survive the cap/area checks below. The observed end
        // must remain a single-valued cylinder branch strictly inside its domain over the
        // entire (convex rectangular) footprint.
        let Some(transverse) = geom::unit(geom::cross(axis, run)) else {
            continue;
        };
        let max_offset = corners
            .iter()
            .map(|&c| geom::dot(geom::sub(c, centre), transverse).abs())
            .fold(f64::NEG_INFINITY, f64::max);
        if radius - max_offset <= 1e-6 || native_bore(ctx, surfaces, cylinder_node).is_none() {
            continue;
        }
        for &planar in contexts.iter().filter(|&&n| n != cylinder_node) {
            if !is_planar(part, planar) {
                continue;
            }
            let Some(normal) = normal(part, planar) else {
                continue;
            };
            if normal[r].abs() < 1.0 - 1e-8 {
                continue;
            }
            let end = usize::from(normal[r] < 0.0);
            let end_sign = if end == 1 { 1.0 } else { -1.0 };
            let far = middle(bounds_of(part, planar)[r]);
            if !at(part, planar, r, far) || (centre[r] - far) * end_sign <= 1e-6 {
                continue;
            }
            let mut members = supports.clone();
            members.extend([cylinder_node, planar]);
            if common_valid_solid(part, &members) != Some(owner) {
                continue;
            }
            // The cell: the footprint from the far plane to the near branch.
            let branch = Cap::Branch {
                centre,
                axis,
                radius,
                sign: -end_sign,
            };
            let (low, high) = if end == 1 {
                (Cap::Plane(far), branch)
            } else {
                (branch, Cap::Plane(far))
            };
            let footprint = rectangle(&bounds, r);
            let Some((surface_min, surface_max)) = branch_extent(&branch, run, &footprint) else {
                continue;
            };
            if (if end == 1 {
                surface_min - far
            } else {
                far - surface_max
            }) <= 1e-6
            {
                continue;
            }
            let Some(cell) = capped_prism(run, &footprint, low, high) else {
                continue;
            };
            let Some(cell_volume) = volume(&cell).filter(|&v| v > 1e-12) else {
                continue;
            };
            // The side patches off the far and mouth planes: the floor and the two walls.
            let side_patches: Vec<(&Part, usize)> = sides(&cell)
                .into_iter()
                .filter(|&(c, f)| !at(c, f, d, mouth))
                .collect();
            let source: Vec<(&Part, usize)> = supports.iter().map(|&s| (part, s)).collect();
            if side_patches.iter().any(|&f| !covered_patch(f, &source))
                || source
                    .iter()
                    .any(|&f| !covered_along(run, f, &side_patches))
            {
                continue;
            }
            if !empty(ctx, owner, Some(&cell)) {
                continue;
            }
            let thickness = 2e-5f64.max(radius * 1e-4);
            if [-thickness, thickness].iter().any(|&by| {
                let probe = end_probe(run, &footprint, low, high, by);
                probe.as_ref().and_then(volume).is_none_or(|v| v <= 1e-12)
                    || !empty(ctx, owner, probe.as_ref())
            }) {
                continue;
            }
            let lateral: V3 = std::array::from_fn(|i| if i == d { open * thickness } else { 0.0 });
            match shifted_probe(ctx, owner, run, &footprint, (low, high), lateral) {
                Some((v, m)) if v > 1e-12 && m / v <= 1e-9 => {}
                _ => continue,
            }
            let mut centroid: V3 = std::array::from_fn(|i| middle(bounds[i]));
            centroid[r] = 0.0;
            let delta = geom::sub(centroid, centre);
            let k = geom::dot(axis, run);
            let a = 1.0 - k * k;
            let b = 2.0 * (geom::dot(delta, run) - geom::dot(delta, axis) * k);
            let c = geom::dot(delta, delta) - geom::dot(delta, axis).powi(2) - radius * radius;
            let discriminant = b * b - 4.0 * a * c;
            if discriminant <= 0.0 {
                continue;
            }
            let height = (-b - end_sign * discriminant.sqrt()) / (2.0 * a);
            return Some(CylindricalChannelProof {
                supports,
                cylinder: cylinder_node,
                planar_context: planar,
                owner,
                run_axis: run_axis.to_string(),
                width_axis: width_axis.to_string(),
                open_sign,
                bounds,
                run_interval: if end == 1 {
                    (far, height)
                } else {
                    (height, far)
                },
                cylindrical_end: end,
                axis_point: centre,
                axis_direction: axis,
                radius,
                volume: cell_volume,
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unit square swept from the plane at 0 up to a cylinder of radius 2 along x, axis at
    /// height 3: the cell's volume is the integral of `3 − √(4 − y²)` over the square.
    #[test]
    fn capped_prism_measures_a_cell_under_a_branch() {
        let run = [0.0, 0.0, 1.0];
        let footprint = [
            [0.0, -0.5, 0.0],
            [1.0, -0.5, 0.0],
            [1.0, 0.5, 0.0],
            [0.0, 0.5, 0.0],
        ];
        let branch = Cap::Branch {
            centre: [0.0, 0.0, 3.0],
            axis: [1.0, 0.0, 0.0],
            radius: 2.0,
            sign: -1.0,
        };
        let cell = capped_prism(run, &footprint, Cap::Plane(0.0), branch).unwrap();
        // ∫ √(4 − y²) dy over [−½, ½] = y√(4 − y²)/2 + 2 asin(y/2), twice at ½.
        let root = 0.5 * 3.75f64.sqrt() / 2.0 + 2.0 * 0.25f64.asin();
        let expected = 3.0 - 2.0 * root;
        assert!((volume(&cell).unwrap() - expected).abs() < 1e-12);
    }
}
