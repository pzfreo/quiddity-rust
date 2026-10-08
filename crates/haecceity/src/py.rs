//! Python's numeric semantics, wherever the port must reproduce the Python implementation's
//! results to the last bit: rounding, summation, norms, the `%` operator, tuple ordering and
//! first-wins `min`/`max`.
//!
//! A last-bit difference matters here because records are rounded (to decimals or significant
//! figures) and then grouped with `==`: near a rounding boundary the bit decides the output.

use std::cmp::Ordering;

/// `round(value, digits)`: correctly rounded on the exact binary value.
pub fn round_to(value: f64, digits: usize) -> f64 {
    format!("{value:.digits$}")
        .parse()
        .expect("formatted float parses")
}

/// `round(value, 3)`, the grid most records are published on.
pub fn round_to3(value: f64) -> f64 {
    round_to(value, 3)
}

/// `quantise(value)`: six significant figures, the default grid.
pub fn quantise6(value: f64) -> f64 {
    quantise(value, 6)
}

/// `quiddity._geometry.quantise`: *value* to *figures* significant figures.
pub fn quantise(value: f64, figures: usize) -> f64 {
    let precision = figures.saturating_sub(1);
    format!("{value:.precision$e}")
        .parse()
        .expect("formatted float parses")
}

/// `without_negative_zero`: -0.0 becomes 0.0.
pub fn without_negative_zero(value: f64) -> f64 {
    if value == 0.0 { 0.0 } else { value }
}

/// `math.fsum`: the correctly rounded sum (Shewchuk's algorithm, as CPython implements it).
pub fn fsum(values: impl IntoIterator<Item = f64>) -> f64 {
    let mut partials: Vec<f64> = Vec::new();
    for mut x in values {
        let mut i = 0;
        for j in 0..partials.len() {
            let mut y = partials[j];
            if x.abs() < y.abs() {
                std::mem::swap(&mut x, &mut y);
            }
            let hi = x + y;
            let lo = y - (hi - x);
            if lo != 0.0 {
                partials[i] = lo;
                i += 1;
            }
            x = hi;
        }
        partials.truncate(i);
        partials.push(x);
    }
    let Some(&last) = partials.last() else {
        return 0.0;
    };
    let (mut hi, mut lo, mut n) = (last, 0.0, partials.len() - 1);
    while n > 0 {
        let x = hi;
        let y = partials[n - 1];
        n -= 1;
        hi = x + y;
        lo = y - (hi - x);
        if lo != 0.0 {
            break;
        }
    }
    // Round half to even across the remaining partials.
    if n > 0 && ((lo < 0.0 && partials[n - 1] < 0.0) || (lo > 0.0 && partials[n - 1] > 0.0)) {
        let y = lo * 2.0;
        let x = hi + y;
        if y == x - hi {
            hi = x;
        }
    }
    hi
}

/// The builtin `sum` of floats (Python 3.12+): Neumaier-compensated, starting from integer 0,
/// so a sum of negative zeros is +0.0.
pub fn sum(values: impl IntoIterator<Item = f64>) -> f64 {
    let (mut total, mut compensation) = (0.0f64, 0.0f64);
    for x in values {
        let t = total + x;
        compensation += if total.abs() >= x.abs() {
            (total - t) + x
        } else {
            (x - t) + total
        };
        total = t;
    }
    // CPython drops a non-finite compensation, so an infinite term sums to inf rather than NaN.
    if compensation.is_finite() {
        total + compensation
    } else {
        total
    }
}

/// `quiddity._geometry.dot`: the exactly rounded dot product (`fsum` of the products).
pub fn dot(a: &[f64], b: &[f64]) -> f64 {
    fsum(a.iter().zip(b).map(|(x, y)| x * y))
}

/// `math.hypot(*values)`: the square root of the correctly rounded sum of the exact squares.
/// CPython scales and corrects differently, so the two can differ in the last bit; both are
/// within an ulp of the true norm.
pub fn hypot(values: &[f64]) -> f64 {
    let mut terms = Vec::with_capacity(2 * values.len());
    for &x in values {
        let hi = x * x;
        terms.push(hi);
        terms.push(x.mul_add(x, -hi));
    }
    fsum(terms).sqrt()
}

/// `quiddity._geometry.unit_or_none`: divided by its `hypot` norm, `None` when degenerate.
pub fn unit(v: [f64; 3]) -> Option<[f64; 3]> {
    let n = hypot(&v);
    (n.is_finite() && n > 1e-9).then(|| v.map(|c| c / n))
}

/// `math.dist(a, b)`.
pub fn dist(a: &[f64], b: &[f64]) -> f64 {
    let d: Vec<f64> = a.iter().zip(b).map(|(x, y)| x - y).collect();
    hypot(&d)
}

/// Python's float `%`: the result takes the divisor's sign, and a zero result is +0.0 for a
/// positive divisor (`rem_euclid` would keep -0.0).
pub fn modulo(a: f64, b: f64) -> f64 {
    let mut r = a % b;
    if r != 0.0 {
        if (r < 0.0) != (b < 0.0) {
            r += b;
        }
    } else {
        r = 0.0f64.copysign(b);
    }
    r
}

/// Python's `<` on floats as an ordering: -0.0 equals 0.0 (`total_cmp` would separate them).
/// Python has no order for NaN; here every NaN sorts after every number and equals every other
/// NaN, so that this stays a total order and a sort stays deterministic.
pub fn order(a: f64, b: f64) -> Ordering {
    a.partial_cmp(&b)
        .unwrap_or_else(|| a.is_nan().cmp(&b.is_nan()))
}

/// Python's ordering of float tuples: lexicographic, then by length.
pub fn tuple_order(a: &[f64], b: &[f64]) -> Ordering {
    a.iter()
        .zip(b)
        .map(|(x, y)| order(*x, *y))
        .find(|o| o.is_ne())
        .unwrap_or_else(|| a.len().cmp(&b.len()))
}

/// The index of the first minimum by *key* (Python's `min` keeps the first of ties).
pub fn first_min<T>(items: &[T], key: impl Fn(&T) -> f64) -> usize {
    let mut best = 0;
    for i in 1..items.len() {
        if key(&items[i]) < key(&items[best]) {
            best = i;
        }
    }
    best
}

/// The index of the first maximum by *key*.
pub fn first_max<T>(items: &[T], key: impl Fn(&T) -> f64) -> usize {
    let mut best = 0;
    for i in 1..items.len() {
        if key(&items[i]) > key(&items[best]) {
            best = i;
        }
    }
    best
}

/// `math.isclose(a, b, rel_tol=rel, abs_tol=abs)` (Python's default `rel_tol` is 1e-9).
pub fn isclose(a: f64, b: f64, rel: f64, abs: f64) -> bool {
    a == b || (a - b).abs() <= (rel * a.abs().max(b.abs())).max(abs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summation_matches_cpython() {
        assert_eq!(fsum([1e16, 1.0, -1e16]), 1.0);
        assert_eq!(fsum([0.1; 10]), 1.0);
        assert_eq!(sum([1e16, 1.0, -1e16]), 1.0);
        assert!(sum([-0.0, -0.0]).is_sign_positive());
        assert_eq!(sum([1.0, f64::INFINITY]), f64::INFINITY);
        assert_eq!(dot(&[1e8, 1.0], &[1e8, -1e16]), 0.0);
    }

    #[test]
    fn modulo_matches_python() {
        assert!(modulo(-180.0, 180.0) == 0.0 && modulo(-180.0, 180.0).is_sign_positive());
        assert_eq!(modulo(-90.0, 180.0), 90.0);
        assert_eq!(modulo(270.0, 180.0), 90.0);
    }

    #[test]
    fn order_is_total_with_nan_last() {
        assert_eq!(order(-0.0, 0.0), Ordering::Equal);
        assert_eq!(order(f64::NAN, f64::NAN), Ordering::Equal);
        assert_eq!(order(f64::NAN, f64::INFINITY), Ordering::Greater);
        assert_eq!(order(f64::NEG_INFINITY, f64::NAN), Ordering::Less);
        let mut a = vec![3.0, f64::NAN, -1.0, 0.0, f64::NAN, -0.0, f64::INFINITY, 2.0];
        let mut b = a.clone();
        b.reverse();
        a.sort_by(|x, y| order(*x, *y));
        b.sort_by(|x, y| order(*x, *y));
        let numbers = |v: &[f64]| v[..6].to_vec();
        assert_eq!(numbers(&a), [-1.0, 0.0, -0.0, 2.0, 3.0, f64::INFINITY]);
        assert_eq!(numbers(&a), numbers(&b));
        assert!(a[6..].iter().chain(&b[6..]).all(|x| x.is_nan()));
    }

    #[test]
    fn hypot_is_accurate() {
        assert_eq!(hypot(&[3.0, 4.0]), 5.0);
        assert_eq!(hypot(&[2.0, 3.0, 6.0]), 7.0);
        assert_eq!(dist(&[1.0, 1.0, 1.0], &[4.0, 5.0, 1.0]), 5.0);
    }

    #[test]
    fn rounding_matches_python() {
        assert_eq!(round_to(2.0005, 3), 2.001);
        assert_eq!(round_to(0.0625, 3), 0.062);
        assert_eq!(quantise(1234567.0, 6), 1234570.0);
        assert_eq!(quantise(0.000123456789, 6), 0.000123457);
    }
}
