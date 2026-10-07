//! A face in its surface's parameter space: boundary loops unwrapped across periodic
//! parameters (with singular points routed along their parameter line), OpenCascade-compatible
//! parameter ranges, and point containment.

use std::f64::consts::TAU;

use super::brep::Part;
use super::geom::{self, Surface, V3};
use super::py;

/// The face's boundary as (u, v) polylines.
///
/// Containment is decided by crossing parity, which needs no loop orientation. That matters:
/// OpenCascade re-derives wire orientation from geometry when it reads a file, and the files it
/// writes carry `FACE_BOUND` flags its own reader then corrects, so the flags are not evidence.
/// Parity does need every loop closed in parameter space; [`crate::brep::Part::uv_loops`]
/// closes a loop that runs round a sphere through its pole.
#[derive(Debug)]
pub struct FaceDomain {
    loops: Vec<Vec<(f64, f64)>>,
    periodic: (bool, bool),
    u_range: (f64, f64),
    v_range: (f64, f64),
}

fn range<'a>(
    points: impl Iterator<Item = &'a (f64, f64)>,
    pick: impl Fn(&(f64, f64)) -> f64,
) -> (f64, f64) {
    points
        .map(pick)
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| {
            (lo.min(x), hi.max(x))
        })
}

impl FaceDomain {
    pub fn new(loops: &[UvLoop], periodic: (bool, bool)) -> Self {
        let all = || loops.iter().flat_map(|l| l.points.iter());
        FaceDomain {
            u_range: range(all(), |p| p.0),
            v_range: range(all(), |p| p.1),
            loops: loops.iter().map(|l| l.points.clone()).collect(),
            periodic,
        }
    }

    pub fn u_range(&self) -> (f64, f64) {
        self.u_range
    }

    pub fn v_range(&self) -> (f64, f64) {
        self.v_range
    }

    /// Whether (u, v) lies on the face, by the parity of a parameter-space ray.
    ///
    /// The ray runs in +v. A periodic v needs a finite end outside the face's v range; a
    /// periodic u wraps every segment next to the point. A face closed in v but open in u
    /// casts in +u instead, and a face closed in both is the whole surface.
    pub fn contains(&self, u: f64, v: f64) -> bool {
        let (pu, pv) = self.periodic;
        let (vmin, vmax) = self.v_range;
        let (umin, umax) = self.u_range;
        if pv && vmax - vmin >= TAU - 1e-9 {
            if !pu || umax - umin >= TAU - 1e-9 {
                return true;
            }
            let u = umin + (u - umin).rem_euclid(TAU);
            if u > umax {
                return false;
            }
            return crossing_parity(
                &self.loops,
                v,
                u,
                true,
                true,
                umax + 0.5 * (TAU - (umax - umin)),
            );
        }
        let mut v = v;
        let mut v_end = f64::INFINITY;
        if pv {
            v = vmin + (v - vmin).rem_euclid(TAU);
            if v > vmax {
                return false;
            }
            v_end = vmax + 0.5 * (TAU - (vmax - vmin));
        }
        let u = if pu { u.rem_euclid(TAU) } else { u };
        crossing_parity(&self.loops, u, v, pu, false, v_end)
    }
}

/// Parity of the crossings of the parameter-space ray `(a, b) → (a, b_end)` with the loops,
/// where `a` is each loop point's first coordinate (its second when `swap`). `periodic_a` wraps
/// each segment next to `a`.
fn crossing_parity(
    loops: &[Vec<(f64, f64)>],
    a: f64,
    b: f64,
    periodic_a: bool,
    swap: bool,
    b_end: f64,
) -> bool {
    let pick = |p: &(f64, f64)| if swap { (p.1, p.0) } else { (p.0, p.1) };
    let mut count = 0usize;
    for lp in loops {
        for w in lp.windows(2) {
            let (mut a0, b0) = pick(&w[0]);
            let (mut a1, b1) = pick(&w[1]);
            if periodic_a {
                let shift = geom::nearest_turn(a0, a) - a0;
                a0 += shift;
                a1 += shift;
            }
            if (a0 > a) == (a1 > a) {
                continue;
            }
            let bc = b0 + (a - a0) / (a1 - a0) * (b1 - b0);
            if bc > b && bc < b_end {
                count += 1;
            }
        }
    }
    count % 2 == 1
}

/// One loop of a face in its surface's (u, v) space, unwrapped across periodic parameters.
#[derive(Clone, Debug)]
pub struct UvLoop {
    pub points: Vec<(f64, f64)>,
}

impl Part {
    /// The face's outer loop (`BRepTools::OuterWire`): the only loop, or the one whose
    /// parameter-space box is largest.
    pub fn outer_loop(&self, face: usize) -> Option<usize> {
        let loops = self.uv_loops(face)?;
        let area = |lp: &UvLoop| {
            let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
            for &(u, v) in &lp.points {
                (lo[0], lo[1], hi[0], hi[1]) =
                    (lo[0].min(u), lo[1].min(v), hi[0].max(u), hi[1].max(v));
            }
            (hi[0] - lo[0]) * (hi[1] - lo[1])
        };
        (!loops.is_empty()).then(|| py::first_max(loops, area))
    }

    /// build123d's `Face.center()`: the area centroid of a planar face, otherwise the surface
    /// point at the middle of the face's parameter range.
    pub fn face_centre(&self, face: usize) -> Option<V3> {
        let f = &self.faces[face];
        let Surface::Plane { frame } = f.surface else {
            let (u0, u1, v0, v1) = self.uv_bounds(face)?;
            return Some(f.surface.value(0.5 * (u0 + u1), 0.5 * (v0 + v1)));
        };
        // Shoelace over each loop in plane coordinates; the largest loop is the outer
        // boundary and the rest are holes in it (loop orientation is not relied on).
        let mut loops: Vec<(f64, f64, f64)> = self
            .uv_loops(face)?
            .iter()
            .map(|lp| {
                let (mut a, mut cx, mut cy) = (0.0, 0.0, 0.0);
                for w in lp.points.windows(2) {
                    let ((x0, y0), (x1, y1)) = (w[0], w[1]);
                    let k = x0 * y1 - x1 * y0;
                    a += k;
                    cx += (x0 + x1) * k;
                    cy += (y0 + y1) * k;
                }
                (a.abs() / 2.0, cx / (3.0 * a), cy / (3.0 * a))
            })
            .collect();
        loops.sort_by(|a, b| b.0.total_cmp(&a.0));
        let (outer, holes) = loops.split_first()?;
        let area = outer.0 - holes.iter().map(|h| h.0).sum::<f64>();
        let cx = (outer.0 * outer.1 - holes.iter().map(|h| h.0 * h.1).sum::<f64>()) / area;
        let cy = (outer.0 * outer.2 - holes.iter().map(|h| h.0 * h.2).sum::<f64>()) / area;
        Some(frame.to_world([cx, cy, 0.0]))
    }

    /// The face's trimmed region in parameter space.
    pub fn domain(&self, face: usize) -> Option<&FaceDomain> {
        self.cache[face]
            .domain
            .get_or_init(|| {
                let f = &self.faces[face];
                Some(FaceDomain::new(self.uv_loops(face)?, f.surface.periodic()))
            })
            .as_ref()
    }

    /// Each loop as a closed polyline in (u, v), with periodic parameters unwrapped so each
    /// polyline is continuous. A loop around a periodic direction ends one period away from where
    /// it began.
    pub fn uv_loops(&self, face: usize) -> Option<&[UvLoop]> {
        self.cache[face]
            .uv_loops
            .get_or_init(|| self.compute_uv_loops(face))
            .as_deref()
    }

    fn compute_uv_loops(&self, face: usize) -> Option<Vec<UvLoop>> {
        let f = &self.faces[face];
        let (pu, pv) = f.surface.periodic();
        let mut loops = Vec::new();
        for lp in &f.loops {
            let mut pts: Vec<(f64, f64)> = Vec::new();
            if let Some(apex) = lp.vertex {
                // A degenerate loop: the whole periodic u line at the apex's v.
                let (_, v) = f.surface.parameters(apex, None)?;
                if pu {
                    loops.push(UvLoop {
                        points: periodic_line(0.0, TAU, v),
                    });
                }
                continue;
            }
            let mut raw: Vec<(f64, f64)> = Vec::new();
            for &(e, forward) in &lp.edges {
                let samples = &self.edges[e].samples;
                let ordered: Box<dyn Iterator<Item = &V3>> = if forward {
                    Box::new(samples.iter())
                } else {
                    Box::new(samples.iter().rev())
                };
                for p in ordered {
                    let hint = raw.last().copied();
                    raw.push(f.surface.parameters(*p, hint)?);
                }
            }
            let raw = route_singular_points(&f.surface, raw, f.reversed);
            for (mut u, mut v) in raw {
                if let Some(&(lu, lv)) = pts.last() {
                    if pu {
                        u = geom::nearest_turn(u, lu);
                    }
                    if pv {
                        v = geom::nearest_turn(v, lv);
                    }
                }
                pts.push((u, v));
            }
            close_through_pole(&f.surface, &mut pts, f.reversed);
            loops.push(UvLoop { points: pts });
        }
        Some(loops)
    }

    /// `BRepTools::UVBounds` — (u_first, u_last, v_first, v_last) of the face's trimmed range.
    ///
    /// A periodic direction the face closes around (a seam edge, or a loop that winds once
    /// round) spans exactly one period starting at the seam.
    pub fn uv_bounds(&self, face: usize) -> Option<(f64, f64, f64, f64)> {
        let f = &self.faces[face];
        let (pu, pv) = f.surface.periodic();
        let loops = self.uv_loops(face)?;
        let mut range = [
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ];
        // Unwrap every loop next to the first so separate loops share one period.
        let reference = loops.first().and_then(|l| l.points.first()).copied()?;
        let mut winds = (false, false);
        for lp in loops {
            let lp = &lp.points;
            let (Some(first), Some(last)) = (lp.first(), lp.last()) else {
                continue;
            };
            let shift_u = if pu {
                geom::nearest_turn(first.0, reference.0) - first.0
            } else {
                0.0
            };
            let shift_v = if pv {
                geom::nearest_turn(first.1, reference.1) - first.1
            } else {
                0.0
            };
            winds.0 |= pu && (last.0 - first.0).abs() > 1.0;
            winds.1 |= pv && (last.1 - first.1).abs() > 1.0;
            for &(u, v) in lp {
                range[0] = range[0].min(u + shift_u);
                range[1] = range[1].max(u + shift_u);
                range[2] = range[2].min(v + shift_v);
                range[3] = range[3].max(v + shift_v);
            }
        }
        // OpenCascade's range covers each B-spline pcurve's control polygon, which can overshoot
        // the curve itself. Align each polygon to the loops' period before taking it in.
        let centre = (0.5 * (range[0] + range[1]), 0.5 * (range[2] + range[3]));
        for poles in &f.pcurve_poles {
            let (mut lo_u, mut hi_u, mut lo_v, mut hi_v) = (
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
            );
            for &(u, v) in poles {
                (lo_u, hi_u, lo_v, hi_v) = (lo_u.min(u), hi_u.max(u), lo_v.min(v), hi_v.max(v));
            }
            let mid = (0.5 * (lo_u + hi_u), 0.5 * (lo_v + hi_v));
            let su = if pu {
                geom::nearest_turn(mid.0, centre.0) - mid.0
            } else {
                0.0
            };
            let sv = if pv {
                geom::nearest_turn(mid.1, centre.1) - mid.1
            } else {
                0.0
            };
            range[0] = range[0].min(lo_u + su);
            range[1] = range[1].max(hi_u + su);
            range[2] = range[2].min(lo_v + sv);
            range[3] = range[3].max(hi_v + sv);
        }
        let seam = self.seam_parameters(face);
        for (periodic, wound, lo, hi, seam_value) in
            [(pu, winds.0, 0, 1, seam.0), (pv, winds.1, 2, 3, seam.1)]
        {
            if !periodic {
                continue;
            }
            if wound || seam_value.is_some() || range[hi] - range[lo] > TAU - 1e-9 {
                let start = seam_value.unwrap_or(0.0);
                range[lo] = start;
                range[hi] = start + TAU;
            }
        }
        Some((range[0], range[1], range[2], range[3]))
    }

    /// The constant periodic parameter of a seam edge (an edge used twice by this face), if
    /// any, normalised to `[0, 2π)` — the side OpenCascade starts its parameter range from.
    fn seam_parameters(&self, face: usize) -> (Option<f64>, Option<f64>) {
        let f = &self.faces[face];
        let mut seen = std::collections::BTreeMap::new();
        for lp in &f.loops {
            for &(e, _) in &lp.edges {
                *seen.entry(e).or_insert(0) += 1;
            }
        }
        let mut out = (None, None);
        for (&e, &count) in &seen {
            if count < 2 {
                continue;
            }
            let samples = &self.edges[e].samples;
            let Some((u0, v0)) = f.surface.parameters(samples[0], None) else {
                continue;
            };
            let mid = samples[samples.len() / 2];
            let Some((u1, v1)) = f.surface.parameters(mid, None) else {
                continue;
            };
            if (geom::nearest_turn(u1, u0) - u0).abs() < 1e-7 {
                out.0 = Some(round_seam(u0));
            } else if (geom::nearest_turn(v1, v0) - v0).abs() < 1e-7 {
                out.1 = Some(round_seam(v0));
            }
        }
        out
    }
}

/// Replace each run of singular samples (where u is undefined) by the singular v line from the
/// u the boundary arrives at to the u it leaves at.
///
/// Which way round is a question only orientation answers — a hemisphere bounded by one
/// meridian circle arrives and leaves half a turn apart — so the face's side decides it: STEP
/// keeps the face on a loop's left, which in parameter space is the -u side of a boundary
/// rising to the top line (+u for a reversed face), mirrored at the bottom.
fn route_singular_points(
    surface: &Surface,
    raw: Vec<(f64, f64)>,
    reversed: bool,
) -> Vec<(f64, f64)> {
    let n = raw.len();
    let singular: Vec<bool> = raw
        .iter()
        .map(|&(_, v)| surface.singular_v(v).is_some())
        .collect();
    if !singular.iter().any(|s| *s) || singular.iter().all(|s| *s) {
        return raw;
    }
    // Start on a regular sample so that no singular run straddles the loop's ends.
    let first_regular = singular.iter().position(|s| !s).expect("a regular sample");
    let raw: Vec<(f64, f64)> = raw[first_regular..]
        .iter()
        .chain(&raw[..first_regular])
        .copied()
        .collect();
    let singular: Vec<bool> = singular[first_regular..]
        .iter()
        .chain(&singular[..first_regular])
        .copied()
        .collect();
    let mut out = Vec::with_capacity(n + 64);
    let mut i = 0;
    while i < n {
        if !singular[i] {
            out.push(raw[i]);
            i += 1;
            continue;
        }
        let start = i;
        while i < n && singular[i] {
            i += 1;
        }
        // Neighbours on either side, wrapping round the closed loop.
        let before = (0..n)
            .map(|k| (start + n - 1 - k) % n)
            .find(|&k| !singular[k])
            .expect("a regular sample");
        let after = (0..n)
            .map(|k| (i + k) % n)
            .find(|&k| !singular[k])
            .expect("a regular sample");
        let v = surface.singular_v(raw[start].1).expect("a singular sample");
        let (ua, ub) = (raw[before].0, raw[after].0);
        let mut ub = geom::nearest_turn(ub, ua);
        if (ub - ua).abs() > 1e-6 {
            // Rising to the top line (v above the boundary's) runs -u for a forward face.
            let rising = v > raw[before].1;
            let toward = if rising != reversed { -1.0 } else { 1.0 };
            if (ub - ua) * toward < 0.0 {
                ub += toward * TAU;
            }
        }
        out.extend(periodic_line(ua, ub, v));
    }
    // Close explicitly: the loop's first sample is regular, so it is where the walk returns.
    if out.last() != out.first() {
        out.push(out[0]);
    }
    out
}

/// Whether the face reaches the singular point (pole or apex) at *v*: the point is a whole
/// parameter line, which lies on the face's boundary rather than strictly inside it, so look
/// just off the line instead.
pub(super) fn touches_singular_point(surface: &Surface, domain: &FaceDomain, v: f64) -> bool {
    let Some(line) = surface.singular_v(v) else {
        return false;
    };
    let off = line - line.signum() * 1e-4;
    (0..64).any(|k| domain.contains(TAU * k as f64 / 64.0, off))
}

/// A loop that runs once round a sphere ends one turn from where it began; close it along a
/// pole's parameter line so that it bounds a region. The pole is the one it reaches along a
/// seam if any; otherwise the one on the face's side of it (the left of a loop running +u on a
/// forward face is +v, the north pole).
fn close_through_pole(surface: &Surface, pts: &mut Vec<(f64, f64)>, reversed: bool) {
    if !matches!(surface, Surface::Sphere { .. }) || pts.len() < 2 {
        return;
    }
    let (first, last) = (pts[0], pts[pts.len() - 1]);
    if (last.0 - first.0).abs() < std::f64::consts::PI {
        return;
    }
    let half = std::f64::consts::FRAC_PI_2;
    let touched = pts
        .iter()
        .map(|p| p.1)
        .find(|v| (v.abs() - half).abs() < 1e-6);
    let pole = match touched {
        Some(v) => v.signum() * half,
        None if (last.0 > first.0) != reversed => half,
        None => -half,
    };
    pts.extend(periodic_line(last.0, first.0, pole));
    pts.push(first);
}

/// Points along a constant-v line from u `a` to `b`, short enough that no segment spans half a
/// period — containment wraps each segment to the nearest turn, which a long one would defeat.
fn periodic_line(a: f64, b: f64, v: f64) -> Vec<(f64, f64)> {
    let n = 64;
    (0..=n)
        .map(|i| (a + (b - a) * i as f64 / n as f64, v))
        .collect()
}

/// A seam at 2π is the same seam as one at 0; OpenCascade starts the range at 0.
fn round_seam(value: f64) -> f64 {
    if (value - TAU).abs() < 1e-9 || value.abs() < 1e-9 {
        0.0
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn domain(loops: Vec<Vec<(f64, f64)>>, periodic: (bool, bool)) -> FaceDomain {
        let loops: Vec<UvLoop> = loops.into_iter().map(|points| UvLoop { points }).collect();
        FaceDomain::new(&loops, periodic)
    }

    fn square(lo: f64, hi: f64) -> Vec<(f64, f64)> {
        vec![(lo, lo), (hi, lo), (hi, hi), (lo, hi), (lo, lo)]
    }

    #[test]
    fn annulus_excludes_its_hole() {
        let d = domain(vec![square(0.0, 10.0), square(4.0, 6.0)], (false, false));
        assert!(d.contains(2.0, 2.0));
        assert!(!d.contains(5.0, 5.0));
        assert!(!d.contains(11.0, 5.0));
    }

    #[test]
    fn cylinder_band_between_two_circles() {
        // Two separate loops round a periodic u, as files without seam edges give them.
        let ring = |v: f64| periodic_line(0.0, TAU, v);
        let d = domain(vec![ring(0.0), ring(5.0)], (true, false));
        assert!(d.contains(1.0, 2.0) && d.contains(6.0, 2.0) && d.contains(-1.0, 2.0));
        assert!(!d.contains(1.0, -1.0) && !d.contains(1.0, 6.0));
    }

    #[test]
    fn torus_band_partial_in_v() {
        // A seam rectangle [0, 2π] × [0, π/2], as OpenCascade writes a turned fillet.
        let h = std::f64::consts::FRAC_PI_2;
        let mut lp = periodic_line(0.0, TAU, 0.0);
        lp.extend([(TAU, h)]);
        lp.extend(periodic_line(TAU, 0.0, h));
        lp.push((0.0, 0.0));
        let d = domain(vec![lp], (true, true));
        assert!(d.contains(3.0, 0.7));
        assert!(!d.contains(3.0, 2.0) && !d.contains(3.0, -0.5));
    }

    #[test]
    fn hemisphere_closed_through_its_pole() {
        let frame = crate::kernel::geom::Frame {
            origin: [0.0; 3],
            x: [1.0, 0.0, 0.0],
            y: [0.0, 1.0, 0.0],
            z: [0.0, 0.0, 1.0],
        };
        let sphere = Surface::Sphere { frame, radius: 1.0 };
        let h = std::f64::consts::FRAC_PI_2;
        // Seam up to the pole and back, then the equator once round.
        let mut pts = vec![(0.0, 0.0), (0.0, h), (0.0, 0.0)];
        pts.extend(periodic_line(0.0, TAU, 0.0));
        close_through_pole(&sphere, &mut pts, false);
        let d = domain(vec![pts], (true, false));
        assert!(d.contains(2.0, 0.5));
        assert!(!d.contains(2.0, -0.5));
    }
}
