//! The recognisers' shared tolerances and thresholds (`quiddity._geometry`): recogniser policy,
//! kept out of the kernel so changing one is not a kernel change.

use crate::kernel::geom::COORD_FLOOR;
use crate::kernel::py;

pub const AXIS_ALIGNED_COS: f64 = 0.99;
/// A normal component at or below this is no component at all (`AXIS_ZERO_COS`).
pub const AXIS_ZERO_COS: f64 = 0.01;
pub const INTERIOR_PROBE_FRAC: f64 = 0.05;

/// `quiddity._geometry.clears_threshold`: above the threshold, an exact tie (to 1e-9) counting
/// as below, so the decision is the same at every scale.
pub fn clears_threshold(magnitude: f64, threshold: f64) -> bool {
    magnitude >= threshold * (1.0 + 1e-9)
}

/// `quiddity._geometry.cluster_coordinates`: index groups no wider than *tol*, each opened by
/// its lowest member, in ascending order (ties in input order).
pub fn cluster_coordinates(coordinates: &[f64], tol: f64) -> Vec<Vec<usize>> {
    let mut order: Vec<usize> = (0..coordinates.len()).collect();
    order.sort_by(|&a, &b| py::order(coordinates[a], coordinates[b]).then(a.cmp(&b)));
    let mut clusters: Vec<Vec<usize>> = Vec::new();
    for index in order {
        match clusters.last_mut() {
            Some(last) if coordinates[index] - coordinates[last[0]] <= tol => last.push(index),
            _ => clusters.push(vec![index]),
        }
    }
    clusters
}

/// `quiddity._geometry.length_tol`.
pub fn length_tol(nominal: f64, rel: f64) -> f64 {
    rel * nominal + COORD_FLOOR
}
