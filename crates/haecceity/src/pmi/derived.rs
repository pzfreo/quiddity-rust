//! Conservative analytic derivation. All distances here are millimetres.
use crate::geom::{V3, add, cross, dot, norm, scale, sub, unit};

use super::model::DerivedGeometryKind;

/// Fixed comparison floors; representation length uncertainty may increase the distance bound.
pub(super) const DISTANCE_TOLERANCE: f64 = 1e-7;
pub(super) const DIRECTION_TOLERANCE: f64 = 1e-9;

#[derive(Clone, Copy, Debug)]
pub(super) enum SurfaceKind {
    Cylinder(f64),
    Revolution,
    Plane,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Surface {
    pub kind: SurfaceKind,
    pub point: V3,
    pub direction: V3,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Derived {
    pub kind: DerivedGeometryKind,
    pub point: V3,
    pub direction: V3,
    pub radius: Option<f64>,
}

fn parallel(a: V3, b: V3) -> bool {
    norm(cross(a, b)) <= DIRECTION_TOLERANCE
}

fn line_distance(point: V3, origin: V3, direction: V3) -> f64 {
    norm(cross(sub(point, origin), direction))
}

pub(super) fn derive(faces: &[Surface], tolerance: f64) -> Result<Derived, String> {
    let Some(first) = faces.first() else {
        return Err(
            "no deriving faces; explicit geometry alone cannot establish a derivation".into(),
        );
    };
    let mut faces = faces.to_vec();
    for f in &mut faces {
        if f.point.iter().any(|v| !v.is_finite()) {
            return Err("a deriving surface has a non-finite point".into());
        }
        f.direction = unit(f.direction).ok_or("a deriving surface has an invalid direction")?;
        if let SurfaceKind::Cylinder(r) = f.kind
            && (!r.is_finite() || r <= 0.0)
        {
            return Err("a deriving cylinder has an invalid radius".into());
        }
    }
    let direction = faces[0].direction;
    let point = first.point;
    let planes = faces
        .iter()
        .filter(|f| matches!(f.kind, SurfaceKind::Plane))
        .count();
    if planes != 0 {
        if planes != faces.len() {
            return Err("mixed planar and revolution deriving faces".into());
        }
        if faces.len() != 2 {
            return Err(format!(
                "median plane requires exactly two parallel planar deriving faces; found {}",
                faces.len()
            ));
        }
        if !parallel(direction, faces[1].direction) {
            return Err("non-parallel planar deriving faces".into());
        }
        let separation = dot(sub(faces[1].point, point), direction);
        return Ok(Derived {
            kind: DerivedGeometryKind::MedianPlane,
            point: {
                let midpoint = add(point, scale(direction, separation * 0.5));
                if midpoint.iter().any(|v| !v.is_finite()) {
                    return Err("computed median plane is non-finite".into());
                }
                midpoint
            },
            direction,
            radius: None,
        });
    }
    for f in &faces[1..] {
        if !parallel(direction, f.direction) || line_distance(f.point, point, direction) > tolerance
        {
            return Err("non-coaxial revolution deriving faces".into());
        }
    }
    let radius = match faces[0].kind {
        SurfaceKind::Cylinder(r)
            if faces.iter().all(
                |f| matches!(f.kind, SurfaceKind::Cylinder(s) if (s - r).abs() <= tolerance),
            ) =>
        {
            Some(r)
        }
        _ => None,
    };
    Ok(Derived {
        kind: DerivedGeometryKind::Axis,
        point,
        direction,
        radius,
    })
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Explicit {
    Point(V3),
    Line { point: V3, direction: V3 },
    Placement { point: V3, direction: V3 },
    Plane { point: V3, normal: V3 },
}

impl Derived {
    pub fn cross_check(self, explicit: Explicit, tolerance: f64) -> Result<(), String> {
        let on = |point| match self.kind {
            DerivedGeometryKind::Axis => line_distance(point, self.point, self.direction),
            DerivedGeometryKind::MedianPlane => dot(sub(point, self.point), self.direction).abs(),
        };
        let (point, orientation) = match explicit {
            Explicit::Point(p) => (p, None),
            Explicit::Line { point, direction } => {
                let d = unit(direction).ok_or("explicit axis/line has an invalid direction")?;
                let agrees = match self.kind {
                    DerivedGeometryKind::Axis => parallel(self.direction, d),
                    DerivedGeometryKind::MedianPlane => {
                        dot(self.direction, d).abs() <= DIRECTION_TOLERANCE
                    }
                };
                (point, Some(agrees))
            }
            Explicit::Placement { point, direction } => {
                let d = unit(direction).ok_or("explicit placement has an invalid direction")?;
                (point, Some(parallel(self.direction, d)))
            }
            Explicit::Plane { point, normal } => {
                let n = unit(normal).ok_or("explicit plane has an invalid normal")?;
                let agrees = match self.kind {
                    DerivedGeometryKind::Axis => {
                        return Err("explicit plane disagrees with derived axis: incompatible geometry kinds".into());
                    }
                    DerivedGeometryKind::MedianPlane => parallel(self.direction, n),
                };
                (point, Some(agrees))
            }
        };
        if point.iter().any(|v| !v.is_finite()) {
            return Err("explicit geometry has a non-finite point".into());
        }
        let distance = on(point);
        if !distance.is_finite() {
            return Err("explicit geometry comparison is non-finite".into());
        }
        if distance > tolerance {
            return Err(format!(
                "explicit geometry disagrees with derived {}: distance {distance:.12e} mm exceeds tolerance {tolerance:.12e} mm",
                self.kind_name()
            ));
        }
        if orientation == Some(false) {
            return Err(format!(
                "explicit geometry disagrees with derived {}: direction/normal disagreement (sine tolerance {DIRECTION_TOLERANCE:.1e})",
                self.kind_name()
            ));
        }
        Ok(())
    }

    fn kind_name(self) -> &'static str {
        match self.kind {
            DerivedGeometryKind::Axis => "axis",
            DerivedGeometryKind::MedianPlane => "median plane",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const Z: V3 = [0., 0., 1.];
    fn surface(kind: SurfaceKind, point: V3) -> Surface {
        Surface {
            kind,
            point,
            direction: Z,
        }
    }
    fn cylinder(z: f64, r: f64) -> Surface {
        surface(SurfaceKind::Cylinder(r), [0., 0., z])
    }
    #[test]
    fn coaxial_revolution_faces_and_one_cylinder_radius() {
        let a = cylinder(0., 2.);
        let mut b = cylinder(9., 2.);
        b.direction = [0., 0., -7.];
        let d = derive(&[a, b], DISTANCE_TOLERANCE).unwrap();
        assert_eq!(d.kind, DerivedGeometryKind::Axis);
        assert_eq!(d.direction, Z);
        assert_eq!(d.radius, Some(2.));
        assert_eq!(
            derive(&[a, cylinder(9., 3.)], DISTANCE_TOLERANCE)
                .unwrap()
                .radius,
            None
        );
        assert_eq!(
            derive(
                &[a, surface(SurfaceKind::Revolution, [0., 0., 7.])],
                DISTANCE_TOLERANCE
            )
            .unwrap()
            .radius,
            None
        );
        b.point[0] = 0.01;
        assert!(
            derive(&[a, b], DISTANCE_TOLERANCE)
                .unwrap_err()
                .contains("non-coaxial")
        );
        b.point[0] = 0.;
        b.direction = [1., 0., 0.];
        assert!(
            derive(&[a, b], DISTANCE_TOLERANCE)
                .unwrap_err()
                .contains("non-coaxial")
        );
    }
    #[test]
    fn two_parallel_planes_and_refusals() {
        let a = surface(SurfaceKind::Plane, [1., 2., 3.]);
        let mut b = surface(SurfaceKind::Plane, [9., 8., 7.]);
        b.direction = [0., 0., -1.];
        let d = derive(&[a, b], DISTANCE_TOLERANCE).unwrap();
        assert_eq!(d.kind, DerivedGeometryKind::MedianPlane);
        assert_eq!(d.point, [1., 2., 5.]);
        assert_eq!(d.radius, None);
        b.direction = [0., 1., 0.];
        assert!(
            derive(&[a, b], DISTANCE_TOLERANCE)
                .unwrap_err()
                .contains("non-parallel")
        );
        assert!(
            derive(&[a, cylinder(0., 2.)], DISTANCE_TOLERANCE)
                .unwrap_err()
                .contains("mixed")
        );
        assert!(
            derive(&[a], DISTANCE_TOLERANCE)
                .unwrap_err()
                .contains("exactly two")
        );
        assert!(
            derive(&[], DISTANCE_TOLERANCE)
                .unwrap_err()
                .contains("no deriving faces")
        );
    }
    #[test]
    fn explicit_points_axes_lines_and_planes_agree_or_disagree() {
        let axis = derive(&[cylinder(0., 2.)], DISTANCE_TOLERANCE).unwrap();
        for e in [
            Explicit::Point([0., 0., 9.]),
            Explicit::Line {
                point: [0., 0., 4.],
                direction: [0., 0., -3.],
            },
            Explicit::Placement {
                point: [0., 0., 4.],
                direction: Z,
            },
        ] {
            axis.cross_check(e, DISTANCE_TOLERANCE).unwrap();
        }
        for e in [
            Explicit::Point([2., 0., 0.]),
            Explicit::Line {
                point: [0., 0., 4.],
                direction: [1., 0., 0.],
            },
            Explicit::Plane {
                point: [0., 0., 4.],
                normal: Z,
            },
        ] {
            assert!(axis.cross_check(e, DISTANCE_TOLERANCE).is_err());
        }
        let plane = derive(
            &[
                surface(SurfaceKind::Plane, [0.; 3]),
                surface(SurfaceKind::Plane, [0., 0., 4.]),
            ],
            DISTANCE_TOLERANCE,
        )
        .unwrap();
        for e in [
            Explicit::Point([7., 9., 2.]),
            Explicit::Line {
                point: [0., 0., 2.],
                direction: [1., 0., 0.],
            },
            Explicit::Plane {
                point: [3., 4., 2.],
                normal: [0., 0., -2.],
            },
        ] {
            plane.cross_check(e, DISTANCE_TOLERANCE).unwrap();
        }
        assert!(
            plane
                .cross_check(Explicit::Point([0., 0., 0.]), DISTANCE_TOLERANCE)
                .is_err()
        );
        axis.cross_check(
            Explicit::Point([DISTANCE_TOLERANCE * 0.5, 0., 0.]),
            DISTANCE_TOLERANCE,
        )
        .unwrap();
        assert!(
            axis.cross_check(
                Explicit::Point([DISTANCE_TOLERANCE * 2., 0., 0.]),
                DISTANCE_TOLERANCE
            )
            .is_err()
        );
    }
}
