//! Published positions and the unrounded values they were published from.
//!
//! A recogniser publishes a position on a grid (`round(x, 3)` for most points, six decimals for a
//! body's box, eleven significant figures for a hole's location). Framed recognition finds the
//! position in the part's own frame and reports it in the file's coordinates; rounding there in
//! the frame and then adding the frame's origin would leave the published value off its grid.
//! A [`Placed`] keeps the value the recogniser rounded *from* beside the one it published, so
//! the record mapper ([`crate::framed_records`]) can carry the unrounded value through the motion
//! and round once, in the file's coordinates, on the same grid.
//!
//! Inside recognition a `Placed` reads as its published value (it dereferences to it, compares
//! by it, and serialises as it), so decisions and records found in the frame are unchanged.

use std::ops::Deref;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::policy::round_measurement;
use crate::kernel::py;

/// How a position is published.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grid {
    /// `round(x, digits)`.
    Decimals(u8),
    /// `round(x, digits)` with a negative zero cleared.
    CleanDecimals(u8),
    /// `quantise(x, figures)`: significant figures.
    Figures(u8),
    /// A body key ([`super::body::BodyKey`]): its box on six decimals (negative zero cleared),
    /// its volume and area (not positions) on twelve significant figures.
    BodyKey,
    /// Published as found, unrounded.
    Exact,
}

impl Grid {
    /// *value* as the recogniser publishes it at the *index*-th coordinate of a position.
    fn found(self, index: usize, value: f64) -> f64 {
        match self {
            Grid::Decimals(d) => py::round_to(value, d.into()),
            Grid::CleanDecimals(d) => py::without_negative_zero(py::round_to(value, d.into())),
            Grid::Figures(n) => py::quantise(value, n.into()),
            Grid::BodyKey if index < 6 => Grid::CleanDecimals(6).found(index, value),
            Grid::BodyKey => py::quantise(value, 12),
            Grid::Exact => value,
        }
    }

    /// *value*, a scalar coordinate in the file's space, published once on this grid.
    pub fn publish(self, value: f64) -> f64 {
        self.published(0, value)
    }

    /// *value*, a coordinate in the file's space, published once on this grid. A decimal
    /// half-way point within the coordinate floor takes the even neighbour (as measured sizes
    /// do, [`round_measurement`]), so binary noise from the motion cannot choose a side.
    fn published(self, index: usize, value: f64) -> f64 {
        match self {
            Grid::Decimals(d) | Grid::CleanDecimals(d) => {
                py::without_negative_zero(round_measurement(value, d.into()))
            }
            Grid::BodyKey if index < 6 => Grid::CleanDecimals(6).published(index, value),
            _ => self.found(index, value),
        }
    }
}

/// A position's coordinates, each rounded on a grid.
pub trait Coordinates: Clone {
    /// Each coordinate, in order, replaced by *f*(its index, it).
    fn each(&self, f: &mut dyn FnMut(usize, f64) -> f64) -> Self;
}

impl Coordinates for f64 {
    fn each(&self, f: &mut dyn FnMut(usize, f64) -> f64) -> Self {
        f(0, *self)
    }
}

impl<const N: usize> Coordinates for [f64; N] {
    fn each(&self, f: &mut dyn FnMut(usize, f64) -> f64) -> Self {
        std::array::from_fn(|i| f(i, self[i]))
    }
}

impl Coordinates for (f64, f64) {
    fn each(&self, f: &mut dyn FnMut(usize, f64) -> f64) -> Self {
        (f(0, self.0), f(1, self.1))
    }
}

impl Coordinates for Vec<f64> {
    fn each(&self, f: &mut dyn FnMut(usize, f64) -> f64) -> Self {
        self.iter().enumerate().map(|(i, &v)| f(i, v)).collect()
    }
}

impl<const N: usize> Coordinates for Vec<[f64; N]> {
    fn each(&self, f: &mut dyn FnMut(usize, f64) -> f64) -> Self {
        self.iter().map(|p| p.each(&mut |_, v| f(0, v))).collect()
    }
}

/// A published position: the value the recogniser published, the unrounded value it published
/// it from, and the grid. It reads, compares and serialises as the published value.
#[derive(Clone, Copy, Debug)]
pub struct Placed<T> {
    value: T,
    raw: T,
    grid: Grid,
}

impl<T: Coordinates> Placed<T> {
    /// *raw* published on *grid*.
    pub fn new(raw: T, grid: Grid) -> Placed<T> {
        Placed {
            value: raw.each(&mut |i, v| grid.found(i, v)),
            raw,
            grid,
        }
    }

    /// *value* as a recogniser published it from *raw* by its own arithmetic on *grid* (an
    /// integer count of grid steps, say), where [`Placed::new`] would not reproduce it bit for
    /// bit.
    pub fn from_parts(value: T, raw: T, grid: Grid) -> Placed<T> {
        Placed { value, raw, grid }
    }

    /// Both values carried by *f*, on the same grid (a pair as an array, a field of a point).
    pub fn map<U: Coordinates>(self, f: impl Fn(T) -> U) -> Placed<U> {
        Placed {
            value: f(self.value),
            raw: f(self.raw),
            grid: self.grid,
        }
    }

    /// A value published as it is (a fixture, or a value read back from JSON).
    pub fn exact(value: T) -> Placed<T> {
        Placed {
            raw: value.clone(),
            value,
            grid: Grid::Exact,
        }
    }

    /// The value it was published from.
    pub fn raw(&self) -> &T {
        &self.raw
    }

    pub fn grid(&self) -> Grid {
        self.grid
    }

    /// The published value.
    pub fn get(&self) -> T {
        self.value.clone()
    }

    /// Carried by *f* (a motion's action on the position). With *publish*, the unrounded value
    /// is carried and published once, on the grid; otherwise the published value is carried
    /// as it is, as records were before publication in the file's coordinates.
    pub fn carried(self, publish: bool, mut f: impl FnMut(T) -> T) -> Placed<T> {
        let raw = f(self.raw);
        let value = if publish {
            raw.each(&mut |i, v| self.grid.published(i, v))
        } else {
            f(self.value)
        };
        Placed {
            value,
            raw,
            grid: self.grid,
        }
    }

    /// Whether the published value is on its grid in the file's coordinates.
    pub fn on_grid(&self) -> bool {
        let mut on = true;
        self.value.each(&mut |i, v| {
            on &= self.grid.published(i, v) == v;
            v
        });
        on
    }
}

impl Placed<(f64, f64)> {
    /// Each end on its own.
    pub fn ends(self) -> [Placed<f64>; 2] {
        [
            Placed {
                value: self.value.0,
                raw: self.raw.0,
                grid: self.grid,
            },
            Placed {
                value: self.value.1,
                raw: self.raw.1,
                grid: self.grid,
            },
        ]
    }
}

impl<T> Deref for Placed<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}

impl<'a, T> IntoIterator for &'a Placed<T>
where
    &'a T: IntoIterator,
{
    type Item = <&'a T as IntoIterator>::Item;
    type IntoIter = <&'a T as IntoIterator>::IntoIter;
    fn into_iter(self) -> Self::IntoIter {
        self.value.into_iter()
    }
}

impl<T: Coordinates> From<T> for Placed<T> {
    fn from(value: T) -> Placed<T> {
        Placed::exact(value)
    }
}

impl<T: PartialEq> PartialEq for Placed<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T: PartialOrd> PartialOrd for Placed<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.value.partial_cmp(&other.value)
    }
}

impl<T: Serialize + Coordinates> Serialize for Placed<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[cfg(test)]
        audit::record(self);
        self.value.serialize(serializer)
    }
}

/// Crate tests: how many published positions a serialization wrote, and how many of them were
/// off their grid.
#[cfg(test)]
#[doc(hidden)]
pub mod audit {
    use super::{Coordinates, Placed};
    use std::cell::Cell;

    thread_local! {
        static COUNTS: Cell<Option<(usize, usize)>> = const { Cell::new(None) };
    }

    pub(super) fn record<T: Coordinates>(p: &Placed<T>) {
        COUNTS.with(|c| {
            if let Some((checked, off)) = c.get() {
                let (mut n, mut off_grid) = (0, 0);
                p.value.each(&mut |i, v| {
                    n += 1;
                    off_grid += usize::from(p.grid.published(i, v) != v);
                    v
                });
                c.set(Some((checked + n, off + off_grid)));
            }
        });
    }

    /// *f*'s published coordinates on this thread: `(checked, off their grid)`.
    pub fn count(f: impl FnOnce()) -> (usize, usize) {
        COUNTS.with(|c| c.set(Some((0, 0))));
        f();
        COUNTS.with(|c| c.take()).expect("counting")
    }
}

impl<'de, T: Deserialize<'de> + Coordinates> Deserialize<'de> for Placed<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(Placed::exact)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn found_values_are_the_recognisers() {
        let p = Placed::new([1.23456, -0.0001, 7.0], Grid::Decimals(3));
        assert_eq!(*p, [1.235, -0.0, 7.0]);
        assert!(p.get()[1].is_sign_negative());
        let p = Placed::new([1.23456, -0.0001, 7.0], Grid::CleanDecimals(3));
        assert!(!p.get()[1].is_sign_negative());
        assert_eq!(*Placed::new(124.999999996, Grid::Figures(11)), 125.0);
    }

    #[test]
    fn carried_values_round_once_after_the_motion() {
        // Rounded in the frame (42.348) and moved, the value would be 40.000410...; carried
        // unrounded, it is published once on the file's grid.
        let t = -2.347589604795208;
        let p = Placed::new(42.3475896048, Grid::Decimals(3));
        assert_eq!(*p, 42.348);
        assert_eq!(p.carried(false, |v| v + t).get(), 42.348 + t);
        let carried = p.carried(true, |v| v + t);
        assert_eq!(*carried, 40.0);
        assert!(carried.on_grid());
        assert!(!p.carried(false, |v| v + t).on_grid());
    }

    #[test]
    fn half_way_points_take_the_even_neighbour_whatever_the_noise() {
        for noise in [-1e-9, 0.0, 1e-9] {
            let p = Placed::new(0.0, Grid::Decimals(3)).carried(true, |_| 12.3455 + noise);
            assert_eq!(*p, 12.346);
            let p = Placed::new(0.0, Grid::Decimals(3)).carried(true, |_| 12.3445 + noise);
            assert_eq!(*p, 12.344);
        }
    }

    #[test]
    fn a_body_key_rounds_its_box_and_keeps_its_figures() {
        let key = Placed::new(
            vec![
                1.0000004,
                -0.0000001,
                2.0,
                3.0,
                4.0,
                5.0,
                123.4567890123456,
                9.0,
            ],
            Grid::BodyKey,
        );
        assert_eq!(*key, vec![1.0, 0.0, 2.0, 3.0, 4.0, 5.0, 123.456789012, 9.0]);
        let moved = key.carried(true, |mut k| {
            for v in &mut k[..6] {
                *v += 0.1234567;
            }
            k
        });
        assert!(moved.on_grid());
        assert_eq!(moved[6..], [123.456789012, 9.0]);
    }
}
