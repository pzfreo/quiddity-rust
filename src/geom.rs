//! Exact analytic geometry: vectors, placements, surfaces and edge curves.
//!
//! Surface parametrisations follow OpenCascade's conventions (`gp_Cylinder`, `gp_Torus`, ...)
//! so that a parameter range read here means what `BRepAdaptor_Surface` means in the Python
//! implementation.

use std::f64::consts::{PI, TAU};

use crate::nurbs::{NurbsCurve, NurbsSurface};

pub type V3 = [f64; 3];

pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub fn scale(a: V3, k: f64) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
pub fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub fn norm(a: V3) -> f64 {
    dot(a, a).sqrt()
}
pub fn unit(a: V3) -> Option<V3> {
    let n = norm(a);
    (n.is_finite() && n > 1e-300).then(|| scale(a, 1.0 / n))
}
pub fn dist(a: V3, b: V3) -> f64 {
    norm(sub(a, b))
}

/// A right-handed (or, after a mirror, left-handed) placement: origin plus three unit axes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub origin: V3,
    pub x: V3,
    pub y: V3,
    pub z: V3,
}

impl Frame {
    pub fn to_local(&self, p: V3) -> V3 {
        let d = sub(p, self.origin);
        [dot(d, self.x), dot(d, self.y), dot(d, self.z)]
    }
    pub fn dir_to_local(&self, d: V3) -> V3 {
        [dot(d, self.x), dot(d, self.y), dot(d, self.z)]
    }
    pub fn to_world(&self, l: V3) -> V3 {
        add(
            self.origin,
            add(
                scale(self.x, l[0]),
                add(scale(self.y, l[1]), scale(self.z, l[2])),
            ),
        )
    }
    /// `gp_Ax3::Direct()`: whether the frame is right-handed.
    pub fn direct(&self) -> bool {
        dot(cross(self.x, self.y), self.z) > 0.0
    }
}

/// The analytic surface kinds the recognisers read. Anything else is carried by name so a
/// caller can refuse it explicitly rather than mistake it for a supported surface.
#[derive(Clone, Debug)]
pub enum Surface {
    Plane {
        frame: Frame,
    },
    Cylinder {
        frame: Frame,
        radius: f64,
    },
    Cone {
        frame: Frame,
        radius: f64,
        semi_angle: f64,
    },
    Sphere {
        frame: Frame,
        radius: f64,
    },
    Torus {
        frame: Frame,
        major: f64,
        minor: f64,
    },
    /// Any other surface, in exact NURBS form; *kind* names what the file called it.
    Freeform {
        kind: &'static str,
        surface: Box<NurbsSurface>,
    },
    /// A surface that could not be resolved at all.
    Other {
        kind: &'static str,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceType {
    Plane,
    Cylinder,
    Cone,
    Sphere,
    Torus,
    Freeform,
    Other,
}

impl Surface {
    pub fn kind(&self) -> SurfaceType {
        match self {
            Surface::Plane { .. } => SurfaceType::Plane,
            Surface::Cylinder { .. } => SurfaceType::Cylinder,
            Surface::Cone { .. } => SurfaceType::Cone,
            Surface::Sphere { .. } => SurfaceType::Sphere,
            Surface::Torus { .. } => SurfaceType::Torus,
            Surface::Freeform { .. } => SurfaceType::Freeform,
            Surface::Other { .. } => SurfaceType::Other,
        }
    }

    pub fn frame(&self) -> Option<&Frame> {
        match self {
            Surface::Plane { frame }
            | Surface::Cylinder { frame, .. }
            | Surface::Cone { frame, .. }
            | Surface::Sphere { frame, .. }
            | Surface::Torus { frame, .. } => Some(frame),
            Surface::Freeform { .. } | Surface::Other { .. } => None,
        }
    }

    /// (u periodic, v periodic) — both with period 2π where periodic.
    pub fn periodic(&self) -> (bool, bool) {
        match self {
            Surface::Plane { .. } | Surface::Freeform { .. } | Surface::Other { .. } => {
                (false, false)
            }
            Surface::Cylinder { .. } | Surface::Cone { .. } | Surface::Sphere { .. } => {
                (true, false)
            }
            Surface::Torus { .. } => (true, true),
        }
    }

    pub fn value(&self, u: f64, v: f64) -> V3 {
        match *self {
            Surface::Plane { frame } => frame.to_world([u, v, 0.0]),
            Surface::Cylinder { frame, radius } => {
                frame.to_world([radius * u.cos(), radius * u.sin(), v])
            }
            Surface::Cone {
                frame,
                radius,
                semi_angle,
            } => {
                let rho = radius + v * semi_angle.sin();
                frame.to_world([rho * u.cos(), rho * u.sin(), v * semi_angle.cos()])
            }
            Surface::Sphere { frame, radius } => frame.to_world([
                radius * v.cos() * u.cos(),
                radius * v.cos() * u.sin(),
                radius * v.sin(),
            ]),
            Surface::Torus {
                frame,
                major,
                minor,
            } => {
                let rho = major + minor * v.cos();
                frame.to_world([rho * u.cos(), rho * u.sin(), minor * v.sin()])
            }
            Surface::Freeform { ref surface, .. } => surface.value(u, v),
            Surface::Other { .. } => [f64::NAN; 3],
        }
    }

    /// The (u, v) parameters of a point on (or near) the surface. Periodic parameters are
    /// returned in `[0, 2π)`; callers unwrap them. *hint* seeds the iterative inversion of a
    /// freeform surface (the analytic kinds invert in closed form and ignore it).
    pub fn parameters(&self, p: V3, hint: Option<(f64, f64)>) -> Option<(f64, f64)> {
        let angle = |l: V3| l[1].atan2(l[0]).rem_euclid(TAU);
        match *self {
            Surface::Plane { frame } => {
                let l = frame.to_local(p);
                Some((l[0], l[1]))
            }
            Surface::Cylinder { frame, .. } => {
                let l = frame.to_local(p);
                Some((angle(l), l[2]))
            }
            Surface::Cone {
                frame,
                radius,
                semi_angle,
            } => {
                let l = frame.to_local(p);
                let rho = l[0].hypot(l[1]);
                Some((
                    angle(l),
                    (rho - radius) * semi_angle.sin() + l[2] * semi_angle.cos(),
                ))
            }
            Surface::Sphere { frame, .. } => {
                let l = frame.to_local(p);
                let rho = l[0].hypot(l[1]);
                Some((angle(l), l[2].atan2(rho)))
            }
            Surface::Torus { frame, major, .. } => {
                let l = frame.to_local(p);
                let rho = l[0].hypot(l[1]);
                Some((angle(l), l[2].atan2(rho - major).rem_euclid(TAU)))
            }
            Surface::Freeform { ref surface, .. } => Some(surface.invert(p, hint)),
            Surface::Other { .. } => None,
        }
    }

    /// Whether (u, v) is a singular point of the parametrisation — a sphere's pole or a cone's
    /// apex — where u is undefined and a boundary through it runs along the whole v line.
    pub fn is_singular(&self, _u: f64, v: f64) -> bool {
        match *self {
            Surface::Sphere { .. } => (v.abs() - PI / 2.0).abs() < 1e-7,
            Surface::Cone {
                radius, semi_angle, ..
            } => (radius + v * semi_angle.sin()).abs() < 1e-7 * (1.0 + radius),
            _ => false,
        }
    }

    /// Parameters of the points where a doubly-curved surface is extreme along a world axis —
    /// the only interior points a face's bounding box can reach beyond its boundary.
    pub fn axis_extreme_parameters(&self) -> Vec<(f64, f64)> {
        let mut out = Vec::new();
        let axes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        match *self {
            Surface::Sphere { frame, .. } => {
                for a in axes {
                    for sign in [1.0, -1.0] {
                        let w = frame.dir_to_local(scale(a, sign));
                        out.push((
                            w[1].atan2(w[0]).rem_euclid(TAU),
                            w[2].clamp(-1.0, 1.0).asin(),
                        ));
                    }
                }
            }
            Surface::Torus { frame, .. } => {
                for a in axes {
                    let w = frame.dir_to_local(a);
                    if w[0].hypot(w[1]) < 1e-12 {
                        // Along the axis the extremes are whole circles at the tube's top and bottom.
                        for k in 0..16 {
                            let u = TAU * k as f64 / 16.0;
                            out.push((u, PI / 2.0));
                            out.push((u, 3.0 * PI / 2.0));
                        }
                        continue;
                    }
                    for u in [w[1].atan2(w[0]), w[1].atan2(w[0]) + PI] {
                        let v = w[2].atan2(w[0] * u.cos() + w[1] * u.sin());
                        out.push((u.rem_euclid(TAU), v.rem_euclid(TAU)));
                        out.push((u.rem_euclid(TAU), (v + PI).rem_euclid(TAU)));
                    }
                }
            }
            _ => {}
        }
        out
    }

    /// Ray parameters `t > 0` where `origin + t * dir` meets the untrimmed surface, plus a flag
    /// that is set when a hit is tangential (and so cannot be counted reliably).
    pub fn ray_hits(&self, origin: V3, dir: V3, t_max: f64) -> Option<(Vec<f64>, bool)> {
        let mut hits = Vec::new();
        let mut grazing = false;
        let mut quadratic = |a: f64, b: f64, c: f64| {
            if a.abs() < 1e-300 {
                if b.abs() > 1e-300 {
                    hits.push(-c / b);
                }
                return;
            }
            let disc = b * b - 4.0 * a * c;
            let scale_ = (b * b).max((4.0 * a * c).abs()).max(1e-300);
            if disc.abs() <= 1e-12 * scale_ {
                grazing = true;
                hits.push(-b / (2.0 * a));
            } else if disc > 0.0 {
                let s = disc.sqrt();
                let q = -0.5 * (b + b.signum() * s);
                hits.push(q / a);
                if q.abs() > 1e-300 {
                    hits.push(c / q);
                }
            }
        };
        match *self {
            Surface::Plane { frame } => {
                let denom = dot(dir, frame.z);
                if denom.abs() < 1e-12 {
                    return Some((vec![], dot(sub(origin, frame.origin), frame.z).abs() < 1e-9));
                }
                hits.push(dot(sub(frame.origin, origin), frame.z) / denom);
            }
            Surface::Cylinder { frame, radius } => {
                let o = frame.to_local(origin);
                let d = frame.dir_to_local(dir);
                quadratic(
                    d[0] * d[0] + d[1] * d[1],
                    2.0 * (o[0] * d[0] + o[1] * d[1]),
                    o[0] * o[0] + o[1] * o[1] - radius * radius,
                );
            }
            Surface::Cone {
                frame,
                radius,
                semi_angle,
            } => {
                let o = frame.to_local(origin);
                let d = frame.dir_to_local(dir);
                let k = semi_angle.tan();
                // x² + y² = (r + z k)²
                let (rz0, rz1) = (radius + o[2] * k, d[2] * k);
                quadratic(
                    d[0] * d[0] + d[1] * d[1] - rz1 * rz1,
                    2.0 * (o[0] * d[0] + o[1] * d[1] - rz0 * rz1),
                    o[0] * o[0] + o[1] * o[1] - rz0 * rz0,
                );
                // Only the nappe the parametrisation covers (non-negative radius).
                hits.retain(|t| radius + (o[2] + t * d[2]) * k >= -1e-12);
            }
            Surface::Sphere { frame, radius } => {
                let o = sub(origin, frame.origin);
                quadratic(
                    dot(dir, dir),
                    2.0 * dot(o, dir),
                    dot(o, o) - radius * radius,
                );
            }
            Surface::Torus {
                frame,
                major,
                minor,
            } => {
                let o = frame.to_local(origin);
                let d = frame.dir_to_local(dir);
                let f = |t: f64| {
                    let p = [o[0] + t * d[0], o[1] + t * d[1], o[2] + t * d[2]];
                    let s = dot(p, p) + major * major - minor * minor;
                    s * s - 4.0 * major * major * (p[0] * p[0] + p[1] * p[1])
                };
                // Bracket the quartic's roots inside the bounding sphere, then bisect.
                let reach = major + minor;
                let b = dot(o, d);
                let c = dot(o, o) - reach * reach;
                let disc = b * b - c;
                if disc > 0.0 {
                    let (t0, t1) = ((-b - disc.sqrt()).max(0.0), (-b + disc.sqrt()).min(t_max));
                    if t1 > t0 {
                        let steps = 4000;
                        let h = (t1 - t0) / steps as f64;
                        let mut prev_t = t0;
                        let mut prev_f = f(t0);
                        for i in 1..=steps {
                            let t = t0 + h * i as f64;
                            let ft = f(t);
                            if prev_f == 0.0 {
                                hits.push(prev_t);
                            } else if prev_f.signum() != ft.signum() {
                                let (mut lo, mut hi, mut flo) = (prev_t, t, prev_f);
                                for _ in 0..80 {
                                    let mid = 0.5 * (lo + hi);
                                    let fm = f(mid);
                                    if fm.signum() == flo.signum() {
                                        lo = mid;
                                        flo = fm;
                                    } else {
                                        hi = mid;
                                    }
                                }
                                hits.push(0.5 * (lo + hi));
                            }
                            prev_t = t;
                            prev_f = ft;
                        }
                    }
                }
            }
            Surface::Freeform { ref surface, .. } => {
                let (found, graze) = surface.ray_hits(origin, dir, t_max);
                grazing |= graze;
                hits.extend(found.into_iter().map(|h| h.0));
            }
            Surface::Other { .. } => return None,
        }
        hits.retain(|t| *t > 1e-12 && *t <= t_max);
        Some((hits, grazing))
    }
}

/// The curve under an edge.
#[derive(Clone, Debug, PartialEq)]
pub enum Curve {
    Line {
        origin: V3,
        dir: V3,
    },
    Circle {
        frame: Frame,
        radius: f64,
    },
    Ellipse {
        frame: Frame,
        major: f64,
        minor: f64,
    },
    /// Any other curve, already bounded to the edge and running start → end.
    Nurbs(NurbsCurve),
    Other {
        kind: &'static str,
    },
}

impl Curve {
    pub fn value(&self, t: f64) -> V3 {
        match self {
            Curve::Line { origin, dir } => add(*origin, scale(*dir, t)),
            Curve::Circle { frame, radius } => {
                frame.to_world([radius * t.cos(), radius * t.sin(), 0.0])
            }
            Curve::Ellipse {
                frame,
                major,
                minor,
            } => frame.to_world([major * t.cos(), minor * t.sin(), 0.0]),
            Curve::Nurbs(n) => n.value(t),
            Curve::Other { .. } => [f64::NAN; 3],
        }
    }

    /// The curve parameter of a point on (or near) the curve.
    pub fn parameter(&self, p: V3) -> f64 {
        match self {
            Curve::Line { origin, dir } => dot(sub(p, *origin), *dir) / dot(*dir, *dir),
            Curve::Circle { frame, .. } => {
                let l = frame.to_local(p);
                l[1].atan2(l[0]).rem_euclid(TAU)
            }
            Curve::Ellipse {
                frame,
                major,
                minor,
            } => {
                let l = frame.to_local(p);
                (l[1] / minor).atan2(l[0] / major).rem_euclid(TAU)
            }
            Curve::Nurbs(_) | Curve::Other { .. } => f64::NAN,
        }
    }

    pub fn is_periodic(&self) -> bool {
        matches!(self, Curve::Circle { .. } | Curve::Ellipse { .. })
    }
}

/// Wrap `a` into `[reference - π, reference + π)`.
pub fn nearest_turn(a: f64, reference: f64) -> f64 {
    a - TAU * ((a - reference + PI) / TAU).floor()
}

/// Python's `round(value, 3)`: correctly rounded on the exact binary value.
pub fn round3(value: f64) -> f64 {
    format!("{value:.3}")
        .parse()
        .expect("formatted float parses")
}

/// `quiddity._geometry.quantise`: *value* to six significant figures.
pub fn quantise(value: f64) -> f64 {
    format!("{value:.5e}")
        .parse()
        .expect("formatted float parses")
}

/// `quiddity._geometry.length_tol`.
pub fn length_tol(nominal: f64, rel: f64) -> f64 {
    rel * nominal + COORD_FLOOR
}

pub const COORD_FLOOR: f64 = 1e-6;
pub const AXIS_ALIGNED_COS: f64 = 0.99;
pub const INTERIOR_PROBE_FRAC: f64 = 0.05;
