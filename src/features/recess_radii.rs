//! Fail-closed corner-radius proofs for slots and pockets (`quiddity._recess_radii`): four
//! equal concave quarter cylinders at the four flat-wall junctions, spanning the record's whole
//! depth. Anything partial, mixed, misplaced or ambiguous leaves the radius unknown.

use super::recess_faces::{Cylinder, union_bb};
use super::recess_records::Recess;
use crate::kernel::geom::Bounds;
use crate::kernel::py;

/// The largest displacement of two independently published (two-decimal) coordinates.
const PUBLISHED_COORD_TOL: f64 = 0.011;
/// A quarter cylinder spans one radius on both footprint axes, to this ratio.
const QUARTER_RATIO_TOL: f64 = 0.1;

fn close(left: f64, right: f64) -> bool {
    (left - right).abs() <= PUBLISHED_COORD_TOL
}

fn same_span(actual: (f64, f64), expected: (f64, f64)) -> bool {
    close(actual.0, expected.0) && close(actual.1, expected.1)
}

fn axis_bounds(bb: &Bounds, axis: usize) -> (f64, f64) {
    (bb.min[axis], bb.max[axis])
}

/// The distinct radii of concave cylinders along the depth axis spanning the record's depth.
fn candidate_radii<R: Recess>(cylinders: &[Cylinder], record: &R) -> Vec<f64> {
    let mut radii: Vec<f64> = Vec::new();
    for c in cylinders {
        if !c.concave
            || c.axis != record.depth_axis()
            || c.radius <= 0.0
            || !same_span(axis_bounds(&c.bb, c.axis), (record.d_lo(), record.d_hi()))
        {
            continue;
        }
        if !radii.iter().any(|&r| close(c.radius, r)) {
            radii.push(c.radius);
        }
    }
    radii
}

/// Whether one corner (*long_side*, *width_side* each ±1) is a quarter cylinder of *radius*.
fn site_matches<R: Recess>(
    cylinders: &[Cylinder],
    record: &R,
    radius: f64,
    long_side: i32,
    width_side: i32,
) -> bool {
    let (la, wa, da) = (record.long_axis(), record.width_axis(), record.depth_axis());
    let long_bounds = if long_side < 0 {
        (record.lo() - radius, record.lo())
    } else {
        (record.hi(), record.hi() + radius)
    };
    let width_low = record.w_center() - record.width() / 2.0;
    let width_high = record.w_center() + record.width() / 2.0;
    let width_bounds = if width_side < 0 {
        (width_low, width_low + radius)
    } else {
        (width_high - radius, width_high)
    };
    let depth_bounds = (record.d_lo(), record.d_hi());
    let long_center = if long_side < 0 {
        record.lo()
    } else {
        record.hi()
    };
    let width_center = record.w_center() + f64::from(width_side) * (record.width() / 2.0 - radius);
    let matches: Vec<&Cylinder> = cylinders
        .iter()
        .filter(|c| {
            c.concave
                && c.axis == da
                && close(c.radius, radius)
                && close(c.location[la], long_center)
                && close(c.location[wa], width_center)
                && same_span(axis_bounds(&c.bb, da), depth_bounds)
        })
        .collect();
    let Some((first, rest)) = matches.split_first() else {
        return false;
    };
    let combined = rest.iter().fold(first.bb, |b, c| union_bb(&b, &c.bb));
    let expected = [(la, long_bounds), (wa, width_bounds), (da, depth_bounds)];
    if !expected
        .iter()
        .all(|&(axis, span)| same_span(axis_bounds(&combined, axis), span))
    {
        return false;
    }
    [la, wa].iter().all(|&axis| {
        let (lo, hi) = axis_bounds(&combined, axis);
        ((hi - lo) / radius - 1.0).abs() <= QUARTER_RATIO_TOL
    })
}

/// A uniform corner radius proved at all four corners, or `None`: not proved, which does not
/// mean square (`_proved_corner_radius`).
fn proved_corner_radius<R: Recess>(record: &R, cylinders: &[Cylinder]) -> Option<f64> {
    if record.edge_anchored() {
        return None;
    }
    let mut proved: Vec<f64> = Vec::new();
    for radius in candidate_radii(cylinders, record) {
        // At half-width the footprint is a stadium: its ends are the end-radius proof's.
        if radius >= record.width() / 2.0 - PUBLISHED_COORD_TOL {
            continue;
        }
        let all = [(-1, -1), (-1, 1), (1, -1), (1, 1)]
            .iter()
            .all(|&(l, w)| site_matches(cylinders, record, radius, l, w));
        if all {
            let published = py::round_to(radius, 2);
            if published > 0.0 && !proved.contains(&published) {
                proved.push(published);
            }
        }
    }
    match proved[..] {
        [only] => Some(only),
        _ => None,
    }
}

/// The record with a proved corner radius published and each end extended by it, the walls
/// having located `lo`/`hi` at the arc tangencies; otherwise unchanged
/// (`_with_proved_corner_radius`).
pub fn with_proved_corner_radius<R: Recess>(record: R, cylinders: &[Cylinder]) -> R {
    let Some(radius) = proved_corner_radius(&record, cylinders) else {
        return record;
    };
    let mut out = record.spanned(
        py::round_to(record.lo() - radius, 2),
        py::round_to(record.hi() + radius, 2),
        py::round_to(record.hi() - record.lo() + 2.0 * radius, 2),
    );
    out.set_corner_radius(radius);
    out
}
