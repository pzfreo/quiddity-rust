//! The legacy `Passage` compatibility view (`quiddity._passage_compat`): the principal-axis
//! reading of a section passage that the frozen pre-0.4 `Passage` value and the slot grouping
//! are read from. Never public record geometry.
//!
//! Python revalidates its frozen view on every read (`issued_snapshot`); here the fields are
//! private and the view is built only through [`PassageCompatibilityView::new`], so a read
//! cannot see an invalid one.

use super::passages::{Passage, PassageError};
use super::sections::V2;
use crate::kernel::geom::V3;
use crate::kernel::py;

/// The principal legacy geometry of an occurrence (`PrincipalProjection`): axis letter, side
/// count, length and centre to 3 decimals, and the section's corners in the two other axes to 3
/// decimals, canonical.
#[derive(Clone, Debug, PartialEq)]
pub struct PrincipalProjection {
    pub axis: &'static str,
    pub sides: usize,
    pub length: f64,
    pub at: V3,
    pub section: Vec<V2>,
}

/// Historical eligibility and slot grouping, frozen before issuance
/// (`PassageCompatibilityView`): an eligible view carries a complete legacy value, an
/// ineligible one only its grouping key.
#[derive(Clone, Debug, PartialEq)]
pub struct PassageCompatibilityView {
    axis: Option<&'static str>,
    section: Option<Vec<V2>>,
    sides: Option<usize>,
    length: Option<f64>,
    at: Option<V3>,
    legacy_ordinal: Option<usize>,
    eligible: bool,
}

impl PassageCompatibilityView {
    /// The view, refused with Python's messages when its parts are inconsistent.
    pub fn new(
        axis: Option<&'static str>,
        section: Option<Vec<V2>>,
        sides: Option<usize>,
        length: Option<f64>,
        at: Option<V3>,
        legacy_ordinal: Option<usize>,
        eligible: bool,
    ) -> Result<Self, PassageError> {
        if axis.is_some_and(|a| !["x", "y", "z"].contains(&a)) {
            return Err(PassageError("passage compatibility axis is invalid"));
        }
        if axis.is_none() != section.is_none() {
            return Err(PassageError(
                "passage grouping axis and section must be present together",
            ));
        }
        let legacy_complete = length.is_some() && at.is_some() && legacy_ordinal.is_some();
        let legacy_any = length.is_some() || at.is_some() || legacy_ordinal.is_some();
        if eligible && (axis.is_none() || sides.is_none() || !legacy_complete) {
            return Err(PassageError(
                "eligible passage compatibility requires a complete legacy value",
            ));
        }
        if !eligible && legacy_any {
            return Err(PassageError(
                "ineligible passage compatibility cannot carry a legacy value",
            ));
        }
        if axis.is_none() && sides.is_some() {
            return Err(PassageError(
                "passage grouping sides require a principal grouping key",
            ));
        }
        Ok(PassageCompatibilityView {
            axis,
            section,
            sides,
            length,
            at,
            legacy_ordinal,
            eligible,
        })
    }

    pub fn eligible(&self) -> bool {
        self.eligible
    }

    pub fn legacy_ordinal(&self) -> Option<usize> {
        self.legacy_ordinal
    }
}

/// The principal legacy view of an occurrence's primitives (`principal_projection`): `None`
/// unless the run is a principal axis (to 1e-12, positive) and the rounded section is a
/// polygon.
pub fn principal_projection(
    origin: V3,
    run: V3,
    u: V3,
    v: V3,
    interval: (f64, f64),
    boundary: &[V2],
) -> Option<PrincipalProjection> {
    let axes: [(V3, &'static str, usize, usize); 3] = [
        ([1.0, 0.0, 0.0], "x", 1, 2),
        ([0.0, 1.0, 0.0], "y", 0, 2),
        ([0.0, 0.0, 1.0], "z", 0, 1),
    ];
    let &(_, axis, first, second) = axes.iter().find(|(expected, ..)| {
        run.iter()
            .zip(expected)
            .all(|(value, target)| (value - target).abs() <= 1e-12)
    })?;
    let (lo, hi) = interval;
    let mut centre = origin;
    centre[["x", "y", "z"]
        .iter()
        .position(|&a| a == axis)
        .expect("an axis")] = 0.5 * (lo + hi);
    let section: Vec<V2> = boundary
        .iter()
        .map(|point| {
            [first, second].map(|i| py::round_to(origin[i] + point[0] * u[i] + point[1] * v[i], 3))
        })
        .collect();
    let section = canonical_section(&section)?;
    Some(PrincipalProjection {
        axis,
        sides: section.len(),
        length: py::round_to(hi - lo, 3),
        at: centre.map(|c| py::round_to(c, 3)),
        section,
    })
}

/// Freeze one eligible legacy value or ineligible grouping fact (`compatibility_view`).
pub fn compatibility_view(
    projection: Option<&PrincipalProjection>,
    eligible: bool,
    legacy_ordinal: Option<usize>,
) -> Result<PassageCompatibilityView, PassageError> {
    let Some(p) = projection else {
        if eligible {
            return Err(PassageError(
                "eligible passage has no principal compatibility projection",
            ));
        }
        return PassageCompatibilityView::new(None, None, None, None, None, None, false);
    };
    PassageCompatibilityView::new(
        Some(p.axis),
        Some(p.section.clone()),
        Some(p.sides),
        eligible.then_some(p.length),
        eligible.then_some(p.at),
        legacy_ordinal.filter(|_| eligible),
        eligible,
    )
}

/// The public legacy value of an eligible view (`passage_from_view`).
pub fn passage_from_view(view: &PassageCompatibilityView) -> Result<Passage, PassageError> {
    let (true, Some(axis), Some(section), Some(sides), Some(length), Some(at)) = (
        view.eligible,
        view.axis,
        view.section.clone(),
        view.sides,
        view.length,
        view.at,
    ) else {
        return Err(PassageError(
            "ineligible compatibility fact has no legacy Passage",
        ));
    };
    Ok(Passage {
        axis: axis.into(),
        sides,
        length,
        at,
        section,
    })
}

/// The slot-reconciliation grouping of a view (`grouping_from_view`): axis, section and side
/// count, where it has them.
pub fn grouping_from_view(view: &PassageCompatibilityView) -> Option<(&'static str, &[V2], usize)> {
    Some((view.axis?, view.section.as_deref()?, view.sides?))
}

/// A section anticlockwise from its least corner (`_canonical_section`); `None` under three
/// corners, for a repeated corner, or for an area within 1e-12 of zero.
pub fn canonical_section(section: &[V2]) -> Option<Vec<V2>> {
    let n = section.len();
    if n < 3 || (0..n).any(|i| section[..i].contains(&section[i])) {
        return None;
    }
    let area = py::sum((0..n).map(|i| {
        let (left, right) = (section[i], section[(i + 1) % n]);
        left[0] * right[1] - right[0] * left[1]
    }));
    if area.abs() <= 1e-12 {
        return None;
    }
    let mut oriented = section.to_vec();
    if area <= 0.0 {
        oriented.reverse();
    }
    let start = (1..n).fold(0, |best, i| {
        if py::tuple_order(&oriented[i], &oriented[best]).is_lt() {
            i
        } else {
            best
        }
    });
    oriented.rotate_left(start);
    Some(oriented)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_view_refuses_inconsistent_parts() {
        let square = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        assert!(
            PassageCompatibilityView::new(Some("w"), None, None, None, None, None, false).is_err()
        );
        assert!(
            PassageCompatibilityView::new(Some("x"), None, None, None, None, None, false).is_err()
        );
        assert!(
            PassageCompatibilityView::new(
                Some("x"),
                Some(square.clone()),
                Some(4),
                None,
                None,
                None,
                true
            )
            .is_err()
        );
        assert!(
            PassageCompatibilityView::new(None, None, None, Some(1.0), None, None, false).is_err()
        );
        assert!(
            PassageCompatibilityView::new(None, None, Some(4), None, None, None, false).is_err()
        );
        let view = compatibility_view(None, false, None).unwrap();
        assert!(passage_from_view(&view).is_err());
        assert_eq!(grouping_from_view(&view), None);
    }

    #[test]
    fn the_projection_reads_a_principal_run_only() {
        let boundary = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];
        let z = principal_projection(
            [5.0, 6.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            (0.0, 33.245),
            &boundary,
        )
        .unwrap();
        assert_eq!(z.axis, "z");
        assert_eq!(z.sides, 4);
        assert_eq!(z.length, 33.245);
        // The half-quantum midpoint 16.6225 rounds to 16.622 on its binary value.
        assert_eq!(z.at, [5.0, 6.0, py::round_to(16.6225, 3)]);
        assert_eq!(z.section[0], [4.0, 5.0]);
        let tilted = principal_projection(
            [0.0; 3],
            [0.0, 0.6, 0.8],
            [1.0, 0.0, 0.0],
            [0.0, 0.8, -0.6],
            (0.0, 1.0),
            &boundary,
        );
        assert_eq!(tilted, None);
    }
}
