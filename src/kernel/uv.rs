//! A face in its surface's parameter space: boundary loops unwrapped across periodic
//! parameters (with singular points routed along their parameter line), OpenCascade-compatible
//! parameter ranges, and point containment.

use std::f64::consts::TAU;

use super::brep::{Part, Pcurve};
use super::geom::{self, Surface, V3};
use super::sampling::{edge_interval, extremes_along};

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
    /// Per edge of the loop, where each run of its regular samples (not at a pole or apex)
    /// starts in `points`: the turn the edge is on there, which continuity alone cannot carry
    /// across a singular point.
    pub anchors: Vec<Vec<(f64, f64)>>,
    /// Whether the boundary itself runs round u (before any closing along a pole).
    pub winds_u: bool,
}

impl Part {
    /// The face's outer loop (`BRepTools::OuterWire`): the first loop, replaced by any later loop
    /// whose parameter-space box contains the current one's (a later tie wins).
    pub fn outer_loop(&self, face: usize) -> Option<usize> {
        self.loop_placement(face).map(|(outer, _)| outer)
    }

    /// The distinct edges of the face's outer loop (`outer_wire().edges()`).
    pub fn outer_edges(&self, face: usize) -> Vec<usize> {
        let Some(outer) = self.outer_loop(face) else {
            return Vec::new();
        };
        let mut edges: Vec<usize> = self.faces[face].loops[outer]
            .edges
            .iter()
            .map(|e| e.0)
            .collect();
        edges.sort_unstable();
        edges.dedup();
        edges
    }

    /// The outer loop and, per loop, the shift (a whole number of periods) that places it in one
    /// parameter period with the others. Loops are first centred in `[0, 2π)` — where
    /// OpenCascade's pcurves sit — to choose the outer loop; every other loop is then moved to
    /// the period nearest the outer loop's centre, so a hole far round a partial face from the
    /// face's first sample still lands inside it.
    pub(super) fn loop_placement(&self, face: usize) -> Option<(usize, Vec<(f64, f64)>)> {
        let loops = self.uv_loops(face)?;
        let (pu, pv) = self.faces[face].surface.periodic();
        let boxes: Vec<[f64; 4]> = loops
            .iter()
            .map(|lp| {
                lp.points.iter().fold(
                    [
                        f64::INFINITY,
                        f64::NEG_INFINITY,
                        f64::INFINITY,
                        f64::NEG_INFINITY,
                    ],
                    |b, &(u, v)| [b[0].min(u), b[1].max(u), b[2].min(v), b[3].max(v)],
                )
            })
            .collect();
        let centre = |b: &[f64; 4]| (0.5 * (b[0] + b[1]), 0.5 * (b[2] + b[3]));
        let into_period =
            |c: f64, periodic: bool| if periodic { c.rem_euclid(TAU) - c } else { 0.0 };
        let first: Vec<(f64, f64)> = boxes
            .iter()
            .map(|b| {
                let c = centre(b);
                (into_period(c.0, pu), into_period(c.1, pv))
            })
            .collect();
        let placed = |i: usize| {
            let b = boxes[i];
            [
                b[0] + first[i].0,
                b[1] + first[i].0,
                b[2] + first[i].1,
                b[3] + first[i].1,
            ]
        };
        let mut outer = 0;
        for i in 1..loops.len() {
            let (o, c) = (placed(outer), placed(i));
            if c[0] <= o[0] && c[1] >= o[1] && c[2] <= o[2] && c[3] >= o[3] {
                outer = i;
            }
        }
        let anchor = centre(&placed(outer));
        let shifts = (0..loops.len())
            .map(|i| {
                if i == outer {
                    return first[i];
                }
                let c = centre(&boxes[i]);
                let su = if pu {
                    geom::nearest_turn(c.0, anchor.0) - c.0
                } else {
                    0.0
                };
                let sv = if pv {
                    geom::nearest_turn(c.1, anchor.1) - c.1
                } else {
                    0.0
                };
                (su, sv)
            })
            .collect();
        (!loops.is_empty()).then_some((outer, shifts))
    }

    /// ∮ (x dy − y dx)/2, ∮ x² dy/2 and −∮ y² dx/2 along one edge in the plane of *frame*, by
    /// Gauss–Legendre quadrature on short pieces of the exact curve.
    fn edge_moments(
        &self,
        edge: usize,
        forward: bool,
        frame: &crate::kernel::geom::Frame,
    ) -> (f64, f64, f64) {
        const NODES: [(f64, f64); 8] = [
            (-0.960_289_856_497_536_3, 0.101_228_536_290_376_26),
            (-0.796_666_477_413_626_7, 0.222_381_034_453_374_47),
            (-0.525_532_409_916_329, 0.313_706_645_877_887_3),
            (-0.183_434_642_495_649_8, 0.362_683_783_378_362),
            (0.183_434_642_495_649_8, 0.362_683_783_378_362),
            (0.525_532_409_916_329, 0.313_706_645_877_887_3),
            (0.796_666_477_413_626_7, 0.222_381_034_453_374_47),
            (0.960_289_856_497_536_3, 0.101_228_536_290_376_26),
        ];
        let ed = &self.edges[edge];
        let (mut t0, mut t1) = crate::kernel::sampling::edge_interval(
            &ed.curve,
            ed.start,
            ed.end,
            ed.same_sense,
            ed.is_closed(),
        );
        if !forward {
            std::mem::swap(&mut t0, &mut t1);
        }
        let pieces = match ed.curve {
            crate::kernel::geom::Curve::Line { .. } => 1,
            _ => 32,
        };
        let local = |t: f64| {
            let l = frame.to_local(ed.curve.value(t));
            (l[0], l[1])
        };
        let (mut a, mut mx, mut my) = (0.0, 0.0, 0.0);
        for k in 0..pieces {
            let (lo, hi) = (
                t0 + (t1 - t0) * k as f64 / pieces as f64,
                t0 + (t1 - t0) * (k + 1) as f64 / pieces as f64,
            );
            let (half, centre) = (0.5 * (hi - lo), 0.5 * (hi + lo));
            let h = 1e-6 * half.abs().max(1e-12);
            for (x, w) in NODES {
                let t = centre + half * x;
                let (px, py) = local(t);
                let (p1, p0) = (local(t + h), local(t - h));
                let (dx, dy) = ((p1.0 - p0.0) / (2.0 * h), (p1.1 - p0.1) / (2.0 * h));
                let weight = w * half;
                a += weight * 0.5 * (px * dy - py * dx);
                mx += weight * 0.5 * px * px * dy;
                my -= weight * 0.5 * py * py * dx;
            }
        }
        (a, mx, my)
    }

    /// build123d's `Face.center()`: the area centroid of a planar face, otherwise the surface
    /// point at the middle of the face's parameter range.
    pub fn face_centre(&self, face: usize) -> Option<V3> {
        let f = &self.faces[face];
        let Surface::Plane { frame } = f.surface else {
            let (u0, u1, v0, v1) = self.uv_bounds(face)?;
            return Some(f.surface.value(0.5 * (u0 + u1), 0.5 * (v0 + v1)));
        };
        // Green's theorem along each loop's exact edge curves, in plane coordinates; the largest
        // loop is the outer boundary and the rest are holes in it (loop orientation is not
        // relied on). Polyline shoelaces would carry the samples' chord error.
        let mut loops: Vec<(f64, f64, f64)> = f
            .loops
            .iter()
            .filter(|lp| !lp.edges.is_empty())
            .map(|lp| {
                let (mut a, mut mx, mut my) = (0.0, 0.0, 0.0);
                for &(e, forward) in &lp.edges {
                    let (da, dmx, dmy) = self.edge_moments(e, forward, &frame);
                    (a, mx, my) = (a + da, mx + dmx, my + dmy);
                }
                (a.abs(), mx / a, my / a)
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
                // A degenerate loop: the whole periodic u line at the apex's v (one entry per
                // face loop either way, so loop indices agree with the face's).
                let (u, v) = f.surface.parameters(apex, None)?;
                let points = if pu {
                    periodic_line(0.0, TAU, v)
                } else {
                    vec![(u, v)]
                };
                loops.push(UvLoop {
                    points,
                    anchors: Vec::new(),
                    winds_u: false,
                });
                continue;
            }
            let mut raw: Vec<(f64, f64)> = Vec::new();
            let mut points: Vec<V3> = Vec::new();
            let mut firsts = Vec::with_capacity(lp.edges.len());
            for &(e, forward) in &lp.edges {
                firsts.push(raw.len());
                let samples = &self.edges[e].samples;
                let ordered: Box<dyn Iterator<Item = &V3>> = if forward {
                    Box::new(samples.iter())
                } else {
                    Box::new(samples.iter().rev())
                };
                for p in ordered {
                    let hint = raw.last().copied();
                    raw.push(f.surface.parameters(*p, hint)?);
                    points.push(*p);
                }
            }
            // The loop's first samples are inverted with nothing to follow; on a closed B-spline
            // surface (not marked periodic) a seam point has two parameter values. Walk the start
            // again following on from the loop's end, until it agrees with the first pass.
            if !pu && !pv && raw.len() > 2 {
                let mut hint = raw[raw.len() - 1];
                for k in 0..raw.len() - 1 {
                    let again = f.surface.parameters(points[k], Some(hint))?;
                    let same = (again.0 - raw[k].0).abs() + (again.1 - raw[k].1).abs() < 1e-9;
                    raw[k] = again;
                    hint = again;
                    if same {
                        break;
                    }
                }
            }
            let (raw, placed) = route_singular_points(&f.surface, raw, f.reversed);
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
            let winds_u = match (pts.first(), pts.last()) {
                (Some(a), Some(b)) => pu && (b.0 - a.0).abs() > 1.0,
                _ => false,
            };
            close_through_pole(&f.surface, &mut pts, f.reversed);
            let ends: Vec<usize> = firsts
                .iter()
                .skip(1)
                .copied()
                .chain([placed.len()])
                .collect();
            let anchors = firsts
                .iter()
                .zip(&ends)
                .map(|(&a, &b)| {
                    (a..b)
                        .filter(|&k| placed[k].is_some() && (k == a || placed[k - 1].is_none()))
                        .map(|k| pts[placed[k].expect("regular")])
                        .collect()
                })
                .collect();
            loops.push(UvLoop {
                points: pts,
                anchors,
                winds_u,
            });
        }
        Some(loops)
    }

    /// `BRepTools::UVBounds` — (u_first, u_last, v_first, v_last) of the face's trimmed range.
    ///
    /// A periodic direction the face closes around (a seam edge, or a loop that winds once
    /// round) spans exactly one period starting at the seam.
    pub fn uv_bounds(&self, face: usize) -> Option<(f64, f64, f64, f64)> {
        let f = &self.faces[face];
        if let Surface::Plane { frame } = f.surface {
            // A plane's range is exactly its edges' extent in plane coordinates.
            let mut range = [
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
            ];
            for e in self.face_edges(face) {
                let ed = &self.edges[e];
                let interval =
                    edge_interval(&ed.curve, ed.start, ed.end, ed.same_sense, ed.is_closed());
                let ends = [ed.curve.value(interval.0), ed.curve.value(interval.1)];
                for p in
                    ends.into_iter()
                        .chain(extremes_along(&ed.curve, interval, &[frame.x, frame.y]))
                {
                    let l = frame.to_local(p);
                    range = [
                        range[0].min(l[0]),
                        range[1].max(l[0]),
                        range[2].min(l[1]),
                        range[3].max(l[1]),
                    ];
                }
            }
            return range[0]
                .is_finite()
                .then_some((range[0], range[1], range[2], range[3]));
        }
        let (pu, pv) = f.surface.periodic();
        let loops = self.uv_loops(face)?;
        let mut range = [
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ];
        let mut add = |(u, v): (f64, f64)| {
            range = [
                range[0].min(u),
                range[1].max(u),
                range[2].min(v),
                range[3].max(v),
            ];
        };
        // Loops are placed in one period by `loop_placement`; whether a loop winds round a
        // periodic direction is read from its ends.
        let near = |(u, v): (f64, f64), (ru, rv): (f64, f64)| {
            (
                if pu { geom::nearest_turn(u, ru) } else { u },
                if pv { geom::nearest_turn(v, rv) } else { v },
            )
        };
        let mut winds = (false, false);
        for lp in loops {
            let (Some(&first), Some(&last)) = (lp.points.first(), lp.points.last()) else {
                continue;
            };
            winds.0 |= pu && (last.0 - first.0).abs() > 1.0;
            winds.1 |= pv && (last.1 - first.1).abs() > 1.0;
        }
        let (_, shifts) = self.loop_placement(face)?;
        if matches!(f.surface, Surface::Sphere { .. }) || f.pcurves.is_empty() {
            for (lp, shift) in loops.iter().zip(&shifts) {
                lp.points
                    .iter()
                    .for_each(|&(u, v)| add((u + shift.0, v + shift.1)));
            }
        } else {
            // Edge by edge: where the file gives an edge's pcurve, it sizes the range in place of
            // the 3D edge (aligned to the edge's own samples to pick the period).
            for ((lp, uv), shift) in f.loops.iter().zip(loops).zip(&shifts) {
                if lp.vertex.is_some() {
                    continue; // an apex bounds v through its edges; its u is meaningless
                }
                // Start this loop where its own unwrapped samples sit, in its placed period.
                let Some(&start) = uv.points.first() else {
                    continue;
                };
                let mut last = (start.0 + shift.0, start.1 + shift.1);
                let (lu, lv) = (
                    self::range(uv.points.iter(), |p| p.0),
                    self::range(uv.points.iter(), |p| p.1),
                );
                let loop_centre = (0.5 * (lu.0 + lu.1) + shift.0, 0.5 * (lv.0 + lv.1) + shift.1);
                for &(e, forward) in &lp.edges {
                    let samples = &self.edges[e].samples;
                    let ordered: Box<dyn Iterator<Item = &V3>> = if forward {
                        Box::new(samples.iter())
                    } else {
                        Box::new(samples.iter().rev())
                    };
                    let mut pts = Vec::with_capacity(samples.len());
                    for p in ordered {
                        let q = f.surface.parameters(*p, None)?;
                        if f.surface.singular_v(q.1).is_some() {
                            continue; // a pole or apex: its u is arbitrary
                        }
                        let q = near(q, last);
                        pts.push(q);
                        last = q;
                    }
                    if pts.is_empty() {
                        continue;
                    }
                    // Put the edge in the period its loop occupies: across a skipped pole or apex
                    // sample, continuity alone cannot tell which turn the edge is on.
                    let n = pts.len() as f64;
                    let mean = (
                        pts.iter().map(|p| p.0).sum::<f64>() / n,
                        pts.iter().map(|p| p.1).sum::<f64>() / n,
                    );
                    let placed = near(mean, loop_centre);
                    let (du, dv) = (placed.0 - mean.0, placed.1 - mean.1);
                    pts.iter_mut().for_each(|p| *p = (p.0 + du, p.1 + dv));
                    last = *pts.last().expect("non-empty");
                    let own: Vec<&Pcurve> = f
                        .pcurves
                        .iter()
                        .filter(|(pe, _)| *pe == e)
                        .map(|(_, c)| c)
                        .collect();
                    if own.is_empty() {
                        pts.iter().for_each(|&p| add(p));
                        continue;
                    }
                    let centre = placed;
                    for curve in own {
                        match curve {
                            Pcurve::Poles(poles) => {
                                let m = poles.len() as f64;
                                let mid = (
                                    poles.iter().map(|p| p.0).sum::<f64>() / m,
                                    poles.iter().map(|p| p.1).sum::<f64>() / m,
                                );
                                let aligned = near(mid, centre);
                                poles.iter().for_each(|&(u, v)| {
                                    add((u + aligned.0 - mid.0, v + aligned.1 - mid.1))
                                });
                            }
                            Pcurve::Line { point, dir } => {
                                // A line constant in one parameter fixes that parameter exactly;
                                // the other comes from the edge's samples.
                                let along_v = dir.0.abs() <= 1e-12 * dir.1.abs();
                                let along_u = dir.1.abs() <= 1e-12 * dir.0.abs();
                                let fixed = near(*point, centre);
                                for &(u, v) in &pts {
                                    add((
                                        if along_v { fixed.0 } else { u },
                                        if along_u { fixed.1 } else { v },
                                    ));
                                }
                            }
                        }
                    }
                }
            }
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
///
/// Also returns where each regular input sample lands in the output (`None` for a singular one).
fn route_singular_points(
    surface: &Surface,
    raw: Vec<(f64, f64)>,
    reversed: bool,
) -> (Vec<(f64, f64)>, Vec<Option<usize>>) {
    let n = raw.len();
    let singular: Vec<bool> = raw
        .iter()
        .map(|&(_, v)| surface.singular_v(v).is_some())
        .collect();
    if !singular.iter().any(|s| *s) || singular.iter().all(|s| *s) {
        return (raw, (0..n).map(Some).collect());
    }
    let mut placed = vec![None; n];
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
    // With a single singular run, route the way that leaves the closed loop turning least round
    // u (a face that does run round the axis is described as well by a loop that does not wind
    // but spans the turn); only a tie needs the orientation.
    let runs = (0..n)
        .filter(|&k| singular[k] && !singular[(k + n - 1) % n])
        .count();
    let regular_turn: f64 = (0..n)
        .filter(|&k| !singular[k] && !singular[(k + 1) % n])
        .map(|k| {
            let (a, b) = (raw[k].0, raw[(k + 1) % n].0);
            geom::nearest_turn(b, a) - a
        })
        .sum();
    let mut out = Vec::with_capacity(n + 64);
    let mut i = 0;
    while i < n {
        if !singular[i] {
            placed[(i + first_regular) % n] = Some(out.len());
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
        let gap = |ub: f64| (regular_turn + ub - ua).abs();
        let unambiguous = runs == 1
            && (gap(ub + TAU) - gap(ub)).abs() > 1e-6
            && (gap(ub - TAU) - gap(ub)).abs() > 1e-6;
        if unambiguous {
            ub = [ub - TAU, ub, ub + TAU]
                .into_iter()
                .min_by(|a, b| gap(*a).total_cmp(&gap(*b)))
                .unwrap();
        } else if (ub - ua).abs() > 1e-6 {
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
    (out, placed)
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
        let loops: Vec<UvLoop> = loops
            .into_iter()
            .map(|points| UvLoop {
                points,
                anchors: Vec::new(),
                winds_u: false,
            })
            .collect();
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
