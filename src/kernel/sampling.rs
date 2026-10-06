//! Edges as polylines: the parameter interval an edge covers on its curve, adaptive sampling
//! to a chord tolerance, and the exact extremes of circular arcs.

use std::f64::consts::TAU;

use super::geom::{self, Curve, V3};

/// Edge samples stand in for the exact curve wherever a polyline is needed (parameter-space
/// loops, boundary distances); the chord may stray from the curve by at most this much (mm).
pub const CHORD_TOLERANCE: f64 = 2e-4;
/// Initial uniform segments per edge (per quarter turn for conics) before adaptive refinement.
const INITIAL_SEGMENTS: usize = 16;
const MAX_REFINE_DEPTH: usize = 14;

/// The curve parameter interval an edge covers, in its own start → end direction.
fn edge_interval(curve: &Curve, start: V3, end: V3, same_sense: bool, closed: bool) -> (f64, f64) {
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
    let Curve::Circle { frame, .. } = curve else {
        return vec![];
    };
    let (a, b) = edge_interval(curve, start, end, same_sense, closed);
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    let mut out = Vec::new();
    for i in 0..3 {
        let base = frame.y[i].atan2(frame.x[i]);
        for k in -2..=3 {
            for t in [
                base + k as f64 * TAU,
                base + std::f64::consts::PI + k as f64 * TAU,
            ] {
                if t > lo && t < hi {
                    out.push(curve.value(t));
                }
            }
        }
    }
    out
}
