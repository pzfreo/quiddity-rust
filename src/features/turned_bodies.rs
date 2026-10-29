//! Whole-solid analytic surfaces of revolution, independent of the recognition frame (#123).
use serde::Serialize;

use super::body::BodyKey;
use super::cylindrical_profiles::ProfileAxis;
use super::placed::{Grid, Placed};
use super::turned::{TurnedProfileKey, profile_key_from_bands};
use super::{Context, Features, Occurrence};
use crate::kernel::brep::Part;
use crate::kernel::geom::{self, COORD_FLOOR, Curve, Surface, V3};

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Bore {
    pub diameter: f64,
    /// Signed distances from `axis_point`, along `axis`.
    pub span: [f64; 2],
}

/// A meridian segment, ordered by its axial endpoints. Coordinates are [axial distance
/// from axis_point, radius]. Circular arcs carry their centre, radius and signed sweep
/// in this plane (positive from +axial towards +radius), so even reentrant tori are exact.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ProfileSegment {
    pub kind: &'static str,
    pub side: &'static str,
    pub start: [f64; 2],
    pub end: [f64; 2],
    pub arc_center: Option<[f64; 2]>,
    pub arc_radius: Option<f64>,
    pub arc_sweep: Option<f64>,
    pub face: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TurnedBody {
    pub axis: ProfileAxis,
    /// Midpoint of the solid's axial extent, on its common axis.
    pub axis_point: Placed<V3>,
    pub od: f64,
    pub length: f64,
    pub bores: Vec<Bore>,
    pub profile: Vec<ProfileSegment>,
    pub body_key: Option<BodyKey>,
    /// Existing turned-step/groove join, when principal-axis cylinder bands exist.
    pub profile_key: Option<TurnedProfileKey>,
}

pub(super) fn tolerance(part: &Part) -> f64 {
    part.file_uncertainty()
        .unwrap_or(COORD_FLOOR)
        .max(COORD_FLOOR)
}

pub(crate) fn canonical(direction: V3) -> V3 {
    let i = geom::dominant_axis(direction.map(f64::abs));
    geom::scale(direction, if direction[i] < 0. { -1. } else { 1. })
}

pub(crate) fn axis(direction: V3) -> ProfileAxis {
    // Only numerical roundoff may become a principal letter; no frame eligibility gate.
    let i = geom::dominant_axis(direction.map(f64::abs));
    if (0..3).all(|j| j == i || direction[j].abs() <= 1e-12) {
        ProfileAxis::Principal(['x', 'y', 'z'][i])
    } else {
        ProfileAxis::Direction(direction)
    }
}

pub(crate) fn direction(axis: ProfileAxis) -> V3 {
    match axis {
        ProfileAxis::Principal(c) => {
            let mut d = [0.; 3];
            d[match c {
                'x' => 0,
                'y' => 1,
                _ => 2,
            }] = 1.;
            d
        }
        ProfileAxis::Direction(d) => d,
    }
}

fn radial(p: V3, origin: V3, d: V3) -> f64 {
    let delta = geom::sub(p, origin);
    geom::norm(geom::sub(delta, geom::scale(d, geom::dot(delta, d))))
}

/// Positive rational B-spline bases form a convex combination of their poles.
/// A complete control net inside a precision-wide plane slab proves that whole
/// support planar; a sampled canonical fit alone is deliberately insufficient.
fn support(part: &Part, face: usize) -> Option<Surface> {
    match &part.faces[face].surface {
        Surface::Freeform { surface, .. } => {
            if !surface
                .weights
                .iter()
                .flatten()
                .all(|w| w.is_finite() && *w > 0.)
            {
                return None;
            }
            let points: Vec<_> = surface.control_points.iter().flatten().copied().collect();
            if points.len() < 3 {
                return None;
            }
            let origin = [0, 1, 2].map(|i| {
                crate::kernel::py::fsum(points.iter().map(|p| p[i])) / points.len() as f64
            });
            let x = points
                .iter()
                .map(|p| geom::sub(*p, origin))
                .max_by(|a, b| geom::norm(*a).total_cmp(&geom::norm(*b)))
                .and_then(geom::unit)?;
            let z = points
                .iter()
                .map(|p| geom::cross(x, geom::sub(*p, origin)))
                .max_by(|a, b| geom::norm(*a).total_cmp(&geom::norm(*b)))
                .and_then(geom::unit)?;
            if !points
                .iter()
                .all(|p| geom::dot(geom::sub(*p, origin), z).abs() <= tolerance(part))
            {
                return None;
            }
            Some(Surface::Plane {
                frame: geom::Frame {
                    origin,
                    x,
                    y: geom::cross(z, x),
                    z,
                },
            })
        }
        Surface::Other { .. } => None,
        surface => Some(surface.clone()),
    }
}

/// Prove one common line. Different revolution axes on the same solid are refused.
/// Spheres constrain the line by their centres, not their arbitrary parameter polar axes.
fn common_axis(part: &Part, faces: &[usize]) -> Option<(V3, V3)> {
    let mut line = None;
    for &f in faces {
        match support(part, f)? {
            Surface::Cylinder { frame, .. }
            | Surface::Cone { frame, .. }
            | Surface::Torus { frame, .. } => {
                line = Some((frame.origin, canonical(geom::unit(frame.z)?)));
                break;
            }
            _ => {}
        }
    }
    let (origin, d) = line.or_else(|| {
        // A sphere or spherical lens: plane normals or two sphere centres determine
        // a line. A complete single sphere admits any axis through its centre.
        let centres: Vec<_> = faces
            .iter()
            .filter_map(|&f| match support(part, f)? {
                Surface::Sphere { frame, .. } => Some(frame.origin),
                _ => None,
            })
            .collect();
        let origin = *centres.first()?;
        let normal = faces.iter().find_map(|&f| match support(part, f)? {
            Surface::Plane { frame } => geom::unit(frame.z),
            _ => None,
        });
        let d = normal
            .or_else(|| {
                centres.iter().find_map(|&c| {
                    (geom::dist(c, origin) > tolerance(part))
                        .then(|| geom::unit(geom::sub(c, origin)))
                        .flatten()
                })
            })
            .unwrap_or([0., 0., 1.]);
        Some((origin, canonical(d)))
    })?;
    let tol = tolerance(part);
    let mut supports = Vec::new();
    let mut size = COORD_FLOOR;
    for &f in faces {
        let (frame, revolved, radius) = match support(part, f)? {
            Surface::Plane { frame } => (frame, false, 0.),
            Surface::Cylinder { frame, radius }
            | Surface::Cone { frame, radius, .. }
            | Surface::Sphere { frame, radius } => (frame, true, radius),
            Surface::Torus {
                frame,
                major,
                minor,
            } => (frame, true, major + minor),
            _ => return None,
        };
        let angle = if matches!(support(part, f)?, Surface::Sphere { .. }) {
            0.
        } else {
            geom::norm(geom::cross(geom::unit(frame.z)?, d))
        };
        if revolved && radial(frame.origin, origin, d) > tol {
            return None;
        }
        size = size.max(radius);
        supports.push(angle);
    }
    if supports.iter().any(|angle| angle * size > tol) {
        return None;
    }
    // Only plausible coaxial candidates need the exact directional extent calculation.
    let x = geom::default_x_axis(d);
    let y = geom::cross(d, x);
    size = [d, x, y]
        .into_iter()
        .map(|v| {
            let (lo, hi) = part.extent_along(faces, v);
            hi - lo
        })
        .fold(size, f64::max);
    if supports.iter().any(|angle| angle * size > tol) {
        return None;
    }
    Some((origin, d))
}

pub(crate) fn sort_profile(profile: &mut [ProfileSegment]) {
    profile.sort_by(|a, b| {
        a.start[0]
            .total_cmp(&b.start[0])
            .then(a.end[0].total_cmp(&b.end[0]))
            .then(a.start[1].total_cmp(&b.start[1]))
            .then(a.end[1].total_cmp(&b.end[1]))
            .then(a.face.cmp(&b.face))
    });
}

fn profile(part: &Part, faces: &[usize], origin: V3, d: V3) -> Option<Vec<ProfileSegment>> {
    let mut out = Vec::new();
    let point = |p| [geom::dot(geom::sub(p, origin), d), radial(p, origin, d)];
    for &face in faces {
        let f = &part.faces[face];
        let support = support(part, face)?;
        let mut seg = ProfileSegment {
            kind: match support {
                Surface::Cylinder { .. } => "cylinder",
                Surface::Cone { .. } => "cone",
                Surface::Sphere { .. } => "sphere",
                Surface::Torus { .. } => "torus",
                _ => "plane_shoulder",
            },
            side: "external",
            start: [0.; 2],
            end: [0.; 2],
            arc_center: None,
            arc_radius: None,
            arc_sweep: None,
            face,
        };
        let (u0, u1, v0, v1) = part.uv_bounds(face)?;
        let u = (u0 + u1) / 2.;
        let v = (v0 + v1) / 2.;
        let p = f.surface.value(u, v);
        let n = part.face_normal(face, u, v)?;
        let delta = geom::sub(p, origin);
        let r = geom::sub(delta, geom::scale(d, geom::dot(delta, d)));
        if geom::dot(n, r) < 0. {
            seg.side = "internal";
        }
        match support {
            Surface::Plane { .. } => {
                seg.kind = "plane_shoulder";
                seg.side = "shoulder";
                let mut radii: Vec<_> = part
                    .face_edges(face)
                    .into_iter()
                    .filter_map(|e| match part.edges[e].curve {
                        Curve::Circle { radius, .. } => Some(radius),
                        _ => None,
                    })
                    .collect();
                radii.extend(
                    f.loops
                        .iter()
                        .filter_map(|lp| lp.vertex.map(|p| radial(p, origin, d))),
                );
                radii.sort_by(f64::total_cmp);
                let high = *radii.last()?;
                let axis_on_plane = geom::add(origin, geom::scale(d, point(p)[0]));
                let at_axis = f
                    .surface
                    .parameters(axis_on_plane, None)
                    .is_some_and(|(u, v)| {
                        geom::dist(f.surface.value(u, v), axis_on_plane) <= tolerance(part)
                            && part
                                .domain(face)
                                .is_some_and(|domain| domain.contains(u, v))
                    });
                let low = if at_axis { 0. } else { *radii.first()? };
                seg.start = [point(p)[0], low];
                seg.end = [point(p)[0], high];
            }
            Surface::Sphere { frame, radius } => {
                let (lo, hi) = part.extent_along(&[face], d);
                let c = point(frame.origin)[0];
                let at = |s: f64| [s, (radius * radius - (s - c).powi(2)).max(0.).sqrt()];
                let shift = geom::dot(origin, d);
                seg.start = at(lo - shift);
                seg.end = at(hi - shift);
                seg.arc_center = Some([c, 0.]);
                seg.arc_radius = Some(radius);
                seg.arc_sweep =
                    Some(seg.end[1].atan2(seg.end[0] - c) - seg.start[1].atan2(seg.start[0] - c));
            }
            Surface::Torus {
                frame,
                major,
                minor,
            } => {
                seg.start = point(f.surface.value(u, v0));
                seg.end = point(f.surface.value(u, v1));
                seg.arc_center = Some([point(frame.origin)[0], major]);
                seg.arc_radius = Some(minor);
                seg.arc_sweep = Some(-geom::dot(frame.z, d).signum() * (v1 - v0));
            }
            Surface::Cylinder { .. } | Surface::Cone { .. } => {
                seg.start = point(f.surface.value(u, v0));
                seg.end = point(f.surface.value(u, v1));
            }
            _ => return None,
        }
        if seg.start[0] > seg.end[0] {
            std::mem::swap(&mut seg.start, &mut seg.end);
            seg.arc_sweep = seg.arc_sweep.map(|v| -v);
        }
        out.push(seg);
    }
    sort_profile(&mut out);
    Some(out)
}

internal_entry! {
/// One record per qualifying valid solid, co-owning its complete boundary.
pub fn discover(ctx: &Context<'_>) -> Vec<Occurrence<TurnedBody>> {
    let part = ctx.part;
    let keys = ctx.body_keys(true);
    let mut out = Vec::new();
    for (s, solid) in part.solids.iter().enumerate() {
        if !part.solid_is_geometrically_valid(s) { continue; }
        let Some((origin, d)) = common_axis(part, &solid.faces) else { continue };
        let (lo, hi) = part.extent_along(&solid.faces, d);
        let mid = (lo + hi) / 2.;
        let origin = geom::add(origin, geom::scale(d, mid - geom::dot(origin, d)));
        let Some(profile) = profile(part, &solid.faces, origin, d) else { continue };
        let mut bores: Vec<Bore> = profile.iter().filter(|p| p.kind == "cylinder" && p.side == "internal")
            .map(|p| Bore { diameter: 2. * p.start[1], span: [p.start[0], p.end[0]] }).collect();
        bores.sort_by(|a, b| a.diameter.total_cmp(&b.diameter).then(a.span[0].total_cmp(&b.span[0])));
        let mut merged: Vec<Bore> = Vec::new();
        for b in bores {
            if let Some(last) = merged.last_mut()
                && (last.diameter - b.diameter).abs() <= tolerance(part)
                && b.span[0] <= last.span[1] + tolerance(part) {
                last.span[1] = last.span[1].max(b.span[1]);
            } else { merged.push(b); }
        }
        // A full revolution's transverse extent is its diameter, including torus/sphere
        // interior extrema; do not sample a meridian or use the world bounding box.
        let transverse = geom::unit(geom::cross(d, geom::default_x_axis(d))).unwrap();
        let (rlo, rhi) = part.extent_along(&solid.faces, transverse);
        let bands: Vec<_> = ctx.cylinders().iter().filter(|c| c.solid == s && c.external).collect();
        let profile_key = match axis(d) {
            ProfileAxis::Principal(c) if !bands.is_empty() => Some(profile_key_from_bands(
                match c { 'x' => 0, 'y' => 1, _ => 2 }, &bands, &part.solid_bounds(s), keys[s].clone(),
            )),
            _ => None,
        };
        out.push(Occurrence {
            record: TurnedBody { axis: axis(d), axis_point: Placed::new(origin, Grid::Decimals(8)),
                od: rhi - rlo, length: hi - lo, bores: merged, profile,
                body_key: keys[s].clone(), profile_key },
            defining: solid.faces.clone(), context: Vec::new(),
        });
    }
    out
}
}

/// Only suppress an accepted OD boss on a classified body: its defining faces are body
/// cylinders, its raw diameter equals OD within model precision, and its axial interval
/// covers at least 90% of the body, leaving at most 5% at either end (plus model precision).
/// Run after every existing projection, so no other feature decision changes.
pub(super) fn suppress_od_bosses(part: &Part, f: &mut Features) {
    if f.turned_bodies.is_empty() {
        return;
    }
    let Some(defining) = f.defining.get("bosses") else {
        return;
    };
    let gone: std::collections::BTreeSet<_> = defining.iter().enumerate().filter_map(|(i, faces)| {
        f.turned_bodies.iter().zip(&f.defining["turned_bodies"]).any(|(body, owned)| {
            let d = direction(body.axis);
            let tol = tolerance(part);
            if faces.is_empty() || !faces.iter().all(|face| owned.contains(face)
                && matches!(part.faces[*face].surface, Surface::Cylinder { radius, .. } if (2. * radius - body.od).abs() <= tol)) {
                return false;
            }
            let (lo, hi) = part.extent_along(faces, d);
            let mid = geom::dot(*body.axis_point, d);
            hi - lo + tol >= 0.9 * body.length
                && lo - (mid - body.length / 2.) <= 0.05 * body.length + tol
                && (mid + body.length / 2.) - hi <= 0.05 * body.length + tol
        }).then_some(i)
    }).collect();
    super::drop_rejected(&mut f.bosses, &gone);
    super::drop_rejected(f.defining.get_mut("bosses").unwrap(), &gone);
    if let Some(status) = f.evidence_status.get_mut("bosses") {
        super::drop_rejected(status, &gone);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn supports_must_agree_within_model_precision_not_a_frame_angle_gate() {
        let p = crate::read_step_file(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/turned_body_disc.step"),
        )
        .unwrap();
        let tol = tolerance(&p);
        for (offset, accepted) in [(0.5 * tol, true), (2. * tol, false)] {
            let mut faces = p.faces.clone();
            let Surface::Cylinder { ref mut frame, .. } = faces[4].surface else {
                panic!()
            };
            frame.origin[0] += offset;
            let q = Part::new(faces, p.edges.clone(), p.solids.clone());
            assert_eq!(common_axis(&q, &q.solids[0].faces).is_some(), accepted);
        }
        for (angle, accepted) in [(tol / 44., true), (tol / 11., false)] {
            let mut faces = p.faces.clone();
            let Surface::Plane { ref mut frame } = faces[3].surface else {
                panic!()
            };
            frame.z = [angle.sin(), 0., angle.cos()];
            let q = Part::new(faces, p.edges.clone(), p.solids.clone());
            assert_eq!(common_axis(&q, &q.solids[0].faces).is_some(), accepted);
        }
        let mut faces = p.faces.clone();
        let Surface::Cylinder { ref mut frame, .. } = faces[4].surface else {
            panic!()
        };
        frame.z = [0., 1., 0.];
        let q = Part::new(faces, p.edges.clone(), p.solids.clone());
        assert!(common_axis(&q, &q.solids[0].faces).is_none());
    }
}
