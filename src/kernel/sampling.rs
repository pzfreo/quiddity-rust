//! Edges as polylines: the parameter interval an edge covers on its curve, adaptive sampling
//! to a chord tolerance, and the exact extremes of curved edges.

use std::f64::consts::{PI, TAU};

use super::geom::{self, Curve, V3, dot};

/// Edge samples stand in for the exact curve wherever a polyline is needed (parameter-space
/// loops, boundary distances); the chord may stray from the curve by at most this much (mm).
pub const CHORD_TOLERANCE: f64 = 2e-4;
/// Initial uniform segments per edge (per quarter turn for conics) before adaptive refinement.
const INITIAL_SEGMENTS: usize = 16;
const MAX_REFINE_DEPTH: usize = 14;

/// The curve parameter interval an edge covers, in its own start → end direction.
pub fn edge_interval(
    curve: &Curve,
    start: V3,
    end: V3,
    same_sense: bool,
    closed: bool,
) -> (f64, f64) {
    let (a, b) = (curve.parameter(start), curve.parameter(end));
    let Some(period) = curve.period() else {
        return (a, b);
    };
    // On a closed curve the edge runs from start to end in its sense, wrapping if it must; a
    // closed edge is the whole curve.
    let mut b = b;
    if same_sense {
        while b <= a + if closed { period * 1e-12 } else { 0.0 } {
            b += period;
        }
    } else {
        while b >= a - if closed { period * 1e-12 } else { 0.0 } {
            b -= period;
        }
    }
    (a, b)
}

pub fn sample_edge(curve: &Curve, start: V3, end: V3, same_sense: bool, closed: bool) -> Vec<V3> {
    let (a, b) = edge_interval(curve, start, end, same_sense, closed);
    let n = match curve {
        Curve::Line { .. } => return vec![start, end],
        Curve::Nurbs(_) => INITIAL_SEGMENTS,
        _ => ((b - a).abs() / (TAU / 4.0) * INITIAL_SEGMENTS as f64)
            .ceil()
            .max(4.0) as usize,
    };
    let params: Vec<f64> = (0..=n).map(|i| a + (b - a) * i as f64 / n as f64).collect();
    let mut out = vec![curve.value(a)];
    for w in params.windows(2) {
        refine(
            curve,
            (w[0], out[out.len() - 1]),
            (w[1], curve.value(w[1])),
            0,
            &mut out,
        );
    }
    // Pin the ends to the vertices so loops close exactly.
    out[0] = start;
    *out.last_mut().expect("at least two samples") = end;
    out
}

/// Append the samples after *lo* up to and including *hi*, splitting while the chord's midpoint
/// strays from the curve by more than the tolerance.
fn refine(curve: &Curve, lo: (f64, V3), hi: (f64, V3), depth: usize, out: &mut Vec<V3>) {
    let tm = 0.5 * (lo.0 + hi.0);
    let pm = curve.value(tm);
    let chord_mid = geom::scale(geom::add(lo.1, hi.1), 0.5);
    if depth < MAX_REFINE_DEPTH && geom::dist(pm, chord_mid) > CHORD_TOLERANCE {
        refine(curve, lo, (tm, pm), depth + 1, out);
        refine(curve, (tm, pm), hi, depth + 1, out);
    } else {
        out.push(hi.1);
    }
}

/// Exact extremes of a circle arc per world axis — the samples alone would undercut a bulge.
pub fn arc_extremes(curve: &Curve, start: V3, end: V3, same_sense: bool, closed: bool) -> Vec<V3> {
    if !matches!(curve, Curve::Circle { .. }) {
        return vec![];
    }
    let axes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    extremes_along(
        curve,
        edge_interval(curve, start, end, same_sense, closed),
        &axes,
    )
}

/// The interior points of the curve over `interval` where the coordinate along each of `dirs`
/// is extreme: in closed form for circles and ellipses, by refining the best of a dense scan for
/// B-splines; a line has none (its ends are its extremes).
pub fn extremes_along(curve: &Curve, interval: (f64, f64), dirs: &[V3]) -> Vec<V3> {
    extreme_parameters(curve, interval, dirs)
        .into_iter()
        .map(|t| curve.value(t))
        .collect()
}

/// The curve parameters of `extremes_along`'s points.
pub fn extreme_parameters(curve: &Curve, (a, b): (f64, f64), dirs: &[V3]) -> Vec<f64> {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    let mut out = Vec::new();
    for &d in dirs {
        // Along d, a conic is A·cos t + B·sin t + C: stationary at atan2(B, A) and half a turn on.
        let base = match curve {
            Curve::Line { .. } => continue,
            Curve::Circle { frame, .. } => Some(dot(frame.y, d).atan2(dot(frame.x, d))),
            Curve::Ellipse {
                frame,
                major,
                minor,
            } => Some((minor * dot(frame.y, d)).atan2(major * dot(frame.x, d))),
            Curve::Nurbs(_) => None,
        };
        if let Some(base) = base {
            for k in -2..=3 {
                for t in [base + k as f64 * TAU, base + PI + k as f64 * TAU] {
                    if t > lo && t < hi {
                        out.push(t);
                    }
                }
            }
            continue;
        }
        for sign in [1.0, -1.0] {
            let g = |t: f64| sign * dot(curve.value(t), d);
            let n = 256;
            let at = |i: usize| lo + (hi - lo) * i as f64 / n as f64;
            let best = (0..=n)
                .max_by(|&i, &j| g(at(i)).total_cmp(&g(at(j))))
                .unwrap();
            if best == 0 || best == n {
                continue;
            }
            // Golden-section search for the maximum between the neighbouring scan points.
            let (mut x0, mut x1) = (at(best - 1), at(best + 1));
            let r = 0.5 * (5f64.sqrt() - 1.0);
            for _ in 0..80 {
                let (c, e) = (x1 - r * (x1 - x0), x0 + r * (x1 - x0));
                if g(c) > g(e) {
                    x1 = e;
                } else {
                    x0 = c;
                }
            }
            out.push(0.5 * (x0 + x1));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::geom::Frame;

    fn unit_circle() -> Curve {
        Curve::Circle {
            frame: Frame {
                origin: [0.0; 3],
                x: [1.0, 0.0, 0.0],
                y: [0.0, 1.0, 0.0],
                z: [0.0, 0.0, 1.0],
            },
            radius: 1.0,
        }
    }

    #[test]
    fn closed_edges_cover_the_whole_curve_in_their_sense() {
        let p = [1.0, 0.0, 0.0];
        let (a, b) = edge_interval(&unit_circle(), p, p, true, true);
        assert!((b - a - TAU).abs() < 1e-12);
        let (a, b) = edge_interval(&unit_circle(), p, p, false, true);
        assert!((a - b - TAU).abs() < 1e-12);
    }

    #[test]
    fn open_arcs_wrap_across_the_parameter_origin() {
        let (start, end) = ([0.0, -1.0, 0.0], [0.0, 1.0, 0.0]);
        let (a, b) = edge_interval(&unit_circle(), start, end, true, false);
        assert!(
            (b - a - std::f64::consts::PI).abs() < 1e-12,
            "through +x, not the long way"
        );
        let samples = sample_edge(&unit_circle(), start, end, true, false);
        assert!(samples.iter().any(|p| p[0] > 0.999));
        // Every chord stays within tolerance of the circle.
        for w in samples.windows(2) {
            let mid = geom::scale(geom::add(w[0], w[1]), 0.5);
            assert!(1.0 - geom::norm(mid) <= CHORD_TOLERANCE * 1.0001);
        }
    }
}
