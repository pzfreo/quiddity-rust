//! A face's trimmed region in its surface's parameter space, and point containment in it.

use std::f64::consts::TAU;

use crate::brep::UvLoop;
use crate::geom;

/// The face's boundary as (u, v) polylines.
///
/// Containment is decided by crossing parity, which needs no loop orientation. That matters:
/// OpenCascade re-derives wire orientation from geometry when it reads a file, and the files it
/// writes carry `FACE_BOUND` flags its own reader then corrects, so the flags are not evidence.
/// Parity does need every loop closed in parameter space; [`crate::brep::Part::uv_loops`]
/// closes a loop that runs round a sphere through its pole.
#[derive(Debug)]
pub struct FaceDomain {
    loops: Vec<Vec<(f64, f64)>>,
    periodic: (bool, bool),
    u_range: (f64, f64),
    v_range: (f64, f64),
}

fn range<'a>(
    points: impl Iterator<Item = &'a (f64, f64)>,
    pick: impl Fn(&(f64, f64)) -> f64,
) -> (f64, f64) {
    points
        .map(pick)
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| {
            (lo.min(x), hi.max(x))
        })
}

impl FaceDomain {
    pub fn new(loops: &[UvLoop], periodic: (bool, bool)) -> Option<Self> {
        let all = || loops.iter().flat_map(|l| l.points.iter());
        Some(FaceDomain {
            u_range: range(all(), |p| p.0),
            v_range: range(all(), |p| p.1),
            loops: loops.iter().map(|l| l.points.clone()).collect(),
            periodic,
        })
    }

    pub fn u_range(&self) -> (f64, f64) {
        self.u_range
    }

    pub fn v_range(&self) -> (f64, f64) {
        self.v_range
    }

    /// Whether (u, v) lies on the face, by the parity of a parameter-space ray.
    ///
    /// The ray runs in +v. A periodic v needs a finite end outside the face's v range; a
    /// periodic u wraps every segment next to the point. A face closed in v but open in u
    /// casts in +u instead, and a face closed in both is the whole surface.
    pub fn contains(&self, u: f64, v: f64) -> bool {
        let (pu, pv) = self.periodic;
        let (vmin, vmax) = self.v_range;
        let (umin, umax) = self.u_range;
        if pv && vmax - vmin >= TAU - 1e-9 {
            if !pu || umax - umin >= TAU - 1e-9 {
                return true;
            }
            let u = umin + (u - umin).rem_euclid(TAU);
            if u > umax {
                return false;
            }
            return crossing_parity(
                &self.loops,
                v,
                u,
                true,
                true,
                umax + 0.5 * (TAU - (umax - umin)),
            );
        }
        let mut v = v;
        let mut v_end = f64::INFINITY;
        if pv {
            v = vmin + (v - vmin).rem_euclid(TAU);
            if v > vmax {
                return false;
            }
            v_end = vmax + 0.5 * (TAU - (vmax - vmin));
        }
        let u = if pu { u.rem_euclid(TAU) } else { u };
        crossing_parity(&self.loops, u, v, pu, false, v_end)
    }
}

/// Parity of the crossings of the parameter-space ray `(a, b) → (a, b_end)` with the loops,
/// where `a` is each loop point's first coordinate (its second when `swap`). `periodic_a` wraps
/// each segment next to `a`.
fn crossing_parity(
    loops: &[Vec<(f64, f64)>],
    a: f64,
    b: f64,
    periodic_a: bool,
    swap: bool,
    b_end: f64,
) -> bool {
    let pick = |p: &(f64, f64)| if swap { (p.1, p.0) } else { (p.0, p.1) };
    let mut count = 0usize;
    for lp in loops {
        for w in lp.windows(2) {
            let (mut a0, b0) = pick(&w[0]);
            let (mut a1, b1) = pick(&w[1]);
            if periodic_a {
                let shift = geom::nearest_turn(a0, a) - a0;
                a0 += shift;
                a1 += shift;
            }
            if (a0 > a) == (a1 > a) {
                continue;
            }
            let bc = b0 + (a - a0) / (a1 - a0) * (b1 - b0);
            if bc > b && bc < b_end {
                count += 1;
            }
        }
    }
    count % 2 == 1
}
