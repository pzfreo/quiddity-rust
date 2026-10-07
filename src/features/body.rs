//! Geometry-derived body identity (`quiddity._body_identity`): a solid's box, volume and area,
//! useful only for equality. Two solids with one signature are ambiguous, and neither gets a key.

use crate::kernel::brep::Part;
use crate::kernel::py;

/// Box corners to six decimals, then volume and area to twelve significant figures.
pub type BodyKey = Vec<f64>;

/// `body_signature`: `None` when the solid's volume or area cannot be computed.
pub fn body_signature(part: &Part, solid: usize) -> Option<BodyKey> {
    let b = part.solid_bounds(solid);
    let (volume, area) = part.solid_mass(solid)?;
    let mut key: BodyKey = b
        .min
        .iter()
        .chain(&b.max)
        .map(|&c| py::without_negative_zero(py::round_to(c, 6)))
        .collect();
    key.push(py::quantise(volume, 12));
    key.push(py::quantise(area, 12));
    Some(key)
}

/// `unambiguous_body_keys`: per solid, its signature unless another solid shares it (or, with
/// `require_valid_solid`, unless the solid is invalid).
pub fn unambiguous_body_keys(
    part: &Part,
    signatures: &[Option<BodyKey>],
    require_valid_solid: bool,
) -> Vec<Option<BodyKey>> {
    let kept: Vec<Option<&BodyKey>> = signatures
        .iter()
        .enumerate()
        .map(|(s, sig)| {
            sig.as_ref()
                .filter(|_| !require_valid_solid || part.solid_is_valid(s))
        })
        .collect();
    kept.iter()
        .map(|sig| {
            let sig = (*sig)?;
            let count = kept.iter().filter(|other| **other == Some(sig)).count();
            (count == 1).then(|| sig.clone())
        })
        .collect()
}
