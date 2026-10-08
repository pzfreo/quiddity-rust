//! Logical face regions — the faces of one plane or cylinder joined across splits and tangent
//! seams — and what the blind-slot families ask of them: their boundary as one wire, its runs
//! of straight and circular edges, how two regions meet, and whether the sweep of a cap region
//! is empty. These are `quiddity.round_bottom_slots`' helpers, which `rectangular_blind_slots`
//! also imports.

use std::collections::BTreeSet;

use crate::kernel::brep::{Arc, Part};
use crate::kernel::classify::Classifier;
use crate::kernel::geom::{self, Bounds, COORD_FLOOR, Curve, SMOOTH_ARC_GAP, V3};
use crate::kernel::sampling::edge_interval;
use crate::kernel::volume::{Prism, PrismEdge, Probe, common_volume};

use super::planes::axis_aligned_axis;

/// A set of faces, by index (Python's `frozenset[FaceNode]`).
pub type Region = BTreeSet<usize>;

const LENGTH_REL: f64 = 1e-7;

/// Kernel-coordinate equality scaled to the largest of the relevant local lengths
/// (`_length_tolerance`).
pub fn length_tolerance(values: &[f64]) -> f64 {
    geom::length_tol(values.iter().fold(0.0, |m, v| m.max(v.abs())), LENGTH_REL)
}

/// The extents of a face's box across a plane normal to *axis*.
fn extents_across(b: &Bounds, axis: usize) -> impl Iterator<Item = f64> + '_ {
    (0..3)
        .filter(move |&i| i != axis)
        .map(|i| b.max[i] - b.min[i])
}

/// Connected same-principal-plane faces, without crossing a tangent blend (`_coplanar_region`):
/// empty when the seed is not an axis-aligned plane.
pub fn coplanar_region(part: &Part, seed: usize) -> Region {
    let Some((axis, coord)) = axis_aligned_axis(part, seed) else {
        return Region::new();
    };
    let seed_scale =
        extents_across(&part.face_bounds(seed), axis).fold(f64::NEG_INFINITY, f64::max);
    let mut found = Region::from([seed]);
    let mut pending = vec![seed];
    while let Some(current) = pending.pop() {
        for neighbour in part.neighbours(current) {
            if found.contains(&neighbour) || part.arc(current, neighbour) != Some(Arc::Smooth) {
                continue;
            }
            let Some((other_axis, other_coord)) = axis_aligned_axis(part, neighbour) else {
                continue;
            };
            let mut scales = vec![seed_scale];
            scales.extend(extents_across(&part.face_bounds(neighbour), axis));
            if other_axis == axis && (other_coord - coord).abs() <= length_tolerance(&scales) {
                found.insert(neighbour);
                pending.push(neighbour);
            }
        }
    }
    found
}

/// How two regions meet, when every pair of faces that meets does so the same way
/// (`_relation`).
pub fn relation(part: &Part, left: &Region, right: &Region) -> Option<Arc> {
    let mut kinds = Vec::new();
    for &a in left {
        for &b in right {
            if let Some(kind) = part.arc(a, b)
                && !kinds.contains(&kind)
            {
                kinds.push(kind);
            }
        }
    }
    (kinds.len() == 1).then(|| kinds[0])
}

/// The box of a region's faces (`_region_bounds`).
pub fn region_bounds(part: &Part, nodes: &Region) -> Bounds {
    let mut b = Bounds::empty();
    for &n in nodes {
        b.merge(&part.face_bounds(n));
    }
    b
}

/// The common extent of the regions along *axis*, if they all share it (`_same_span`).
pub fn same_span(part: &Part, regions: &[&Region], axis: usize) -> Option<(f64, f64)> {
    let spans: Vec<(f64, f64)> = regions
        .iter()
        .map(|r| {
            let b = region_bounds(part, r);
            (b.min[axis], b.max[axis])
        })
        .collect();
    let (low, high) = spans[0];
    let tolerance = length_tolerance(&[high - low]);
    spans[1..]
        .iter()
        .all(|&(a, b)| (a - low).abs() <= tolerance && (b - high).abs() <= tolerance)
        .then_some((low, high))
}

/// One edge of a boundary wire, run in the wire's direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WireEdge {
    pub edge: usize,
    pub forward: bool,
}

impl WireEdge {
    fn ends(&self, part: &Part) -> (V3, V3) {
        let e = &part.edges[self.edge];
        if self.forward {
            (e.start, e.end)
        } else {
            (e.end, e.start)
        }
    }

    /// A straight edge's direction in the wire (build123d `tangent_at()`).
    pub fn direction(&self, part: &Part) -> Option<V3> {
        let (a, b) = self.ends(part);
        geom::unit(geom::sub(b, a))
    }

    /// The edge's samples in the wire's direction.
    pub fn points(&self, part: &Part) -> Vec<V3> {
        let mut points = part.edges[self.edge].samples.clone();
        if !self.forward {
            points.reverse();
        }
        points
    }
}

/// What a boundary run is made of (`GeomType.LINE` / `GeomType.CIRCLE`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunKind {
    Line,
    Circle,
}

fn run_kind(part: &Part, edge: usize) -> Option<RunKind> {
    match part.edges[edge].curve {
        Curve::Line { .. } => Some(RunKind::Line),
        Curve::Circle { .. } => Some(RunKind::Circle),
        _ => None,
    }
}

/// A circular edge's radius.
pub fn edge_radius(part: &Part, edge: usize) -> Option<f64> {
    match part.edges[edge].curve {
        Curve::Circle { radius, .. } => Some(radius),
        _ => None,
    }
}

/// An edge's length: exact for lines and circular arcs, along its samples otherwise.
pub fn edge_length(part: &Part, edge: usize) -> f64 {
    let e = &part.edges[edge];
    match &e.curve {
        Curve::Line { .. } => geom::dist(e.start, e.end),
        Curve::Circle { radius, .. } => {
            let (a, b) = edge_interval(&e.curve, e.start, e.end, e.same_sense, e.is_closed());
            radius * (b - a).abs()
        }
        _ => e.samples.windows(2).map(|w| geom::dist(w[0], w[1])).sum(),
    }
}

/// The total length of a run of edges.
pub fn run_length(part: &Part, run: &[WireEdge]) -> f64 {
    run.iter().map(|w| edge_length(part, w.edge)).sum()
}

/// The edges a region's faces use once, joined end to end into its one closed boundary wire
/// (`_region_boundary_wire`): `None` when an edge is used by more than two of the faces, when
/// the boundary is not exactly one closed wire, or, with *planar*, when the wire bounds no plane.
///
/// `Wire.combine` joins ends within `COORD_FLOOR`, and so does this; each edge is turned to run
/// on from the last, so co-directed straight edges are co-directed in the wire.
pub fn region_boundary(part: &Part, nodes: &Region, planar: bool) -> Option<Vec<WireEdge>> {
    if nodes.is_empty() {
        return None;
    }
    let mut uses: Vec<(usize, usize)> = Vec::new();
    for &n in nodes {
        for e in part.face_edges(n) {
            match uses.iter_mut().find(|(edge, _)| *edge == e) {
                Some(entry) => entry.1 += 1,
                None => uses.push((e, 1)),
            }
        }
    }
    if uses.iter().any(|&(_, count)| count > 2) {
        return None;
    }
    let mut free: Vec<usize> = uses
        .into_iter()
        .filter(|&(_, count)| count == 1)
        .map(|(e, _)| e)
        .collect();
    if free.is_empty() {
        return None;
    }
    let near = |a: V3, b: V3| geom::dist(a, b) <= COORD_FLOOR;
    let first = WireEdge {
        edge: free.remove(0),
        forward: true,
    };
    let (start, mut end) = first.ends(part);
    let mut wire = vec![first];
    while !near(end, start) || wire.len() == 1 && !part.edges[first.edge].is_closed() {
        let next = free.iter().enumerate().find_map(|(i, &e)| {
            let edge = &part.edges[e];
            if near(edge.start, end) {
                Some((i, true))
            } else if near(edge.end, end) {
                Some((i, false))
            } else {
                None
            }
        });
        let (i, forward) = next?;
        let step = WireEdge {
            edge: free.remove(i),
            forward,
        };
        end = step.ends(part).1;
        wire.push(step);
    }
    if !free.is_empty() || (planar && !is_planar(part, &wire)) {
        return None;
    }
    Some(wire)
}

/// Whether a wire lies in one plane (`Face(wire)` succeeds): within `COORD_FLOOR` of the plane
/// through its samples' centroid, normal to their Newell normal.
fn is_planar(part: &Part, wire: &[WireEdge]) -> bool {
    let points: Vec<V3> = wire.iter().flat_map(|w| w.points(part)).collect();
    let n = points.len() as f64;
    let centroid = points
        .iter()
        .fold([0.0; 3], |c, p| geom::add(c, geom::scale(*p, 1.0 / n)));
    let mut normal = [0.0; 3];
    for (i, p) in points.iter().enumerate() {
        let q = points[(i + 1) % points.len()];
        normal = geom::add(
            normal,
            geom::cross(geom::sub(*p, centroid), geom::sub(q, centroid)),
        );
    }
    let Some(normal) = geom::unit(normal) else {
        return false;
    };
    points
        .iter()
        .all(|p| geom::dot(normal, geom::sub(*p, centroid)).abs() <= COORD_FLOOR)
}

/// Co-directed straight or same-radius circular runs around one boundary (`_boundary_runs`):
/// `None` when an edge is neither a line nor a circle. A run split by the wire's seam is joined.
pub fn boundary_runs(part: &Part, wire: &[WireEdge]) -> Option<Vec<(RunKind, Vec<WireEdge>)>> {
    if wire.is_empty() {
        return None;
    }
    let kinds: Vec<RunKind> = wire
        .iter()
        .map(|w| run_kind(part, w.edge))
        .collect::<Option<_>>()?;
    // Whether *b* continues the run *a* ends.
    let continues = |kind: RunKind, a: &WireEdge, b: &WireEdge| match kind {
        RunKind::Line => match (a.direction(part), b.direction(part)) {
            (Some(p), Some(q)) => 1.0 - geom::dot(p, q) <= SMOOTH_ARC_GAP,
            _ => false,
        },
        RunKind::Circle => {
            let (p, q) = (
                edge_radius(part, a.edge).unwrap_or(f64::NAN),
                edge_radius(part, b.edge).unwrap_or(f64::NAN),
            );
            (p - q).abs() <= length_tolerance(&[p, q])
        }
    };
    let mut groups: Vec<(RunKind, Vec<WireEdge>)> = Vec::new();
    for (w, &kind) in wire.iter().zip(&kinds) {
        match groups.last_mut() {
            Some((k, members)) if *k == kind && continues(kind, members.last().unwrap(), w) => {
                members.push(*w)
            }
            _ => groups.push((kind, vec![*w])),
        }
    }
    if groups.len() > 1 && groups[0].0 == groups[groups.len() - 1].0 {
        let kind = groups[0].0;
        let tail = groups[groups.len() - 1].1.last().unwrap();
        if continues(kind, tail, &groups[0].1[0]) {
            let (_, mut joined) = groups.pop().unwrap();
            joined.append(&mut groups[0].1);
            groups[0].1 = joined;
        }
    }
    Some(groups)
}

/// Whether a planar region is one valid hole-free principal rectangle (`_principal_rectangle`).
pub fn principal_rectangle(part: &Part, nodes: &Region, normal_axis: usize) -> bool {
    let Some(wire) = region_boundary(part, nodes, true) else {
        return false;
    };
    if wire
        .iter()
        .any(|w| run_kind(part, w.edge) != Some(RunKind::Line))
    {
        return false;
    }
    let Some(directions): Option<Vec<V3>> = wire.iter().map(|w| w.direction(part)).collect() else {
        return false;
    };
    let mut runs = vec![directions[0]];
    for &d in &directions[1..] {
        if 1.0 - geom::dot(*runs.last().unwrap(), d) > SMOOTH_ARC_GAP {
            runs.push(d);
        }
    }
    if runs.len() > 1 && 1.0 - geom::dot(*runs.last().unwrap(), runs[0]) <= SMOOTH_ARC_GAP {
        runs.pop();
    }
    let in_plane: Vec<usize> = (0..3).filter(|&a| a != normal_axis).collect();
    let mut run_axes = Vec::new();
    for d in runs {
        let aligned: Vec<usize> = in_plane
            .iter()
            .copied()
            .filter(|&a| 1.0 - d[a].abs() <= SMOOTH_ARC_GAP)
            .collect();
        if aligned.len() != 1 {
            return false;
        }
        run_axes.push(aligned[0]);
    }
    run_axes.len() == 4
        && in_plane
            .iter()
            .all(|a| run_axes.iter().filter(|&&r| r == *a).count() == 2)
}

/// Whether one plane normal to *normal_axis* at *station* (within the tolerance of *nominal*),
/// taken as its coplanar region, meets every source region and only convexly
/// (`_common_convex_context`): the stock face a feature opens through.
pub fn common_convex_context(
    part: &Part,
    sources: &[&Region],
    normal_axis: usize,
    station: f64,
    nominal: f64,
) -> bool {
    let neighbours: Vec<Region> = sources
        .iter()
        .map(|s| s.iter().flat_map(|&m| part.neighbours(m)).collect())
        .collect();
    let seeds: Region = neighbours.iter().flatten().copied().collect();
    let mut seen = Region::new();
    for seed in seeds {
        if seen.contains(&seed) {
            continue;
        }
        let region = coplanar_region(part, seed);
        seen.extend(&region);
        match axis_aligned_axis(part, seed) {
            Some((axis, coord))
                if axis == normal_axis
                    && (coord - station).abs() <= length_tolerance(&[nominal]) => {}
            _ => continue,
        }
        let all_convex = sources.iter().all(|source| {
            let kinds: Vec<Option<Arc>> = source
                .iter()
                .flat_map(|&m| {
                    part.neighbours(m)
                        .into_iter()
                        .filter(|n| region.contains(n))
                        .map(move |n| part.arc(m, n))
                })
                .collect();
            !kinds.is_empty() && kinds.iter().all(|k| *k == Some(Arc::Convex))
        });
        if all_convex {
            return true;
        }
    }
    false
}

/// Whether the cap region bounded by *wire* (in the plane `run = cap_station`), swept along
/// *run* to *open_station*, shares no volume with the solid (`_empty_sweep`).
pub fn empty_sweep(
    solid: &Classifier<'_>,
    part: &Part,
    wire: &[WireEdge],
    run: usize,
    cap_station: f64,
    open_station: f64,
) -> bool {
    let prism = Prism {
        axis: run,
        lo: cap_station.min(open_station),
        hi: cap_station.max(open_station),
        loops: vec![
            wire.iter()
                .map(|w| PrismEdge {
                    points: w.points(part),
                    straight: run_kind(part, w.edge) == Some(RunKind::Line),
                })
                .collect(),
        ],
    };
    // An unanswered probe proves no emptiness.
    common_volume(solid, &Probe::Prism(prism)) == Some(0.0)
}
