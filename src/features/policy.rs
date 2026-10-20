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
            Some(last) if coordinates[index] - coordinates[last[0]] <= tol + COORD_FLOOR => {
                last.push(index)
            }
            _ => clusters.push(vec![index]),
        }
    }
    clusters
}

/// `quiddity._geometry.length_tol`.
pub fn length_tol(nominal: f64, rel: f64) -> f64 {
    rel * nominal + COORD_FLOOR
}

/// Round a measured size on its publication grid. Decimal half-way points within the
/// kernel's coordinate floor use ties-to-even, so placement round-off cannot choose a side.
/// Absolute positions keep their existing publication rule; round lengths before positions.
/// On grids finer than the coordinate floor, neighbouring half-way snap bands overlap;
/// use ordinary ties-to-even rounding there instead of collapsing distinct grid values.
pub fn round_measurement(value: f64, digits: usize) -> f64 {
    let factor = 10f64.powi(digits as i32);
    if COORD_FLOOR * factor >= 0.5 {
        return py::round_to(value, digits);
    }
    let scaled = value * factor;
    let half = scaled.floor() + 0.5;
    let stable = if (scaled - half).abs() <= COORD_FLOOR * factor {
        half
    } else {
        scaled
    };
    py::without_negative_zero(stable.round_ties_even() / factor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measurement_halfway_rounding_survives_coordinate_noise() {
        for (size, expected) in [(92.075, 92.08), (59.075, 59.08), (1.225, 1.22)] {
            for noise in [-1e-9, 0.0, 1e-9] {
                assert_eq!(round_measurement(size + noise, 2), expected);
                assert_eq!(round_measurement(-size + noise, 2), -expected);
            }
        }
        assert_eq!(round_measurement(1.22501, 2), 1.23);
        assert_eq!(round_measurement(1.22499, 2), 1.22);
    }

    #[test]
    fn coordinate_cluster_includes_the_tolerance_boundary_after_translation() {
        for offset in [0.0, 123.456, -78.9] {
            assert_eq!(
                cluster_coordinates(&[offset, offset + 0.5], 0.5),
                vec![vec![0, 1]]
            );
            assert_eq!(
                cluster_coordinates(&[offset, offset + 0.50001], 0.5),
                vec![vec![0], vec![1]]
            );
        }
    }

    #[test]
    fn fine_measurement_grids_do_not_collapse_adjacent_values() {
        for (value, expected) in [(1.0000029, 1.000003), (12.3456789, 12.345679)] {
            assert_eq!(round_measurement(value, 6), expected);
            assert_eq!(round_measurement(-value, 6), -expected);
            assert_eq!(round_measurement(value, 7), value);
            assert_eq!(round_measurement(-value, 7), -value);
        }
    }
}
