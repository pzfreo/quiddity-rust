//! Axis-aligned prism probes (`quiddity._volume_probe.prism_material_fraction` and
//! `prism_is_empty`): how much of an inset box a solid's material fills, answered by
//! [`kernel::volume`](crate::kernel::volume) rather than a boolean.

use super::Context;
use crate::kernel::geom::Bounds;
use crate::kernel::volume::{Probe, common_volume};

/// OpenCascade cannot construct a volumetric probe at or below this extent (`PRISM_PROBE_FLOOR`).
pub const PRISM_PROBE_FLOOR: f64 = 1e-6;

/// Per axis, the `(low, high)` span of a prism.
pub type Spans = [(f64, f64); 3];

/// The fraction of the prism, drawn in by *inset* (at most a quarter of each span) on every side,
/// that *solid*'s material fills (`prism_material_fraction`). `None` where Python raises: a span
/// without positive extent; and where the volume probe cannot answer. A prism too thin to build
/// counts as full: it cannot prove emptiness.
pub fn prism_material_fraction(
    ctx: &Context<'_>,
    solid: usize,
    spans: &Spans,
    inset: f64,
) -> Option<f64> {
    let mut size = [0.0; 3];
    let mut centre = [0.0; 3];
    for (axis, &(low, high)) in spans.iter().enumerate() {
        let axis_inset = inset.min((high - low) / 4.0);
        size[axis] = (high - low) - 2.0 * axis_inset;
        centre[axis] = (low + high) / 2.0;
    }
    let smallest = size.iter().copied().fold(f64::INFINITY, f64::min);
    if smallest <= 0.0 {
        return None;
    }
    if smallest <= PRISM_PROBE_FLOOR {
        return Some(1.0);
    }
    let probe = Probe::Box(Bounds {
        min: [0, 1, 2].map(|i| centre[i] - size[i] / 2.0),
        max: [0, 1, 2].map(|i| centre[i] + size[i] / 2.0),
    });
    let occupied = common_volume(ctx.solid_classifier(solid), &probe)?;
    Some(occupied / (size[0] * size[1] * size[2]))
}

/// Whether the inset prism holds no material of *solid* at all (`prism_is_empty`): exactly
/// zero, not within a tolerance. A probe that cannot answer proves nothing, so is not empty.
pub fn prism_is_empty(ctx: &Context<'_>, solid: usize, spans: &Spans, inset: f64) -> bool {
    prism_material_fraction(ctx, solid, spans, inset) == Some(0.0)
}
