//! Whole-solid queries the families ask (`quiddity._solid_properties`): a solid's box,
//! validity, volume and area.
//!
//! Python keeps one cache of these per run so nine families do not recompute them. Here the
//! per-face answers they are made of (each face's box and mass, a solid's validity) are cached
//! on [`Part`] itself, so asking again costs a sum over cached faces, and every family reads the
//! same values. [`super::body`] builds body keys from the same reads.

use crate::kernel::brep::Part;
use crate::kernel::geom::Bounds;

/// The solid's box (`SolidProperties.bounding_box`).
pub fn bounding_box(part: &Part, solid: usize) -> Bounds {
    part.solid_bounds(solid)
}

/// Whether the kernel calls the solid valid (`SolidProperties.is_valid`).
pub fn is_valid(part: &Part, solid: usize) -> bool {
    part.solid_is_valid(solid)
}

/// The solid's volume (`SolidProperties.volume`), or `None` when a face's mass cannot be
/// computed: an unknown volume is never reported as a number.
pub fn volume(part: &Part, solid: usize) -> Option<f64> {
    part.solid_mass(solid).map(|(v, _)| v)
}

/// The solid's area (`SolidProperties.area`), or `None` as for [`volume`].
pub fn area(part: &Part, solid: usize) -> Option<f64> {
    part.solid_mass(solid).map(|(_, a)| a)
}
