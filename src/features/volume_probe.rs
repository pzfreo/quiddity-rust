//! Axis-aligned prism probes (`quiddity._volume_probe.prism_material_fraction` and
//! `prism_is_empty`): how much of an inset box a solid's material fills, answered by
//! [`kernel::volume`](crate::kernel::volume) rather than a boolean.

use super::Context;
use crate::kernel::geom::Bounds;
use crate::kernel::volume::{Probe, common_volume, fills_at_most, probe_volume};

/// OpenCascade cannot construct a volumetric probe at or below this extent (`PRISM_PROBE_FLOOR`).
pub const PRISM_PROBE_FLOOR: f64 = 1e-6;

/// Per axis, the `(low, high)` span of a prism.
pub type Spans = [(f64, f64); 3];

internal_entry! {
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
    let (size, probe) = inset_prism(spans, inset);
    let smallest = size.iter().copied().fold(f64::INFINITY, f64::min);
    if smallest <= 0.0 {
        return None;
    }
    if smallest <= PRISM_PROBE_FLOOR {
        return Some(1.0);
    }
    let occupied = common_volume(ctx.solid_classifier(solid), &probe)?;
    Some(occupied / (size[0] * size[1] * size[2]))
}
}

internal_entry! {
/// Whether the inset prism holds no material of *solid* at all (`prism_is_empty`): exactly
/// zero, not within a tolerance. A probe that cannot answer proves nothing, so is not empty.
pub fn prism_is_empty(ctx: &Context<'_>, solid: usize, spans: &Spans, inset: f64) -> bool {
    let (size, probe) = inset_prism(spans, inset);
    if size.iter().copied().fold(f64::INFINITY, f64::min) <= PRISM_PROBE_FLOOR {
        return false;
    }
    let denominator = size[0] * size[1] * size[2];
    // At extreme coordinates, rounding can collapse the constructed box even
    // when its requested size is positive. Preserve the fraction-valued
    // caller's answer when the bounded kernel query cannot divide by its box.
    if !denominator.is_normal() || !probe_volume(&probe).is_some_and(|v| v.is_normal() && v > 0.0) {
        return common_volume(ctx.solid_classifier(solid), &probe)
            .is_some_and(|occupied| occupied / denominator == 0.0);
    }
    // Only emptiness is needed. A positive running volume already disproves
    // it; the kernel's bounded query retains all unresolved-line semantics.
    fills_at_most(ctx.solid_classifier(solid), &probe, 0.0)
}
}

fn inset_prism(spans: &Spans, inset: f64) -> ([f64; 3], Probe<'static>) {
    let mut size = [0.0; 3];
    let mut centre = [0.0; 3];
    for (axis, &(low, high)) in spans.iter().enumerate() {
        let axis_inset = inset.min((high - low) / 4.0);
        size[axis] = (high - low) - 2.0 * axis_inset;
        centre[axis] = (low + high) / 2.0;
    }
    let probe = Probe::Box(Bounds {
        min: [0, 1, 2].map(|i| centre[i] - size[i] / 2.0),
        max: [0, 1, 2].map(|i| centre[i] + size[i] / 2.0),
    });
    (size, probe)
}
