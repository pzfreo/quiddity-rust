//! Real roots of a polynomial on an interval, by isolation rather than sampling: the roots of
//! the derivative split the interval into pieces on which the polynomial is monotonic, so each
//! piece holds at most one root, found by bisection to the last bit. A root the polynomial only
//! touches (a tangency) is a critical point where it vanishes, and is reported as one.

/// A root and whether the polynomial only touches zero there (a double root: a tangency).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Root {
    pub x: f64,
    pub touching: bool,
}

/// `c[0] + c[1] x + c[2] x² + …` at x.
pub fn eval(c: &[f64], x: f64) -> f64 {
    c.iter().rev().fold(0.0, |acc, &k| acc * x + k)
}

/// The sum of the terms' magnitudes at x: the scale against which a value counts as zero.
fn magnitude(c: &[f64], x: f64) -> f64 {
    c.iter().rev().fold(0.0, |acc, &k| acc * x.abs() + k.abs())
}

fn derivative(c: &[f64]) -> Vec<f64> {
    c.iter()
        .enumerate()
        .skip(1)
        .map(|(i, &k)| k * i as f64)
        .collect()
}

/// The real roots of the polynomial with coefficients *c* (lowest degree first) in `[lo, hi]`,
/// ascending.
pub fn roots(c: &[f64], lo: f64, hi: f64) -> Vec<Root> {
    let mut c = c.to_vec();
    while c.len() > 1 && c[c.len() - 1] == 0.0 {
        c.pop();
    }
    if c.len() <= 1 || lo > hi {
        return Vec::new();
    }
    // Where the polynomial is monotonic between consecutive points, it has at most one root.
    let mut cuts = vec![lo];
    let critical = if c.len() > 2 {
        roots(&derivative(&c), lo, hi)
    } else {
        Vec::new()
    };
    cuts.extend(critical.iter().map(|r| r.x).filter(|&x| x > lo && x < hi));
    cuts.push(hi);
    let zero = |x: f64| eval(&c, x).abs() <= 1e-12 * magnitude(&c, x).max(f64::MIN_POSITIVE);
    let mut out: Vec<Root> = Vec::new();
    let push = |x: f64, touching: bool, out: &mut Vec<Root>| {
        if out
            .last()
            .is_none_or(|r| x - r.x > 1e-12 * x.abs().max(1.0))
        {
            out.push(Root { x, touching });
        }
    };
    for w in cuts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let (fa, fb) = (eval(&c, a), eval(&c, b));
        let interior = |x: f64| x > lo && x < hi;
        if zero(a) {
            // A zero at a critical point is a tangency unless the sign changes across it.
            let touching = interior(a) && critical.iter().any(|r| r.x == a) && {
                let h = 1e-6 * (b - a).max(1e-300);
                eval(&c, a - h).signum() == eval(&c, a + h).signum()
            };
            push(a, touching, &mut out);
            continue;
        }
        if fa.signum() == fb.signum() || zero(b) {
            continue;
        }
        let (mut x0, mut x1, mut f0) = (a, b, fa);
        for _ in 0..200 {
            let mid = 0.5 * (x0 + x1);
            if mid <= x0 || mid >= x1 {
                break;
            }
            let fm = eval(&c, mid);
            if fm == 0.0 {
                x0 = mid;
                x1 = mid;
                break;
            }
            if fm.signum() == f0.signum() {
                x0 = mid;
                f0 = fm;
            } else {
                x1 = mid;
            }
        }
        push(0.5 * (x0 + x1), false, &mut out);
    }
    if zero(hi) {
        push(hi, false, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn product(rs: &[f64]) -> Vec<f64> {
        let mut c = vec![1.0];
        for &r in rs {
            let mut next = vec![0.0; c.len() + 1];
            for (i, &k) in c.iter().enumerate() {
                next[i] -= r * k;
                next[i + 1] += k;
            }
            c = next;
        }
        c
    }

    #[test]
    fn finds_close_pairs_sampling_would_miss() {
        // 4000 samples over [0, 10] step 2.5e-3: this pair falls between two of them.
        let c = product(&[1.0, 1.0 + 1e-3, 3.0, 7.5]);
        let xs: Vec<f64> = roots(&c, 0.0, 10.0).iter().map(|r| r.x).collect();
        assert_eq!(xs.len(), 4, "{xs:?}");
        for (x, want) in xs.iter().zip([1.0, 1.0 + 1e-3, 3.0, 7.5]) {
            assert!((x - want).abs() < 1e-12, "{x} vs {want}");
        }
    }

    #[test]
    fn reads_roots_closer_than_the_values_resolve_as_one_tangency() {
        let found = roots(&product(&[1.0, 1.0 + 1e-7, 3.0]), 0.0, 10.0);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found[0].touching && (found[0].x - 1.0).abs() < 1e-6);
    }

    #[test]
    fn reports_a_double_root_once_as_touching() {
        let c = product(&[2.0, 2.0, 5.0, -1.0]);
        let found = roots(&c, 0.0, 10.0);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!((found[0].x - 2.0).abs() < 1e-9 && found[0].touching);
        assert!((found[1].x - 5.0).abs() < 1e-12 && !found[1].touching);
    }

    #[test]
    fn keeps_to_the_interval() {
        let c = product(&[-1.0, 0.5, 4.0]);
        let xs: Vec<f64> = roots(&c, 0.0, 3.0).iter().map(|r| r.x).collect();
        assert_eq!(xs, vec![0.5]);
    }
}
