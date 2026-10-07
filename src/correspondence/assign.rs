//! One-to-one assignment with an explicit "unmatched" option (the Hungarian method), and how
//! clearly each answer beats its alternatives.
//!
//! The *n* × *m* costs are embedded in a square (n + m) matrix: old *i* may instead take its own
//! "unmatched" column, and new *j* its own "unmatched" row, each at `unmatched`; dummy meets
//! dummy at 0. A pair is then worth matching only when it costs less than leaving both
//! unmatched. At the optimum the duals give every edge a reduced cost ≥ 0, and forcing any
//! assignment raises the total by at least the sum of its edges' reduced costs: a cheap lower
//! bound that settles most margin questions without solving again.

// Matrix code reads clearest indexed.
#![allow(clippy::needless_range_loop)]

/// A cost no assignment takes.
pub const FORBIDDEN: f64 = 1e9;

pub struct Solution {
    /// Each old item's new item, or none.
    pub matched: Vec<Option<usize>>,
    pub total: f64,
    n: usize,
    m: usize,
    /// Reduced costs of the square matrix, row-major.
    reduced: Vec<f64>,
}

impl Solution {
    /// The reduced cost of old *i* taking new *j*, or (with `None`) going unmatched.
    pub fn reduced(&self, i: usize, j: Option<usize>) -> f64 {
        let size = self.n + self.m;
        let col = j.unwrap_or(self.m + i);
        self.reduced[i * size + col]
    }
}

fn square(costs: &[Vec<f64>], m: usize, unmatched: f64) -> Vec<Vec<f64>> {
    let n = costs.len();
    let size = n + m;
    let mut a = vec![vec![0.0; size]; size];
    for i in 0..n {
        for j in 0..m {
            a[i][j] = costs[i][j].min(FORBIDDEN);
        }
        for k in 0..n {
            a[i][m + k] = if k == i { unmatched } else { FORBIDDEN };
        }
    }
    for l in 0..m {
        for j in 0..m {
            a[n + l][j] = if l == j { unmatched } else { FORBIDDEN };
        }
    }
    a
}

/// The cheapest assignment of the *n* old items (rows of *costs*) to *m* new ones.
pub fn solve(costs: &[Vec<f64>], m: usize, unmatched: f64) -> Solution {
    let n = costs.len();
    let a = square(costs, m, unmatched);
    let (assignment, u, v) = hungarian(&a);
    let size = n + m;
    let mut reduced = vec![0.0; size * size];
    for i in 0..size {
        for j in 0..size {
            reduced[i * size + j] = (a[i][j] - u[i] - v[j]).max(0.0);
        }
    }
    let matched = (0..n)
        .map(|i| (assignment[i] < m).then_some(assignment[i]))
        .collect();
    let total = (0..size).map(|i| a[i][assignment[i]]).sum();
    Solution {
        matched,
        total,
        n,
        m,
        reduced,
    }
}

/// The least total of an assignment with old *i* forbidden from new *j* (`Some`) or from staying
/// unmatched (`None`), less the optimum.
pub fn forbid(costs: &[Vec<f64>], m: usize, unmatched: f64, i: usize, j: Option<usize>) -> f64 {
    let n = costs.len();
    let base = solve(costs, m, unmatched).total;
    let mut a = square(costs, m, unmatched);
    a[i][j.unwrap_or(m + i)] = FORBIDDEN;
    let (assignment, _, _) = hungarian(&a);
    let total: f64 = (0..n + m).map(|r| a[r][assignment[r]]).sum();
    total - base
}

/// The least total of an assignment in which old *i* stays unmatched, less the optimum.
pub fn force_unmatched(costs: &[Vec<f64>], m: usize, unmatched: f64, i: usize) -> f64 {
    force(costs, m, unmatched, i, m + i)
}

/// The least total of an assignment in which old *i* takes column *j* of the square matrix (a
/// new item below *m*, its own "unmatched" column at m + i), less the optimum.
pub fn force(costs: &[Vec<f64>], m: usize, unmatched: f64, i: usize, j: usize) -> f64 {
    let n = costs.len();
    let base = solve(costs, m, unmatched).total;
    let mut a = square(costs, m, unmatched);
    for col in 0..n + m {
        if col != j {
            a[i][col] = FORBIDDEN;
        }
    }
    let (assignment, _, _) = hungarian(&a);
    let total: f64 = (0..n + m).map(|r| a[r][assignment[r]]).sum();
    total - base
}

/// The square assignment problem by shortest augmenting paths (Kuhn–Munkres with potentials,
/// O(n³)): each row's column, and the row and column potentials.
fn hungarian(a: &[Vec<f64>]) -> (Vec<usize>, Vec<f64>, Vec<f64>) {
    let n = a.len();
    let inf = f64::INFINITY;
    // 1-based, with column 0 as the augmenting path's root.
    let mut u = vec![0.0; n + 1];
    let mut v = vec![0.0; n + 1];
    let mut p = vec![0usize; n + 1];
    let mut way = vec![0usize; n + 1];
    for i in 1..=n {
        p[0] = i;
        let mut j0 = 0;
        let mut minv = vec![inf; n + 1];
        let mut used = vec![false; n + 1];
        loop {
            used[j0] = true;
            let i0 = p[j0];
            let mut delta = inf;
            let mut j1 = 0;
            for j in 1..=n {
                if !used[j] {
                    let cur = a[i0 - 1][j - 1] - u[i0] - v[j];
                    if cur < minv[j] {
                        minv[j] = cur;
                        way[j] = j0;
                    }
                    if minv[j] < delta {
                        delta = minv[j];
                        j1 = j;
                    }
                }
            }
            for j in 0..=n {
                if used[j] {
                    u[p[j]] += delta;
                    v[j] -= delta;
                } else {
                    minv[j] -= delta;
                }
            }
            j0 = j1;
            if p[j0] == 0 {
                break;
            }
        }
        loop {
            let j1 = way[j0];
            p[j0] = p[j1];
            j0 = j1;
            if j0 == 0 {
                break;
            }
        }
    }
    let mut row = vec![0usize; n];
    for j in 1..=n {
        if p[j] > 0 {
            row[p[j] - 1] = j - 1;
        }
    }
    (row, u[1..].to_vec(), v[1..].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assigns_cheaply_and_leaves_poor_matches_unmatched() {
        let costs = vec![
            vec![0.1, 0.9, 5.0],
            vec![0.2, 0.15, 5.0],
            vec![5.0, 5.0, 5.0],
        ];
        let s = solve(&costs, 3, 0.5);
        assert_eq!(s.matched, vec![Some(0), Some(1), None]);
        assert!((s.total - (0.1 + 0.15 + 0.5 + 0.5)).abs() < 1e-12);
        // Forbidding 0→0 costs at least its reduced-cost bound, and exactly the re-solve.
        let delta = forbid(&costs, 3, 0.5, 0, Some(0));
        assert!(delta + 1e-12 >= s.reduced(0, None).min(s.reduced(0, Some(1))));
        assert!((force(&costs, 3, 0.5, 2, 2) - (5.0 - 1.0)).abs() < 1e-12);
    }

    #[test]
    fn identical_repeats_have_no_margin() {
        let costs = vec![vec![0.0, 0.0], vec![0.0, 0.0]];
        let s = solve(&costs, 2, 0.5);
        assert!(s.matched.iter().all(Option::is_some));
        assert!(forbid(&costs, 2, 0.5, 0, s.matched[0]) < 1e-12);
    }
}
