//! Geometry-derived body identity (`quiddity._body_identity`): a solid's box, volume and area,
//! useful only for equality. Two solids with one signature are ambiguous, and neither gets a key.

use super::placed::{Grid, Placed};
use crate::kernel::brep::Part;

/// Box corners to six decimals, then volume and area to twelve significant figures
/// ([`Grid::BodyKey`]).
pub type BodyKey = Placed<Vec<f64>>;

internal_entry! {
/// `body_signature`: `None` when the solid's volume or area cannot be computed.
pub fn body_signature(part: &Part, solid: usize) -> Option<BodyKey> {
    let b = part.solid_bounds(solid);
    let (volume, area) = part.solid_mass(solid)?;
    let mut key: Vec<f64> = b.min.iter().chain(&b.max).copied().collect();
    key.push(volume);
    key.push(area);
    Some(Placed::new(key, Grid::BodyKey))
}
}

internal_entry! {
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
}
