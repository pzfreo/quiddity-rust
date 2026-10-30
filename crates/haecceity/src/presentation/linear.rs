//! Model-space linear dimensions. Arrow triangles and shortest-first, outward ladder rule
//! ported from Draftwright aaa7a73, dims/placement.rs (respace/place_linear_at/arrow) and
//! dims/leaders.rs (head), Paul Fremantle, at the copyright owner's direction for #137.
//! No sheets, lanes around other views, or authoring policy are imported.
use super::{Callout, placement::Placed};
use crate::{
    Part,
    geom::{self, Bounds, Surface, V3},
    pmi::*,
    rays::RayCaster,
};

const AXES: [V3; 3] = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
fn canonical(v: V3) -> V3 {
    let v = geom::unit(v).unwrap_or([1., 0., 0.]);
    if v.iter().find(|x| x.abs() > 1e-9).is_some_and(|x| *x < 0.) {
        geom::scale(v, -1.)
    } else {
        v
    }
}
fn range(b: Bounds, d: V3) -> [f64; 2] {
    let mut r = [f64::INFINITY, f64::NEG_INFINITY];
    for x in [b.min[0], b.max[0]] {
        for y in [b.min[1], b.max[1]] {
            for z in [b.min[2], b.max[2]] {
                let t = geom::dot([x, y, z], d);
                r[0] = r[0].min(t);
                r[1] = r[1].max(t);
            }
        }
    }
    r
}
#[derive(Clone, Debug)]
enum Element {
    Plane(usize),
    Axis(usize),
    Point(V3),
}
impl Element {
    fn bounds(&self, part: &Part) -> Bounds {
        match *self {
            Self::Plane(f) | Self::Axis(f) => part.face_bounds(f),
            Self::Point(p) => Bounds { min: p, max: p },
        }
    }
    fn point(&self, part: &Part) -> V3 {
        match *self {
            Self::Point(p) => p,
            Self::Axis(f) => match part.faces[f].surface {
                Surface::Cylinder { frame, .. }
                | Surface::Cone { frame, .. }
                | Surface::Torus { frame, .. } => frame.origin,
                _ => self.bounds(part).centre(),
            },
            Self::Plane(f) => match part.faces[f].surface {
                Surface::Plane { frame } => frame.origin,
                _ => self.bounds(part).centre(),
            },
        }
    }
    fn axis(&self, part: &Part) -> Option<V3> {
        match *self {
            Self::Axis(f) => match part.faces[f].surface {
                Surface::Cylinder { frame, .. }
                | Surface::Cone { frame, .. }
                | Surface::Torus { frame, .. } => Some(canonical(frame.z)),
                _ => None,
            },
            _ => None,
        }
    }
    fn normal(&self, part: &Part) -> Option<V3> {
        match *self {
            Self::Plane(f) => match part.faces[f].surface {
                Surface::Plane { frame } => Some(canonical(frame.z)),
                _ => None,
            },
            _ => None,
        }
    }
    fn foot(
        &self,
        part: &Part,
        caster: &RayCaster<'_>,
        d: V3,
        v: V3,
        w: V3,
        level: f64,
    ) -> Option<V3> {
        let p = self.point(part);
        match *self {
            Self::Point(p) => ((geom::dot(p, w) - level).abs() < 1e-6).then_some(p),
            Self::Axis(_) => {
                let a = self.axis(part)?;
                let den = geom::dot(a, w);
                if den.abs() < 1e-9 {
                    return ((geom::dot(p, w) - level).abs() < 1e-6).then_some(p);
                }
                Some(geom::add(
                    p,
                    geom::scale(a, (level - geom::dot(p, w)) / den),
                ))
            }
            Self::Plane(f) => {
                let extent = part.bounds().diagonal().max(1.);
                let along = range(part.bounds(), d)[0] - extent;
                let r = range(self.bounds(part), v);
                // Interior stations first, then trim endpoints. RayCaster tests the real face,
                // including its holes and split boundaries, not only its supporting plane.
                for fraction in [0.95, 0.75, 0.5, 0.25, 0.05, 1., 0.] {
                    let t = r[0] + fraction * (r[1] - r[0]);
                    let origin = geom::add(
                        geom::scale(d, along),
                        geom::add(geom::scale(v, t), geom::scale(w, level)),
                    );
                    if let Some(hit) = caster.hits_on(&[f], origin, d, extent * 4.)?.first() {
                        return Some(geom::add(origin, geom::scale(d, hit.t)));
                    }
                }
                None
            }
        }
    }
}
fn elements(part: &Part, p: &PartPmi, feature: FeatureId, depth: usize) -> Vec<Element> {
    if depth > p.features.len() {
        return Vec::new();
    }
    if let Feature::Derived {
        geometry: Some(g), ..
    } = &p.features[feature.0]
        && let Some(g) = p.derived_geometry.get(g.0)
        && g.kind == DerivedGeometryKind::MedianPlane
    {
        return vec![Element::Point(g.point.clone().map(|x| x.mm()))];
    }
    match &p.features[feature.0] {
        Feature::Items(items) => items
            .iter()
            .filter_map(|a| match a {
                Anchor::Face(f) => match part.faces.get(f.0)?.surface {
                    Surface::Plane { .. } => Some(Element::Plane(f.0)),
                    Surface::Cylinder { .. } | Surface::Cone { .. } | Surface::Torus { .. } => {
                        Some(Element::Axis(f.0))
                    }
                    _ => None,
                },
                Anchor::Edge(e) => part.edges.get(e.0).map(|e| Element::Point(e.midpoint())),
                Anchor::Geometry(g) => {
                    let g = p.geometry.get(g.0)?;
                    crate::pmi::write::geometry_point(&g.item).map(|q| {
                        Element::Point(q.map(|x| {
                            x * g.length_unit.as_ref().map_or(1., LengthUnit::millimetres)
                        }))
                    })
                }
            })
            .collect(),
        Feature::Group { members, .. } | Feature::Derived { from: members, .. } => members
            .iter()
            .flat_map(|f| elements(part, p, *f, depth + 1))
            .collect(),
    }
}
/// True for a straight distance or size, excluding angular and circular/radial notation.
pub fn is_linear(d: &Dimension) -> bool {
    super::dimension_type(d) == "linear dimension"
        && !matches!(
            d.kind,
            DimensionKind::Location {
                kind: LocationKind::CurvedDistance,
                ..
            } | DimensionKind::Size {
                kind: SizeKind::CurveLength,
                ..
            }
        )
}
/// Two real witness feet, the measured direction and an outward direction, in millimetres.
#[derive(Clone, Debug)]
pub struct Measurement {
    pub feet: [V3; 2],
    pub direction: V3,
    pub outward: V3,
}
impl Measurement {
    pub fn length(&self) -> f64 {
        geom::dot(geom::sub(self.feet[1], self.feet[0]), self.direction).abs()
    }
}
fn pair(
    part: &Part,
    caster: &RayCaster<'_>,
    a: &Element,
    b: &Element,
    nominal: f64,
) -> Option<(f64, Measurement)> {
    let delta = geom::sub(b.point(part), a.point(part));
    let mut dirs = Vec::new();
    dirs.extend(a.normal(part));
    dirs.extend(b.normal(part));
    dirs.extend(AXES);
    dirs.extend(geom::unit(delta));
    let mut best: Option<(f64, Measurement)> = None;
    for (priority, d) in dirs.into_iter().enumerate() {
        let d = canonical(d);
        if a.normal(part)
            .is_some_and(|n| geom::dot(n, d).abs() < 1. - 1e-8)
            || b.normal(part)
                .is_some_and(|n| geom::dot(n, d).abs() < 1. - 1e-8)
        {
            continue;
        }
        // An axis is a centreline, not a vertex on the cylindrical wall.
        if a.axis(part)
            .into_iter()
            .chain(b.axis(part))
            .any(|axis| geom::dot(axis, d).abs() > 1e-8)
        {
            continue;
        }
        let separation = geom::dot(delta, d).abs();
        if separation < 1e-9 {
            continue;
        }
        let score = (separation - nominal).abs() / nominal.max(1e-9) + priority as f64 * 1e-12;
        if best.as_ref().is_some_and(|(s, _)| *s <= score) {
            continue;
        }
        let views = if matches!((a, b), (Element::Point(_), Element::Point(_)))
            && geom::norm(geom::cross(d, delta)) > 1e-8
        {
            vec![canonical(geom::cross(d, delta))]
        } else if let Some(axis) = a.axis(part).or_else(|| b.axis(part)) {
            vec![axis]
        } else {
            let mut candidates = AXES;
            let mut extents = [0, 1, 2];
            extents.sort_by(|&a, &b| {
                (part.bounds().max[b] - part.bounds().min[b])
                    .total_cmp(&(part.bounds().max[a] - part.bounds().min[a]))
            });
            let rod = part.bounds().max_extent()
                > 3. * (part.bounds().max[extents[1]] - part.bounds().min[extents[1]]);
            candidates.sort_by(|x, y| {
                let bx = range(part.bounds(), *x);
                let by = range(part.bounds(), *y);
                if rod && geom::dot(d, AXES[extents[0]]).abs() > 1. - 1e-8 {
                    (by[1] - by[0]).total_cmp(&(bx[1] - bx[0]))
                } else {
                    (bx[1] - bx[0]).total_cmp(&(by[1] - by[0]))
                }
            });
            candidates
                .into_iter()
                .filter_map(|x| {
                    let p = geom::sub(x, geom::scale(d, geom::dot(x, d)));
                    (geom::norm(p) > 1e-8).then(|| canonical(p))
                })
                .collect::<Vec<_>>()
        };
        'view: for w in views {
            let w = canonical(w);
            let mut v = canonical(geom::cross(w, d));
            let centre = geom::scale(
                geom::add(a.bounds(part).centre(), b.bounds(part).centre()),
                0.5,
            );
            if geom::dot(geom::sub(centre, part.bounds().centre()), v) < -1e-9 {
                v = geom::scale(v, -1.);
            }
            let ar = range(a.bounds(part), w);
            let br = range(b.bounds(part), w);
            let lo = ar[0].max(br[0]);
            let hi = ar[1].min(br[1]);
            if lo > hi + 1e-7 {
                continue;
            }
            // Bore witnesses meet the axis at the face/mouth. Other witnesses use a common
            // section through both trimmed faces, trying interior sections before boundaries.
            let fractions: &[f64] = if a.axis(part).is_some() || b.axis(part).is_some() {
                &[1., 0., 0.5]
            } else {
                &[0.5, 0.75, 0.25, 0.95, 0.05, 1., 0.]
            };
            for fraction in fractions {
                let level = lo + fraction * (hi - lo);
                if let (Some(pa), Some(pb)) = (
                    a.foot(part, caster, d, v, w, level),
                    b.foot(part, caster, d, v, w, level),
                ) {
                    best = Some((
                        score
                            + ((range(part.bounds(), v)[1] - geom::dot(pa, v))
                                + (range(part.bounds(), v)[1] - geom::dot(pb, v)))
                                / part.bounds().max_extent()
                                * 1e-9,
                        Measurement {
                            feet: [pa, pb],
                            direction: d,
                            outward: v,
                        },
                    ));
                    break 'view;
                }
            }
        }
    }
    best
}
// Exact boundary witnesses for elements without an analytic plane/axis description.
// They remain on the semantic targets; nominal values never manufacture geometry.
fn witnesses(part: &Part, p: &PartPmi, feature: FeatureId, depth: usize) -> Vec<Element> {
    if depth > p.features.len() {
        return Vec::new();
    }
    match &p.features[feature.0] {
        Feature::Items(items) => items
            .iter()
            .flat_map(|a| match a {
                Anchor::Face(f) => {
                    let face = &part.faces[f.0];
                    face.loops
                        .iter()
                        .flat_map(|lp| lp.edges.iter())
                        .flat_map(|&(e, _)| {
                            let edge = &part.edges[e];
                            [edge.start, edge.midpoint(), edge.end]
                        })
                        .take(32)
                        .map(Element::Point)
                        .collect::<Vec<_>>()
                }
                Anchor::Edge(e) => {
                    let e = &part.edges[e.0];
                    vec![
                        Element::Point(e.start),
                        Element::Point(e.midpoint()),
                        Element::Point(e.end),
                    ]
                }
                Anchor::Geometry(_) => elements(part, p, feature, depth + 1),
            })
            .take(32)
            .collect(),
        Feature::Group { members, .. } | Feature::Derived { from: members, .. } => members
            .iter()
            .flat_map(|f| witnesses(part, p, *f, depth + 1))
            .take(32)
            .collect(),
    }
}
/// Resolve the two semantic elements. Size aspects containing opposite faces are measured
/// between those faces; a single planar size face finds its opposite on the same part.
pub fn measurements(part: &Part, p: &PartPmi) -> Vec<Option<Measurement>> {
    let caster = RayCaster::for_faces(part, (0..part.faces.len()).collect());
    p.dimensions
        .iter()
        .map(|d| {
            if !is_linear(d) {
                return None;
            }
            let nominal = d.nominal.as_ref().map_or(0., Value::si).abs();
            let (a, b) = match d.kind {
                DimensionKind::Location { from, to, .. } => {
                    (elements(part, p, from, 0), elements(part, p, to, 0))
                }
                DimensionKind::Size { feature, .. } => {
                    let a = elements(part, p, feature, 0);
                    let b = if a.len() > 1 {
                        a.clone()
                    } else {
                        (0..part.faces.len())
                            .filter(|&f| matches!(part.faces[f].surface, Surface::Plane { .. }))
                            .map(Element::Plane)
                            .collect()
                    };
                    (a, b)
                }
            };
            let resolved = a
                .iter()
                .flat_map(|a| b.iter().filter_map(|b| pair(part, &caster, a, b, nominal)))
                .min_by(|a, b| a.0.total_cmp(&b.0));
            resolved
                .or_else(|| {
                    let (a, b) = match d.kind {
                        DimensionKind::Location { from, to, .. } => {
                            (witnesses(part, p, from, 0), witnesses(part, p, to, 0))
                        }
                        DimensionKind::Size { feature, .. } => {
                            let a = witnesses(part, p, feature, 0);
                            (a.clone(), a)
                        }
                    };
                    a.iter()
                        .flat_map(|a| b.iter().filter_map(|b| pair(part, &caster, a, b, nominal)))
                        .min_by(|a, b| a.0.total_cmp(&b.0))
                })
                .map(|(_, m)| m)
        })
        .collect()
}

#[derive(Clone, Debug)]
pub struct LinearPlaced {
    pub placed: Placed,
    pub feet: [V3; 2],
    pub dimension: [V3; 2],
    pub text_center: V3,
    pub text_bounds: Bounds,
    pub offset: f64,
    pub gap: f64,
}
/// Model-space ladder: shortest span first, ties in semantic order, then the first free
/// outward rung. This is Draftwright's nesting rule, without its sheet layout.
pub struct Ladder {
    bounds: Bounds,
    groups: Vec<(V3, V3, usize)>,
    labels: Vec<Bounds>,
}
impl Ladder {
    pub fn new(bounds: Bounds) -> Self {
        Self {
            bounds,
            groups: Vec::new(),
            labels: Vec::new(),
        }
    }
    pub fn reserve(&mut self, b: Bounds) {
        self.labels.push(b);
    }
    pub fn draw(&mut self, m: &Measurement, c: &Callout) -> LinearPlaced {
        let d = m.direction;
        let v = m.outward;
        let normal = canonical(geom::cross(d, v));
        let up = geom::cross(normal, d);
        let length = m.length();
        let h = self.bounds.max_extent() / 125.;
        let scale = (h / 3.).min(length * 0.65 / c.size[0].max(1.));
        let arrow = (0.9 * h).min(length / 8.);
        let half = arrow / 6.;
        let gap = h / 3.;
        let overshoot = 2. * h / 3.;
        let edge = range(self.bounds, v)[1];
        let group = self
            .groups
            .iter()
            .position(|(u, w, _)| geom::dot(*u, d) > 1. - 1e-8 && geom::dot(*w, v) > 1. - 1e-8);
        let mut rung = group.map_or(0, |i| self.groups[i].2);
        let (centre, offset, label_bounds) = loop {
            let offset = (10. + rung as f64 * 7.) * h / 3.;
            let level = edge + offset;
            let a = geom::add(m.feet[0], geom::scale(v, level - geom::dot(m.feet[0], v)));
            let b = geom::add(m.feet[1], geom::scale(v, level - geom::dot(m.feet[1], v)));
            let centre = geom::scale(geom::add(a, b), 0.5);
            let mut bb = Bounds::empty();
            for x in [-c.size[0] / 2., c.size[0] / 2.] {
                for y in [-c.size[1] / 2., c.size[1] / 2.] {
                    bb.add(geom::add(
                        centre,
                        geom::add(geom::scale(d, x * scale), geom::scale(v, y * scale)),
                    ));
                }
            }
            let padding = h / 5.;
            let overlap = self.labels.iter().any(|b| {
                (0..3).all(|k| bb.min[k] < b.max[k] + padding && bb.max[k] + padding > b.min[k])
            });
            rung += 1;
            if !overlap {
                break (centre, offset, bb);
            }
        };
        if let Some(i) = group {
            self.groups[i].2 = rung;
        } else {
            self.groups.push((d, v, rung));
        }
        self.labels.push(label_bounds);
        let level = edge + offset;
        let mut ends = m
            .feet
            .map(|p| geom::add(p, geom::scale(v, level - geom::dot(p, v))));
        if geom::dot(geom::sub(ends[1], ends[0]), d) < 0. {
            ends.swap(0, 1);
        }
        let half_text = c.size[0] * scale / 2. + gap.min(length * 0.02);
        let mut lines = Vec::new();
        for foot in m.feet {
            lines.push(vec![
                geom::add(foot, geom::scale(v, gap)),
                geom::add(foot, geom::scale(v, level + overshoot - geom::dot(foot, v))),
            ]);
        }
        // The dimension line has a text break; its arrow-to-arrow span is the measurement.
        lines.push(vec![ends[0], geom::sub(centre, geom::scale(d, half_text))]);
        lines.push(vec![geom::add(centre, geom::scale(d, half_text)), ends[1]]);
        for (tip, sign) in [(ends[0], 1.), (ends[1], -1.)] {
            let tail = geom::add(tip, geom::scale(d, sign * arrow));
            let a = geom::add(tail, geom::scale(v, half));
            let b = geom::sub(tail, geom::scale(v, half));
            lines.extend([vec![tip, a], vec![a, b], vec![b, tip]]);
        }
        lines.extend(c.polylines().into_iter().map(|l| {
            l.into_iter()
                .map(|p| {
                    geom::add(
                        centre,
                        geom::add(
                            geom::scale(d, (p[0] - c.size[0] / 2.) * scale),
                            geom::scale(up, (p[1] - c.size[1] / 2.) * scale),
                        ),
                    )
                })
                .collect()
        }));
        LinearPlaced {
            placed: Placed {
                anchor: m.feet[0],
                origin: centre,
                normal,
                along: d,
                lines,
            },
            feet: m.feet,
            dimension: ends,
            text_center: centre,
            text_bounds: label_bounds,
            offset,
            gap,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arrow_triangles_and_text_keep_their_frame_in_a_rotated_plane() {
        let d = canonical([1., 2., 3.]);
        let v = canonical(geom::cross(d, [0., 1., 0.]));
        let m = Measurement {
            feet: [
                [1., 2., 3.],
                geom::add(
                    [1., 2., 3.],
                    geom::add(geom::scale(d, 12.), geom::scale(v, 2.)),
                ),
            ],
            direction: d,
            outward: v,
        };
        let mut ladder = Ladder::new(Bounds {
            min: [-10.; 3],
            max: [10.; 3],
        });
        let c = super::super::label("12 ±0.1");
        let p = ladder.draw(&m, &c);
        assert!((geom::dist(p.dimension[0], p.dimension[1]) - 12.).abs() < 1e-12);
        assert!(
            geom::dist(
                p.text_center,
                geom::scale(geom::add(p.dimension[0], p.dimension[1]), 0.5)
            ) < 1e-12
        );
        assert!(
            (geom::dot(
                p.placed.normal,
                geom::cross(d, geom::cross(p.placed.normal, d))
            ) - 1.)
                .abs()
                < 1e-12
        );
        for q in p.placed.lines.iter().flatten() {
            assert!(geom::dot(geom::sub(*q, p.text_center), p.placed.normal).abs() < 1e-12);
        }
        // Ported Draftwright head: length 0.9 text heights, flare half-width = length/6.
        let tip = p.placed.lines[4][0];
        let a = p.placed.lines[4][1];
        let b = p.placed.lines[5][1];
        let tail = geom::scale(geom::add(a, b), 0.5);
        assert!((geom::dist(a, b) / geom::dist(tip, tail) - 1. / 3.).abs() < 1e-12);
        let up = geom::cross(p.placed.normal, d);
        let scale = (20_f64 / 125. / 3.).min(12. * 0.65 / c.size[0]);
        let expected: Vec<_> = c
            .polylines()
            .into_iter()
            .flatten()
            .map(|q| {
                [
                    (q[0] - c.size[0] / 2.) * scale,
                    (q[1] - c.size[1] / 2.) * scale,
                ]
            })
            .collect();
        for (q, e) in p.placed.lines.iter().skip(10).flatten().zip(expected) {
            let delta = geom::sub(*q, p.text_center);
            assert!(
                (geom::dot(delta, d) - e[0]).abs() < 1e-12
                    && (geom::dot(delta, up) - e[1]).abs() < 1e-12
            );
        }
    }
}
