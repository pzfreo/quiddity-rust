//! The recognisers' shared tolerances and thresholds (`quiddity._geometry`): recogniser policy,
//! kept out of the kernel so changing one is not a kernel change.

use crate::kernel::geom::COORD_FLOOR;
use crate::kernel::py;

pub const AXIS_ALIGNED_COS: f64 = 0.99;
/// A normal component at or below this is no component at all (`AXIS_ZERO_COS`).
pub const AXIS_ZERO_COS: f64 = 0.01;
pub const INTERIOR_PROBE_FRAC: f64 = 0.05;

parity_entry! {
/// `quiddity._geometry.clears_threshold`: above the threshold, an exact tie (to 1e-9) counting
/// as below, so the decision is the same at every scale.
pub fn clears_threshold(magnitude: f64, threshold: f64) -> bool {
    magnitude >= threshold * (1.0 + 1e-9)
}
}

parity_entry! {
/// Bounded coordinate clusters. A unique minimum-cardinality partition is intrinsic to the
/// distances; opening each cluster at the lowest coordinate would choose a side of a tolerance
/// chain. Connected components with multiple minimum partitions refuse rather than claim one.
pub fn cluster_coordinates(coordinates: &[f64], tol: f64) -> Vec<Vec<usize>> {
    let mut order: Vec<usize> = (0..coordinates.len()).collect();
    order.sort_by(|&a, &b| py::order(coordinates[a], coordinates[b]).then(a.cmp(&b)));
    let mut out = Vec::new();
    let mut start = 0;
    while start < order.len() {
        let mut end = start + 1;
        while end < order.len()
            && coordinates[order[end]] - coordinates[order[end - 1]] <= tol + COORD_FLOOR
        {
            end += 1;
        }
        let component = &order[start..end];
        let n = component.len();
        let mut minimum = vec![n + 1; n + 1];
        let mut ways = vec![0usize; n + 1];
        let mut next = vec![0; n];
        minimum[n] = 0;
        ways[n] = 1;
        for i in (0..n).rev() {
            for j in i..n {
                if coordinates[component[j]] - coordinates[component[i]] > tol + COORD_FLOOR {
                    break;
                }
                let count = 1 + minimum[j + 1];
                if count < minimum[i] {
                    minimum[i] = count;
                    ways[i] = ways[j + 1];
                    next[i] = j + 1;
                } else if count == minimum[i] {
                    ways[i] = (ways[i] + ways[j + 1]).min(2);
                }
            }
        }
        if ways[0] == 1 {
            let mut i = 0;
            while i < n {
                out.push(component[i..next[i]].to_vec());
                i = next[i];
            }
        }
        start = end;
    }
    out
}
}

parity_entry! {
/// `quiddity._geometry.length_tol`.
pub fn length_tol(nominal: f64, rel: f64) -> f64 {
    rel * nominal + COORD_FLOOR
}
}

parity_entry! {
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

#[cfg(test)]
mod chain_motion_tests {
    use super::*;
    #[test]
    fn symmetric_tolerance_chain_refuses_after_translation_and_axis_reversal() {
        // Three collinear levels permit [a,b]+[c] and [a]+[b,c], with identical evidence.
        for shift in [0., 123.456, -78.9] {
            for sign in [-1., 1.] {
                let values = [0., 0.4, 0.8].map(|v| shift + sign * v);
                assert!(cluster_coordinates(&values, 0.5).is_empty());
                let mut values = values;
                values.reverse();
                assert!(cluster_coordinates(&values, 0.5).is_empty());
            }
        }
        assert_eq!(
            cluster_coordinates(&[0., 0.4, 0.8, 1.2], 0.5),
            vec![vec![0, 1], vec![2, 3]]
        );
    }
}
