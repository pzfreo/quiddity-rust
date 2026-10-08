//! Canonical analytic surface values (`quiddity._analytic_surfaces`) and the one-fact-per-face
//! effective surface they give (`_effective_surfaces.EffectiveSurfaceIndex.fact`): a native
//! plane, cylinder, cone or sphere as itself, a B-spline or Bezier face as the one primitive it
//! is certified to be (`Part::recovered`), anything else (a torus, other freeform kinds) none.

use crate::kernel::brep::Part;
use crate::kernel::geom::{COORD_FLOOR, Surface, V3};
use crate::kernel::py;

const EQUIVALENCE_REL: f64 = 1e-9;
const AXIS_GAP: f64 = 1e-9;
const ANGLE_GAP: f64 = 1e-9;

/// `SurfaceKind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SurfaceKind {
    Plane,
    Cylinder,
    Cone,
    Sphere,
}

/// `AnalyticSurfaceFact`, reduced to what its readers use. `parameters` are
/// `validated_parameters`: a plane's canonical normal and offset; a cylinder's axis point
/// nearest the origin, canonical axis and radius; a cone's apex, canonical axis and signed
/// semi-angle; a sphere's centre and radius. `native` is `SurfaceProvenance.NATIVE` (and with
/// it `NATIVE_ORIENTED`); a recovered surface is unoriented.
#[derive(Clone, Debug, PartialEq)]
pub struct AnalyticFact {
    pub kind: SurfaceKind,
    pub parameters: Vec<f64>,
    pub native: bool,
}

/// `_canonical_direction_and_sign`: the dominant component (ties to the later axis) made
/// non-negative, and the sign applied.
fn canonical_direction_and_sign(d: V3) -> (V3, f64) {
    // `max_by` keeps the last of ties, as Python's `max(key=(abs, axis))` does.
    let dominant = (0..3)
        .max_by(|&a, &b| py::order(d[a].abs(), d[b].abs()))
        .expect("three components");
    let sign = if d[dominant] >= 0.0 { 1.0 } else { -1.0 };
    (d.map(|c| sign * c), sign)
}

/// `_closest_axis_point`: the point of the axis line through *location* nearest the origin.
fn closest_axis_point(location: V3, direction: V3) -> V3 {
    let along = py::sum((0..3).map(|i| location[i] * direction[i]));
    [0, 1, 2].map(|i| location[i] - along * direction[i])
}

/// `_primitive_parameters` of a native analytic surface; `None` for any other kind.
fn primitive_parameters(surface: &Surface) -> Option<(SurfaceKind, Vec<f64>)> {
    Some(match surface {
        Surface::Plane { frame } => {
            let (direction, _) = canonical_direction_and_sign(frame.z);
            let offset = py::sum((0..3).map(|i| frame.origin[i] * direction[i]));
            (
                SurfaceKind::Plane,
                vec![direction[0], direction[1], direction[2], offset],
            )
        }
        Surface::Cylinder { frame, radius } => {
            let (direction, _) = canonical_direction_and_sign(frame.z);
            let point = closest_axis_point(frame.origin, direction);
            let mut p = point.to_vec();
            p.extend(direction);
            p.push(*radius);
            (SurfaceKind::Cylinder, p)
        }
        Surface::Cone {
            frame, semi_angle, ..
        } => {
            let (direction, sign) = canonical_direction_and_sign(frame.z);
            let mut p = surface.cone_apex()?.to_vec();
            p.extend(direction);
            p.push(sign * semi_angle);
            (SurfaceKind::Cone, p)
        }
        Surface::Sphere { frame, radius } => (
            SurfaceKind::Sphere,
            vec![frame.origin[0], frame.origin[1], frame.origin[2], *radius],
        ),
        _ => return None,
    })
}

/// `validated_parameters`: finite, a unit axis, a positive radius, a cone angle strictly
/// between zero and π/2; negative zeros made positive.
pub fn validated_parameters(surface: &Surface) -> Option<(SurfaceKind, Vec<f64>)> {
    let (kind, parameters) = primitive_parameters(surface)?;
    if parameters.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let direction = match kind {
        SurfaceKind::Plane => Some(&parameters[..3]),
        SurfaceKind::Cylinder | SurfaceKind::Cone => Some(&parameters[3..6]),
        SurfaceKind::Sphere => None,
    };
    if let Some(d) = direction
        && (py::sum(d.iter().map(|c| c * c)).sqrt() - 1.0).abs() > 1e-9
    {
        return None;
    }
    let last = *parameters.last().expect("parameters are never empty");
    match kind {
        SurfaceKind::Cylinder | SurfaceKind::Sphere if last <= 0.0 => return None,
        SurfaceKind::Cone if !(0.0 < last.abs() && last.abs() < std::f64::consts::FRAC_PI_2) => {
            return None;
        }
        _ => {}
    }
    Some((
        kind,
        parameters
            .into_iter()
            .map(py::without_negative_zero)
            .collect(),
    ))
}

/// The native analytic fact of a face's own surface (`EffectiveSurfaceIndex._derive`, native
/// branch); `None` for a torus, a freeform or an unresolved surface, or invalid parameters.
pub fn native_fact(part: &Part, face: usize) -> Option<AnalyticFact> {
    let (kind, parameters) = validated_parameters(&part.faces[face].surface)?;
    Some(AnalyticFact {
        kind,
        parameters,
        native: true,
    })
}

/// `EffectiveSurfaceIndex.fact` when it is an `AnalyticSurfaceFact`: the native fact, or for a
/// B-spline or Bezier face the one primitive it is recovered as.
pub fn effective_fact(part: &Part, face: usize) -> Option<AnalyticFact> {
    match &part.faces[face].surface {
        Surface::Freeform {
            kind: "BSPLINE" | "BEZIER",
            ..
        } => {
            let (kind, parameters) = validated_parameters(part.recovered(face)?)?;
            Some(AnalyticFact {
                kind,
                parameters,
                native: false,
            })
        }
        _ => native_fact(part, face),
    }
}

fn distance(a: &[f64], b: &[f64]) -> f64 {
    py::sum(a.iter().zip(b).map(|(x, y)| (x - y).powi(2))).sqrt()
}

fn axis_equal(a: &[f64], b: &[f64]) -> bool {
    1.0 - py::sum(a.iter().zip(b).map(|(x, y)| x * y)).abs() <= AXIS_GAP
}

/// `equivalent_parameters`: whether two validated facts of one kind prove one placed analytic
/// continuation at the local length *local* (finite and positive, else `None`: Python raises).
pub fn equivalent_parameters(
    kind: SurfaceKind,
    left: &[f64],
    right: &[f64],
    local: f64,
) -> Option<bool> {
    if !(local.is_finite() && local > 0.0) {
        return None;
    }
    let tolerance = EQUIVALENCE_REL * local + COORD_FLOOR;
    Some(match kind {
        SurfaceKind::Plane => {
            axis_equal(&left[..3], &right[..3]) && (left[3] - right[3]).abs() <= tolerance
        }
        SurfaceKind::Cylinder => {
            distance(&left[..3], &right[..3]) <= tolerance
                && axis_equal(&left[3..6], &right[3..6])
                && (left[6] - right[6]).abs() <= tolerance
        }
        SurfaceKind::Cone => {
            distance(&left[..3], &right[..3]) <= tolerance
                && axis_equal(&left[3..6], &right[3..6])
                && (left[6] - right[6]).abs() <= ANGLE_GAP
        }
        SurfaceKind::Sphere => {
            distance(&left[..3], &right[..3]) <= tolerance
                && (left[3] - right[3]).abs() <= tolerance
        }
    })
}
