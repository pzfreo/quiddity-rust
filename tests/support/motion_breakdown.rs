//! Python tools/rigid_motion_sweep.py's exact occurrence matcher. Matching is evidence
//! containment, then maximum cardinality, same-family count, overlap sum, and least pair list.
//! Unlike feature correspondence this deliberately permits observed family transitions.
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub struct Occurrence {
    pub family: String,
    pub faces: Vec<usize>,
}

pub fn occurrences(signature: &BTreeMap<&str, Vec<Vec<usize>>>) -> Vec<Occurrence> {
    signature
        .iter()
        .flat_map(|(family, sets)| {
            sets.iter().map(|faces| Occurrence {
                family: (*family).into(),
                faces: {
                    let mut f = faces.clone();
                    f.sort_unstable();
                    f.dedup();
                    f
                },
            })
        })
        .collect()
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Assignment {
    count: usize,
    same: usize,
    overlap: f64,
    pairs: Vec<(usize, usize)>,
}

impl Assignment {
    fn better_than(&self, other: &Self) -> bool {
        (self.count, self.same)
            .cmp(&(other.count, other.same))
            .then_with(|| self.overlap.partial_cmp(&other.overlap).unwrap())
            .then_with(|| other.pairs.cmp(&self.pairs))
            .is_gt()
    }
}

type Edge = (usize, bool, f64);
type Memo = BTreeMap<(usize, Vec<usize>), Assignment>;

fn solve(
    index: usize,
    used: &[usize],
    edges: &[Vec<Edge>],
    last: &[usize],
    memo: &mut Memo,
) -> Assignment {
    if index == edges.len() {
        return Assignment::default();
    }
    // Forget used destinations with no future edge. This leaves the exact Python programme
    // unchanged while avoiding exponential states for the corpus's many disjoint occurrences.
    let used: Vec<_> = used.iter().copied().filter(|&j| last[j] >= index).collect();
    let key = (index, used.clone());
    if let Some(result) = memo.get(&key) {
        return result.clone();
    }
    let mut best = solve(index + 1, &used, edges, last, memo);
    for &(j, same, overlap) in &edges[index] {
        if used.contains(&j) {
            continue;
        }
        let mut next = used.clone();
        next.push(j);
        next.sort_unstable();
        let mut option = solve(index + 1, &next, edges, last, memo);
        option.count += 1;
        option.same += usize::from(same);
        option.overlap += overlap;
        option.pairs.insert(0, (index, j));
        if option.better_than(&best) {
            best = option;
        }
    }
    memo.insert(key, best.clone());
    best
}

/// Additive matching objective, with an exact lexicographic incidence vector for the final
/// pair-list tie. Earlier (baseline, moved) pairs have higher priority at fixed cardinality.
#[derive(Clone, Default)]
struct Weight {
    same: i64,
    overlap: f64,
    priority: BTreeMap<usize, i32>,
}
impl Weight {
    fn step(&self, i: usize, j: usize, same: bool, overlap: f64, width: usize, sign: i32) -> Self {
        let mut out = self.clone();
        out.same += i64::from(sign) * i64::from(same);
        out.overlap += f64::from(sign) * overlap;
        let value = out.priority.entry(i * width + j).or_default();
        *value += sign;
        if *value == 0 {
            out.priority.remove(&(i * width + j));
        }
        out
    }
    fn better_than(&self, other: &Self) -> bool {
        let first = self
            .same
            .cmp(&other.same)
            .then_with(|| self.overlap.partial_cmp(&other.overlap).unwrap());
        if first.is_ne() {
            return first.is_gt();
        }
        for key in self
            .priority
            .keys()
            .chain(other.priority.keys())
            .collect::<std::collections::BTreeSet<_>>()
        {
            let order = self
                .priority
                .get(key)
                .unwrap_or(&0)
                .cmp(other.priority.get(key).unwrap_or(&0));
            if order.is_ne() {
                return order.is_gt();
            }
        }
        false
    }
}

/// Successive maximum-weight augmenting paths implement the same objective for large corpus
/// inventories. The reference's exponential bitmask programme is retained for small fixtures.
fn augmenting_matching(edges: &[Vec<Edge>], width: usize) -> Vec<(usize, usize)> {
    let n = edges.len();
    let nodes = n + width;
    let mut left = vec![None; n];
    let mut right = vec![None; width];
    loop {
        let mut distance = vec![None::<Weight>; nodes];
        let mut previous = vec![None; nodes];
        for i in 0..n {
            if left[i].is_none() {
                distance[i] = Some(Weight::default());
            }
        }
        for _ in 0..nodes.saturating_sub(1) {
            let mut changed = false;
            for (i, destinations) in edges.iter().enumerate() {
                for &(j, same, overlap) in destinations {
                    let (from, to, sign) = if left[i] == Some(j) {
                        (n + j, i, -1)
                    } else {
                        (i, n + j, 1)
                    };
                    let Some(start) = &distance[from] else {
                        continue;
                    };
                    let score = start.step(i, j, same, overlap, width, sign);
                    if distance[to]
                        .as_ref()
                        .is_none_or(|old| score.better_than(old))
                    {
                        distance[to] = Some(score);
                        previous[to] = Some(from);
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        let mut best: Option<usize> = None;
        for (j, matched) in right.iter().enumerate() {
            if matched.is_none()
                && let Some(score) = &distance[n + j]
                && best.is_none_or(|old| score.better_than(distance[n + old].as_ref().unwrap()))
            {
                best = Some(j);
            }
        }
        let Some(j) = best else {
            break;
        };
        let mut node = n + j;
        let mut path = Vec::new();
        while let Some(from) = previous[node] {
            assert!(
                path.len() < nodes,
                "matching objective has a residual cycle"
            );
            path.push((from, node));
            node = from;
        }
        for &(from, to) in &path {
            if from >= n {
                right[from - n] = None;
                left[to] = None;
            }
        }
        for &(from, to) in &path {
            if from < n {
                left[from] = Some(to - n);
                right[to - n] = Some(from);
            }
        }
    }
    left.into_iter()
        .enumerate()
        .filter_map(|(i, j)| j.map(|j| (i, j)))
        .collect()
}

pub fn matching(before: &[Occurrence], after: &[Occurrence]) -> Vec<(usize, usize)> {
    // This attains all three upper bounds simultaneously; the identity is also the least
    // pair list (including repeated identical occurrences). Avoid solving a proved optimum.
    if before == after {
        return (0..before.len()).map(|i| (i, i)).collect();
    }
    let mut last = vec![0; after.len()];
    let edges: Vec<Vec<Edge>> = before
        .iter()
        .enumerate()
        .map(|(i, a)| {
            after
                .iter()
                .enumerate()
                .filter_map(|(j, b)| {
                    let subset = |x: &[usize], y: &[usize]| x.iter().all(|f| y.contains(f));
                    if !subset(&a.faces, &b.faces) && !subset(&b.faces, &a.faces) {
                        return None;
                    }
                    assert!(
                        !a.faces.is_empty() && !b.faces.is_empty(),
                        "occurrences need defining evidence"
                    );
                    let intersection = a.faces.iter().filter(|f| b.faces.contains(f)).count();
                    let union = a.faces.len() + b.faces.len() - intersection;
                    last[j] = i;
                    Some((j, a.family == b.family, intersection as f64 / union as f64))
                })
                .collect()
        })
        .collect();
    if before.len() <= 20 && after.len() <= 20 {
        solve(0, &[], &edges, &last, &mut Memo::new()).pairs
    } else {
        augmenting_matching(&edges, after.len())
    }
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Breakdown {
    pub baseline: usize,
    pub same: usize,
    pub absent: BTreeMap<String, usize>,
    pub reclassified: BTreeMap<String, usize>,
    pub introduced: BTreeMap<String, usize>,
}

impl Breakdown {
    pub fn compare(before: &[Occurrence], after: &[Occurrence]) -> Self {
        let pairs = matching(before, after);
        let mut out = Self {
            baseline: before.len(),
            ..Self::default()
        };
        for &(i, j) in &pairs {
            if before[i].family == after[j].family {
                out.same += 1;
            } else {
                *out.reclassified
                    .entry(format!("{}->{}", before[i].family, after[j].family))
                    .or_default() += 1;
            }
        }
        for (i, a) in before.iter().enumerate() {
            if !pairs.iter().any(|p| p.0 == i) {
                *out.absent.entry(a.family.clone()).or_default() += 1;
            }
        }
        for (j, b) in after.iter().enumerate() {
            if !pairs.iter().any(|p| p.1 == j) {
                *out.introduced.entry(b.family.clone()).or_default() += 1;
            }
        }
        out
    }

    pub fn add(&mut self, other: &Self) {
        self.baseline += other.baseline;
        self.same += other.same;
        for (to, from) in [
            (&mut self.absent, &other.absent),
            (&mut self.reclassified, &other.reclassified),
            (&mut self.introduced, &other.introduced),
        ] {
            for (family, count) in from {
                *to.entry(family.clone()).or_default() += count;
            }
        }
    }

    pub fn regressions(&self, limit: &Self) -> Vec<String> {
        let mut out = Vec::new();
        for (kind, got, max) in [
            ("absent", &self.absent, &limit.absent),
            ("reclassified", &self.reclassified, &limit.reclassified),
            ("introduced", &self.introduced, &limit.introduced),
        ] {
            for (family, count) in got {
                let max = max.get(family).copied().unwrap_or(0);
                if *count > max {
                    out.push(format!("{kind} {family}: {count} > {max}"));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn o(family: &str, faces: &[usize]) -> Occurrence {
        Occurrence {
            family: family.into(),
            faces: faces.into(),
        }
    }
    #[test]
    fn exact_matching_beats_greedy_and_observes_transitions() {
        let a = [o("hole", &[1]), o("pocket", &[1, 2])];
        let b = [o("hole", &[1, 2]), o("slot", &[1, 3])];
        assert_eq!(matching(&a, &b), [(0, 1), (1, 0)]);
        let result = Breakdown::compare(&a, &b);
        assert_eq!(
            result.reclassified,
            BTreeMap::from([("hole->slot".into(), 1), ("pocket->hole".into(), 1)])
        );
        assert!(result.absent.is_empty() && result.introduced.is_empty());
    }
    #[test]
    fn same_family_then_overlap_then_lexicographic_pairs() {
        let a = [o("hole", &[1]), o("slot", &[1])];
        let b = [o("slot", &[1]), o("hole", &[1])];
        assert_eq!(matching(&a, &b), [(0, 1), (1, 0)]);
        let b = [o("other", &[1, 2]), o("other", &[1])];
        assert_eq!(matching(&a[..1], &b), [(0, 1)]);
        assert_eq!(matching(&a, &b), [(0, 0), (1, 1)]);
        let result = Breakdown::compare(&[o("a", &[7])], &[o("b", &[8])]);
        assert_eq!(result.absent["a"], 1);
        assert_eq!(result.introduced["b"], 1);
        assert_eq!(result.regressions(&Breakdown::default()).len(), 2);
    }
    #[test]
    fn augmenting_paths_agree_with_reference_dynamic_programme() {
        let mut seed = 7u64;
        for _ in 0..1000 {
            let mut edges = vec![Vec::new(); 5];
            let mut last = vec![0; 5];
            for (i, row) in edges.iter_mut().enumerate() {
                for (j, tail) in last.iter_mut().enumerate() {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    if seed >> 62 == 0 {
                        continue;
                    }
                    row.push((j, seed & 1 != 0, ((seed >> 2) % 4 + 1) as f64 / 4.));
                    *tail = i;
                }
            }
            assert_eq!(
                augmenting_matching(&edges, 5),
                solve(0, &[], &edges, &last, &mut Memo::new()).pairs,
                "{edges:?}"
            );
        }
    }
}
