//! Closed prismatic rings of planar walls (`quiddity._rings`): finding them, measuring them, and
//! asking what caps them.
//!
//! A ring is a cycle of planar faces whose spans along one principal axis all agree, each
//! meeting exactly two others: the walls of a constant-cross-section void running along that
//! axis. What caps its ends decides the feature: neither end is a passage, one a pocket with a
//! floor, both an enclosed cavity. Nothing here decides what a ring is; a recogniser reads the
//! shape, span and caps.

use std::collections::BTreeSet;

use super::Context;
use super::edge_open::SPAN_EPS;
use super::graph::{is_planar, normal, span};
use super::policy::AXIS_ZERO_COS;
use super::sections::V2;
use crate::kernel::classify::{Classifier, State};
use crate::kernel::geom::{Bounds, Curve};
use crate::kernel::py;

/// One closed ring (`Ring`): its walls, its cross-section corners in the two axes other than
/// `axis` (canonical: anticlockwise from the least corner), its span along `axis`, and the
/// neighbouring faces that fill each end.
#[derive(Clone, Debug, PartialEq)]
pub struct Ring {
    /// The walls, ascending (Python's component order is unspecified).
    pub nodes: Vec<usize>,
    pub section: Vec<V2>,
    pub axis: usize,
    pub low: f64,
    pub high: f64,
    /// The faces closing the low and the high end.
    pub cap_nodes: (BTreeSet<usize>, BTreeSet<usize>),
}

impl Ring {
    /// Whether each end is capped (`Ring.caps`).
    pub fn caps(&self) -> (bool, bool) {
        (!self.cap_nodes.0.is_empty(), !self.cap_nodes.1.is_empty())
    }
}

/// The two axes other than *axis*, in order.
fn others(axis: usize) -> [usize; 2] {
    match axis {
        0 => [1, 2],
        1 => [0, 2],
        _ => [0, 1],
    }
}

/// Every closed prismatic ring bounding a void, on any principal axis (`rings`), whatever caps
/// it: axis by axis, each axis's rings by their least wall. Rings bounding material (a prism
/// seen from outside) are not returned.
pub fn rings(ctx: &Context<'_>) -> Vec<Ring> {
    let part = ctx.part;
    let planar: Vec<(usize, Option<[f64; 3]>)> = (0..part.faces.len())
        .filter(|&f| is_planar(part, f))
        .map(|f| (f, normal(part, f)))
        .collect();
    let mut out = Vec::new();
    for axis in 0..3 {
        let walls: Vec<usize> = planar
            .iter()
            .filter(|(_, n)| n.is_some_and(|n| n[axis].abs() <= AXIS_ZERO_COS))
            .map(|&(f, _)| f)
            .collect();
        let adjacent: Vec<BTreeSet<usize>> = walls
            .iter()
            .map(|&w| part.neighbours(w).into_iter().collect())
            .collect();
        let at = |node: usize| walls.binary_search(&node).expect("a wall");
        let shares_a_span = |a: usize, b: usize| {
            let (sa, sb) = (span(part, a, axis), span(part, b, axis));
            adjacent[at(a)].contains(&b)
                && (sa.0 - sb.0).abs() <= SPAN_EPS
                && (sa.1 - sb.1).abs() <= SPAN_EPS
        };
        for ring in components(&walls, shares_a_span) {
            let members: BTreeSet<usize> = ring.iter().copied().collect();
            if ring.len() < 3
                || ring
                    .iter()
                    .any(|&n| adjacent[at(n)].intersection(&members).count() != 2)
            {
                continue; // a ring closes; a chain of walls does not
            }
            let Some(section) = cross_section(ctx, &ring, &members, axis) else {
                continue; // the walls do not meet in a simple prismatic polygon
            };
            let spans: Vec<(f64, f64)> = ring.iter().map(|&n| span(part, n, axis)).collect();
            let low = spans.iter().map(|s| s.0).fold(f64::INFINITY, f64::min);
            let high = spans.iter().map(|s| s.1).fold(f64::NEG_INFINITY, f64::max);
            if !is_void(ctx.classifier(), &section, axis, low, high) {
                continue;
            }
            let cap_nodes = capped_ends(ctx, &ring, &members, axis, low, high);
            out.push(Ring {
                nodes: ring,
                section,
                axis,
                low,
                high,
                cap_nodes,
            });
        }
    }
    out
}

/// The components of *items* under the symmetric relation *joined* (`connected_components`),
/// each ascending, by their least member (Python leaves both orders unspecified).
fn components(items: &[usize], joined: impl Fn(usize, usize) -> bool) -> Vec<Vec<usize>> {
    let mut unseen: BTreeSet<usize> = items.iter().copied().collect();
    let mut out = Vec::new();
    while let Some(start) = unseen.pop_first() {
        let mut component = vec![start];
        let mut frontier = vec![start];
        while let Some(current) = frontier.pop() {
            let attached: Vec<usize> = unseen
                .iter()
                .copied()
                .filter(|&o| joined(current, o))
                .collect();
            for o in attached {
                unseen.remove(&o);
                component.push(o);
                frontier.push(o);
            }
        }
        component.sort_unstable();
        out.push(component);
    }
    out
}

/// The ring's corners walked around it, canonical, or `None` when they are not a simple
/// polygon (`_cross_section`). Each corner is the middle, across the run, of the box of the
/// edges two consecutive walls share.
pub(crate) fn cross_section(
    ctx: &Context<'_>,
    ring: &[usize],
    members: &BTreeSet<usize>,
    axis: usize,
) -> Option<Vec<V2>> {
    let part = ctx.part;
    let mut order = vec![ring[0]];
    let mut seen: BTreeSet<usize> = [ring[0]].into();
    while order.len() < ring.len() {
        // Every member has two in-ring neighbours and the component is connected, so there is
        // always exactly one unvisited step (Python's bare `next`).
        let step = part
            .neighbours(*order.last().expect("started"))
            .into_iter()
            .find(|n| members.contains(n) && !seen.contains(n))
            .expect("a ring member has an unvisited neighbour in the ring");
        seen.insert(step);
        order.push(step);
    }
    let [a0, a1] = others(axis);
    let corners: Vec<V2> = (0..order.len())
        .map(|at| {
            let mut b = Bounds::empty();
            for e in part.shared_edges(order[at], order[(at + 1) % order.len()]) {
                // A line is boxed as OpenCascade boxes it, by the line's own points at its two
                // ends: the file's vertices can sit off it by round-off (808.step: 6e-17 across
                // the run), which moves the corner and with it the void probe's point.
                let edge = &part.edges[e];
                match edge.curve {
                    Curve::Line { .. } => [edge.start, edge.end]
                        .iter()
                        .for_each(|&p| b.add(edge.curve.value(edge.curve.parameter(p)))),
                    _ => edge.samples.iter().for_each(|p| b.add(*p)),
                }
            }
            [0.5 * (b.min[a0] + b.max[a0]), 0.5 * (b.min[a1] + b.max[a1])]
        })
        .collect();
    canonical(corners)
}

/// One walk per shape (`_canonical`): anticlockwise from the lexicographically least corner;
/// `None` for a repeated corner or collinear corners.
pub(crate) fn canonical(mut corners: Vec<V2>) -> Option<Vec<V2>> {
    let count = corners.len();
    if (0..count).any(|i| corners[..i].contains(&corners[i])) {
        return None; // two walls meeting at one point is not a simple polygon
    }
    let twice_area = py::sum((0..count).map(|at| {
        let next = corners[(at + 1) % count];
        corners[at][0] * next[1] - next[0] * corners[at][1]
    }));
    if twice_area.abs() <= SPAN_EPS {
        return None; // degenerate: the corners are collinear
    }
    if twice_area < 0.0 {
        corners.reverse();
    }
    let start = (1..count).fold(0, |best, at| {
        if py::tuple_order(&corners[at], &corners[best]).is_lt() {
            at
        } else {
            best
        }
    });
    corners.rotate_left(start);
    Some(corners)
}

/// The polygon's area centroid (`_centroid`).
pub fn centroid(section: &[V2]) -> V2 {
    let count = section.len();
    let (mut twice_area, mut across, mut along) = (0.0, 0.0, 0.0);
    for at in 0..count {
        let [u0, v0] = section[at];
        let [u1, v1] = section[(at + 1) % count];
        let cross = u0 * v1 - u1 * v0;
        twice_area += cross;
        across += (u0 + u1) * cross;
        along += (v0 + v1) * cross;
    }
    [across / (3.0 * twice_area), along / (3.0 * twice_area)]
}

/// A point proved inside the cross-section (`_interior_point`): the centroid of the ear at the
/// lowest corner when no other corner intrudes into it, else the midpoint of the diagonal to
/// the intruder farthest from the ear's chord. The centroid will not do: a concave section's
/// can lie in the material.
pub fn interior_point(section: &[V2]) -> V2 {
    let count = section.len();
    let at = (1..count).fold(0, |best, k| {
        let key = |i: usize| [section[i][1], section[i][0]];
        if py::tuple_order(&key(k), &key(best)).is_lt() {
            k
        } else {
            best
        }
    });
    let ear = [(at + count - 1) % count, at, (at + 1) % count];
    let (before, corner, after) = (section[ear[0]], section[ear[1]], section[ear[2]]);
    let intruders: Vec<V2> = (0..count)
        .filter(|k| !ear.contains(k) && within(section[*k], before, corner, after))
        .map(|k| section[k])
        .collect();
    if intruders.is_empty() {
        return [
            (before[0] + corner[0] + after[0]) / 3.0,
            (before[1] + corner[1] + after[1]) / 3.0,
        ];
    }
    let deepest = intruders[py::first_max(&intruders, |p| turn(before, after, *p).abs())];
    [
        0.5 * (corner[0] + deepest[0]),
        0.5 * (corner[1] + deepest[1]),
    ]
}

fn turn(a: V2, b: V2, c: V2) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

/// Is *point* inside or on triangle *a*, *b*, *c*? (`_within`)
fn within(point: V2, a: V2, b: V2, c: V2) -> bool {
    let turns = [turn(a, b, point), turn(b, c, point), turn(c, a, point)];
    turns.iter().all(|&t| t >= 0.0) || turns.iter().all(|&t| t <= 0.0)
}

/// Is the ring's interior empty rather than a prism of material (`_is_void`)? Only a clean
/// `OUT` at a point proved inside the section, halfway along the span, counts: `ON` and
/// `UNKNOWN` fail closed. *classifier* is the whole part's, or one solid's where the caller
/// asks of that solid alone (prismatic pockets).
pub(crate) fn is_void(
    classifier: &Classifier<'_>,
    section: &[V2],
    axis: usize,
    low: f64,
    high: f64,
) -> bool {
    let [a0, a1] = others(axis);
    let inside = interior_point(section);
    let mut point = [0.0; 3];
    point[axis] = 0.5 * (low + high);
    point[a0] = inside[0];
    point[a1] = inside[1];
    classifier.classify(point) == State::Out
}

/// The neighbouring faces that close each end of the ring (`_capped_ends`): any face, of any
/// surface type, reaching an end of the span and lying within the ring's cross-section box. An
/// end face the ring is punched through extends past it and does not cap it.
pub(crate) fn capped_ends(
    ctx: &Context<'_>,
    ring: &[usize],
    members: &BTreeSet<usize>,
    axis: usize,
    low: f64,
    high: f64,
) -> (BTreeSet<usize>, BTreeSet<usize>) {
    let part = ctx.part;
    let others = others(axis);
    let ring_low = others.map(|a| {
        ring.iter()
            .map(|&n| span(part, n, a).0)
            .fold(f64::INFINITY, f64::min)
    });
    let ring_high = others.map(|a| {
        ring.iter()
            .map(|&n| span(part, n, a).1)
            .fold(f64::NEG_INFINITY, f64::max)
    });
    let mut caps = (BTreeSet::new(), BTreeSet::new());
    for &node in ring {
        for other in part.neighbours(node) {
            if members.contains(&other) {
                continue;
            }
            let end = span(part, other, axis);
            let near_low = (end.0 - low).abs() <= SPAN_EPS || (end.1 - low).abs() <= SPAN_EPS;
            let near_high = (end.0 - high).abs() <= SPAN_EPS || (end.1 - high).abs() <= SPAN_EPS;
            if !(near_low || near_high) {
                continue;
            }
            if others.iter().enumerate().all(|(k, &a)| {
                let s = span(part, other, a);
                s.0 >= ring_low[k] - SPAN_EPS && s.1 <= ring_high[k] + SPAN_EPS
            }) {
                if near_low {
                    caps.0.insert(other);
                }
                if near_high {
                    caps.1.insert(other);
                }
            }
        }
    }
    caps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_refuses_repeated_and_collinear_corners() {
        assert_eq!(canonical(vec![[0.0, 0.0], [1.0, 0.0], [0.0, 0.0]]), None);
        assert_eq!(canonical(vec![[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]]), None);
        assert_eq!(
            canonical(vec![[1.0, 1.0], [0.0, 0.0], [1.0, 0.0]]),
            Some(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]])
        );
    }

    fn winds_around(polygon: &[V2], point: V2) -> bool {
        let mut inside = false;
        for at in 0..polygon.len() {
            let ([ux, uy], [vx, vy]) = (polygon[at], polygon[(at + 1) % polygon.len()]);
            if (uy > point[1]) != (vy > point[1]) {
                let crossing = ux + (point[1] - uy) * (vx - ux) / (vy - uy);
                if point[0] < crossing {
                    inside = !inside;
                }
            }
        }
        inside
    }

    #[test]
    fn interior_point_is_inside_convex_and_concave_sections() {
        let triangle = [[-8.0, -6.0], [8.0, -6.0], [0.0, 8.0]];
        let u_shape = [
            [-15.0, -15.0],
            [15.0, -15.0],
            [15.0, 15.0],
            [9.0, 15.0],
            [9.0, -9.0],
            [-9.0, -9.0],
            [-9.0, 15.0],
            [-15.0, 15.0],
        ];
        assert!(winds_around(&triangle, interior_point(&triangle)));
        assert!(winds_around(&u_shape, interior_point(&u_shape)));
        assert!(!winds_around(&u_shape, centroid(&u_shape)));
    }
}
