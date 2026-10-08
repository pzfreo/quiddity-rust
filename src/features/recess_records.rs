//! The records slots, pockets and channels publish (`quiddity._recess_records`), and what the
//! recess reductions read of a slot or pocket through [`Recess`].

use serde::{Deserialize, Serialize};

use super::body::BodyKey;
use crate::kernel::geom::V3;

pub const AXES: [char; 3] = ['x', 'y', 'z'];

/// The index of an axis letter.
pub fn axis_index(axis: char) -> usize {
    AXES.iter()
        .position(|&a| a == axis)
        .expect("an axis letter")
}

/// The axis that is neither of two others.
pub fn third_axis(a: usize, b: usize) -> usize {
    3 - a - b
}

/// A milled through-slot: two opposed parallel walls facing each other. `width` is the wall
/// separation along `width_axis`, `lo`/`hi` the ends along `long_axis`, `d_lo`/`d_hi` the
/// extent on the third (depth) axis. `body_key` is the source solid's key (`None` when it is
/// ambiguous); `end_radius` and `corner_radius` are `None` when unproved, never "square".
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Slot {
    pub width_axis: char,
    pub long_axis: char,
    pub width: f64,
    pub length: f64,
    pub w_center: f64,
    pub lo: f64,
    pub hi: f64,
    pub d_lo: f64,
    pub d_hi: f64,
    pub body_key: Option<BodyKey>,
    pub end_radius: Option<f64>,
    pub corner_radius: Option<f64>,
}

/// A bounded blind recess: a slot capped by a floor, so with a `depth` (`d_hi - d_lo`) and the
/// side it opens to along the depth axis (`open_sign`). `edge_anchored` marks a corner notch,
/// located by the two envelope edges it touches.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pocket {
    pub width_axis: char,
    pub long_axis: char,
    pub width: f64,
    pub length: f64,
    pub depth: f64,
    pub w_center: f64,
    pub lo: f64,
    pub hi: f64,
    pub d_lo: f64,
    pub d_hi: f64,
    pub open_sign: i32,
    pub edge_anchored: bool,
    pub body_key: Option<BodyKey>,
    pub end_radius: Option<f64>,
    pub corner_radius: Option<f64>,
}

/// A floored rectangular channel open at both longitudinal ends of its solid: its one defining
/// measurement is `width`; the rest is kept for identity.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Channel {
    pub width_axis: char,
    pub long_axis: char,
    pub width: f64,
    pub w_center: f64,
    pub lo: f64,
    pub hi: f64,
    pub d_lo: f64,
    pub d_hi: f64,
    pub open_sign: i32,
    pub body_key: Option<BodyKey>,
}

/// What the shared reductions read and rewrite of a slot or pocket.
pub trait Recess: Clone + PartialEq {
    fn width_axis(&self) -> usize;
    fn long_axis(&self) -> usize;
    fn width(&self) -> f64;
    fn w_center(&self) -> f64;
    fn lo(&self) -> f64;
    fn hi(&self) -> f64;
    fn d_lo(&self) -> f64;
    fn d_hi(&self) -> f64;
    fn edge_anchored(&self) -> bool {
        false
    }
    /// The record with new ends and length (`replace(record, lo=, hi=, length=)`).
    fn spanned(&self, lo: f64, hi: f64, length: f64) -> Self;
    fn set_end_radius(&mut self, radius: f64);
    fn set_corner_radius(&mut self, radius: f64);
    fn set_body_key(&mut self, key: Option<BodyKey>);

    fn depth_axis(&self) -> usize {
        third_axis(self.width_axis(), self.long_axis())
    }

    /// The mid-point in part coordinates (`_region_center`).
    fn region_center(&self) -> V3 {
        let mut c = [0.0; 3];
        c[self.width_axis()] = self.w_center();
        c[self.long_axis()] = (self.lo() + self.hi()) / 2.0;
        c[self.depth_axis()] = (self.d_lo() + self.d_hi()) / 2.0;
        c
    }
}

macro_rules! recess {
    ($record:ty, $anchored:expr) => {
        impl Recess for $record {
            fn width_axis(&self) -> usize {
                axis_index(self.width_axis)
            }
            fn long_axis(&self) -> usize {
                axis_index(self.long_axis)
            }
            fn width(&self) -> f64 {
                self.width
            }
            fn w_center(&self) -> f64 {
                self.w_center
            }
            fn lo(&self) -> f64 {
                self.lo
            }
            fn hi(&self) -> f64 {
                self.hi
            }
            fn d_lo(&self) -> f64 {
                self.d_lo
            }
            fn d_hi(&self) -> f64 {
                self.d_hi
            }
            fn spanned(&self, lo: f64, hi: f64, length: f64) -> Self {
                Self {
                    lo,
                    hi,
                    length,
                    ..self.clone()
                }
            }
            fn set_end_radius(&mut self, radius: f64) {
                self.end_radius = Some(radius);
            }
            fn set_corner_radius(&mut self, radius: f64) {
                self.corner_radius = Some(radius);
            }
            fn set_body_key(&mut self, key: Option<BodyKey>) {
                self.body_key = key;
            }
            fn edge_anchored(&self) -> bool {
                ($anchored)(self)
            }
        }
    };
}

recess!(Slot, |_: &Slot| false);
recess!(Pocket, |p: &Pocket| p.edge_anchored);
