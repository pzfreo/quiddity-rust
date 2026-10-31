//! Feature-local model annotation placement and a deterministic stagger; no sheet layout.
use super::Callout;
use crate::geom::{self, Bounds, V3};
pub use crate::pmi::sites::{face_site, feature_site, surface_sites, text_height};

#[derive(Clone, Debug)]
pub struct Placed {
    pub anchor: V3,
    pub origin: V3,
    pub normal: V3,
    pub along: V3,
    pub lines: Vec<Vec<V3>>,
}

#[derive(Clone)]
pub struct Layout {
    bounds: Bounds,
    axes: [usize; 3],
    height: f64,
    occupied: Vec<Bounds>,
    callouts: Vec<Vec<V3>>,
}
impl Layout {
    pub fn new(bounds: Bounds) -> Self {
        Self::with_unit(bounds, 1.)
    }
    pub fn with_unit(bounds: Bounds, mm_per_unit: f64) -> Self {
        let mut axes = [0, 1, 2];
        axes.sort_by(|&a, &b| {
            (bounds.max[b] - bounds.min[b])
                .total_cmp(&(bounds.max[a] - bounds.min[a]))
                .then(a.cmp(&b))
        });
        let largest = if bounds.max_extent().is_finite() {
            bounds.max_extent().max(1e-6)
        } else {
            1.
        };
        Self {
            bounds,
            axes,
            height: text_height(largest * mm_per_unit) / mm_per_unit,
            occupied: Vec::new(),
            callouts: Vec::new(),
        }
    }
    pub fn text_height(&self) -> f64 {
        self.height
    }
    /// Reserve completed graphics (including leaders) before placing the notes block.
    pub fn reserve(&mut self, lines: &[Vec<V3>]) {
        let mut b = Bounds::empty();
        for &p in lines.iter().flatten() {
            b.add(p);
        }
        if b.max_extent().is_finite() {
            self.occupied.push(b);
        }
    }
    /// ISO 1302's actual tip touches an interior face site; its legs rise normally.
    /// A small fixed list of interior stations avoids existing annotation ink.
    pub fn surface(&mut self, sites: &[(V3, V3)], c: &Callout) -> Option<Placed> {
        let tip = c.strokes.first()?[1];
        let scale = (self.height / 3.)
            .min(0.32 * self.bounds.max_extent().max(self.height) / c.size[0].max(1.));
        let local = c.polylines();
        let mut best = None;
        for &(anchor, outward) in sites {
            let outward = geom::unit(outward)?;
            let mut along = self.along(outward);
            let mut normal = geom::unit(geom::cross(along, outward))?;
            let k = geom::dominant_axis(normal);
            if (k == 1 && normal[k] > 0.) || (k != 1 && normal[k] < 0.) {
                normal = geom::scale(normal, -1.);
                along = geom::scale(along, -1.);
            }
            let at = |p: [f64; 2]| {
                geom::add(
                    anchor,
                    geom::add(
                        geom::scale(along, (p[0] - tip[0]) * scale),
                        geom::scale(outward, (p[1] - tip[1]) * scale),
                    ),
                )
                .map(|x| (x * 1e6).round() / 1e6 + 0.)
            };
            let lines: Vec<Vec<V3>> = local
                .iter()
                .map(|line| line.iter().copied().map(at).collect())
                .collect();
            let mut bounds = Bounds::empty();
            for &p in lines.iter().flatten() {
                bounds.add(p);
            }
            let collisions = self
                .occupied
                .iter()
                .filter(|b| boxes_overlap(bounds, **b, 0.2 * self.height))
                .count();
            if best
                .as_ref()
                .is_none_or(|(score, _, _)| collisions < *score)
            {
                best = Some((
                    collisions,
                    Placed {
                        anchor: at(tip),
                        origin: at(tip),
                        normal,
                        along,
                        lines,
                    },
                    bounds,
                ));
            }
            if collisions == 0 {
                break;
            }
        }
        let (_, placed, bounds) = best?;
        self.occupied.push(bounds);
        Some(placed)
    }
    /// Front-facing notes at one of a fixed set of outside-box stations. Choose
    /// the most compact clear station; double the gap only if none clears all
    /// seven views. Full leaders/extensions reserve space, not only text boxes.
    pub fn notes(&mut self, c: &Callout) -> Placed {
        let scale = self.height / 3.;
        let local = c.polylines();
        let mut ink = Bounds::empty();
        for p in local.iter().flatten() {
            ink.add([p[0] * scale, 0., p[1] * scale]);
        }
        let half = geom::scale(geom::sub(ink.max, ink.min), 0.5);
        let mut occupied = self.bounds;
        for b in &self.occupied {
            occupied.add(b.min);
            occupied.add(b.max);
        }
        let centre = occupied.centre();
        let gap = 2. * self.height;
        let mut distance = gap;
        let origin = loop {
            let stations: [[f64; 3]; 3] = std::array::from_fn(|k| {
                [
                    occupied.min[k] - half[k] - distance,
                    centre[k],
                    occupied.max[k] + half[k] + distance,
                ]
            });
            let mut best: Option<(f64, f64, V3)> = None;
            for x in stations[0] {
                for y in stations[1] {
                    for z in stations[2] {
                        let centre = [x, y, z];
                        let bounds = Bounds {
                            min: geom::sub(centre, half),
                            max: geom::add(centre, half),
                        };
                        if !std::iter::once(&self.bounds)
                            .chain(&self.occupied)
                            .all(|other| {
                                super::linear::VIEWS.into_iter().all(|view| {
                                    projected_bounds_clearance(bounds, *other, view) >= gap
                                })
                            })
                        {
                            continue;
                        }
                        let mut combined = occupied;
                        combined.add(bounds.min);
                        combined.add(bounds.max);
                        let fit = combined.max_extent();
                        let reach = geom::dist(centre, self.bounds.centre());
                        if best.as_ref().is_none_or(|(f, r, _)| {
                            fit.total_cmp(f).then(reach.total_cmp(r)).is_lt()
                        }) {
                            best = Some((fit, reach, geom::sub(centre, ink.centre())));
                        }
                    }
                }
            }
            if let Some((_, _, origin)) = best {
                break origin;
            }
            // Stations offset on two axes are parallel to none of the seven
            // views, so a finite occupied extent always has a clear outer layer.
            distance *= 2.;
        };
        let lines = local
            .iter()
            .map(|line| {
                line.iter()
                    .map(|p| {
                        [
                            origin[0] + p[0] * scale,
                            origin[1],
                            origin[2] + p[1] * scale,
                        ]
                    })
                    .collect()
            })
            .collect::<Vec<Vec<V3>>>();
        self.reserve(&lines);
        Placed {
            anchor: origin,
            origin,
            normal: [0., -1., 0.],
            along: [1., 0., 0.],
            lines,
        }
    }
    fn along(&self, normal: V3) -> V3 {
        self.axes
            .iter()
            .find_map(|&k| {
                let mut a = [0.; 3];
                a[k] = 1.;
                let p = geom::sub(a, geom::scale(normal, geom::dot(a, normal)));
                (geom::norm(p) > 0.5).then(|| geom::unit(p)).flatten()
            })
            .unwrap_or([1., 0., 0.])
    }
    /// A diameter/control frame seen end-on to its feature axis, beside its feature.
    pub fn place(&mut self, anchor: V3, axis: Option<V3>, c: &Callout) -> Placed {
        let mut view = axis.and_then(geom::unit).unwrap_or_else(|| {
            let mut n = [0.; 3];
            n[self.axes[2]] = 1.;
            n
        });
        let along = self.along(view);
        let mut outward = geom::unit(geom::cross(view, along)).unwrap_or([0., 1., 0.]);
        if geom::dot(geom::sub(anchor, self.bounds.centre()), outward) < 0. {
            outward = geom::scale(outward, -1.);
        }
        if geom::dominant_axis(view) == 1 {
            view = geom::scale(view, -1.);
        }
        self.draw(anchor, [along, outward, view], c, false, None)
    }
    /// The datum's plane contains its face normal, and the leader leaves the face normally.
    pub fn on_face(&mut self, anchor: V3, outward: V3, c: &Callout) -> Placed {
        self.face_callout(anchor, outward, c, None)
    }
    pub fn on_feature(&mut self, anchor: V3, outward: V3, c: &Callout, extent: f64) -> Placed {
        let outward = geom::unit(outward).unwrap_or([0., 0., 1.]);
        let mut best = None;
        // Try both readable tangents around the feature normal. A small bore need
        // not inherit a view in which its frame collides with a nearby datum.
        for k in self.axes {
            let mut a = [0.; 3];
            a[k] = 1.;
            let Some(along) = geom::unit(geom::sub(a, geom::scale(outward, geom::dot(a, outward))))
            else {
                continue;
            };
            let Some(mut normal) = geom::unit(geom::cross(along, outward)) else {
                continue;
            };
            let axis = geom::dominant_axis(normal);
            if (axis == 1 && normal[axis] > 0.) || (axis != 1 && normal[axis] < 0.) {
                normal = geom::scale(normal, -1.);
            }
            let mut candidate = self.clone();
            let p = candidate.draw(anchor, [along, outward, normal], c, true, Some(extent));
            let reach = p
                .lines
                .last()
                .into_iter()
                .flatten()
                .map(|q| geom::dist(anchor, *q))
                .fold(0., f64::max);
            if best.as_ref().is_none_or(|(r, _, _)| reach < *r) {
                best = Some((reach, p, candidate));
            }
        }
        if let Some((_, p, layout)) = best {
            *self = layout;
            p
        } else {
            self.face_callout(anchor, outward, c, Some(extent))
        }
    }
    /// End-on frames on axial features use either radial side and a corner
    /// attachment, keeping their leaders short and their projected ink separated.
    pub(crate) fn on_axis(&mut self, anchor: V3, axis: V3, c: &Callout, extent: f64) -> Placed {
        let mut best = None;
        // End-on is also a valid plane for an axial tolerance. It lets a frame
        // share the diameter's plane instead of crossing it in isometric views.
        let along = self.along(axis);
        let radial = geom::unit(geom::cross(axis, along)).unwrap_or([0., 1., 0.]);
        for outward in [radial, geom::scale(radial, -1.)] {
            let mut candidate = self.clone();
            let placed = candidate.draw_columns(
                anchor,
                [along, outward, axis],
                c,
                (true, Some(extent)),
                Some(&[-1., 1., -2., 2., -3., 3.]),
            );
            let reach = placed
                .lines
                .last()
                .into_iter()
                .flatten()
                .map(|p| geom::dist(anchor, *p))
                .fold(0., f64::max);
            if best.as_ref().is_none_or(|(r, _, _)| reach < *r) {
                best = Some((reach, placed, candidate));
            }
        }
        if let Some((_, placed, layout)) = best {
            *self = layout;
            placed
        } else {
            self.on_feature(anchor, axis, c, extent)
        }
    }
    fn face_callout(
        &mut self,
        anchor: V3,
        outward: V3,
        c: &Callout,
        extent: Option<f64>,
    ) -> Placed {
        let outward = geom::unit(outward).unwrap_or([0., 0., 1.]);
        let along = self.along(outward);
        let mut view = geom::unit(geom::cross(along, outward)).unwrap_or([0., -1., 0.]);
        let k = geom::dominant_axis(view);
        if (k == 1 && view[k] > 0.) || (k != 1 && view[k] < 0.) {
            view = geom::scale(view, -1.);
        }
        self.draw(anchor, [along, outward, view], c, true, extent)
    }
    fn draw(
        &mut self,
        anchor: V3,
        basis: [V3; 3],
        c: &Callout,
        face: bool,
        feature_extent: Option<f64>,
    ) -> Placed {
        self.draw_columns(anchor, basis, c, (face, feature_extent), None)
    }
    fn draw_columns(
        &mut self,
        anchor: V3,
        basis: [V3; 3],
        c: &Callout,
        placement: (bool, Option<f64>),
        columns: Option<&[f64]>,
    ) -> Placed {
        let (face, feature_extent) = placement;
        let [along, outward, normal] = basis;
        let h = self.height;
        let scale = (h / 3.).min(0.32 * self.bounds.max_extent().max(h) / c.size[0].max(1.));
        let width = c.size[0] * scale;
        let height = c.size[1] * scale;
        let up = geom::cross(normal, along);
        let mut chosen = anchor;
        let mut chosen_box = Bounds::empty();
        let projected = !face || c.cells.len() > 1;
        let rectangle = |centre| {
            [(-1., -1.), (1., -1.), (1., 1.), (-1., 1.)]
                .map(|(x, y)| {
                    geom::add(
                        centre,
                        geom::add(
                            geom::scale(along, x * width / 2.),
                            geom::scale(up, y * height / 2.),
                        ),
                    )
                })
                .to_vec()
        };
        let columns: &[f64] = columns.unwrap_or(if face && c.cells.len() == 1 {
            &[0.]
        } else {
            &[0., -1., 1., -2., 2., -3., 3.]
        });
        let gap = feature_extent.map_or(0.6 * h, |e| (0.6 * h).min(0.12 * e))
            + 2.4 * f64::from(c.leader_rings > 0) * h;
        let mut candidates: Vec<_> = (0..8)
            .flat_map(|row| {
                columns.iter().map(move |&column| {
                    let a = column * (width / 2. + feature_extent.map_or(2. * h, |_| gap));
                    let b = gap + row as f64 * (height + h);
                    (row, column, a.hypot(b))
                })
            })
            .collect();
        candidates.sort_by(|a, b| a.2.total_cmp(&b.2));
        for (row, column, _) in candidates {
            let centre = geom::add(
                anchor,
                geom::add(
                    geom::scale(outward, gap + height / 2. + row as f64 * (height + h)),
                    geom::scale(
                        along,
                        column * (width / 2. + feature_extent.map_or(2. * h, |_| gap)),
                    ),
                ),
            );
            let mut b = Bounds::empty();
            for x in [-width / 2., width / 2.] {
                for y in [-height / 2., height / 2.] {
                    b.add(geom::add(
                        centre,
                        geom::add(geom::scale(along, x), geom::scale(up, y)),
                    ));
                }
            }
            chosen = centre;
            chosen_box = b;
            if !self.occupied.iter().any(|r| boxes_overlap(b, *r, 0.2 * h))
                && (!projected
                    || self.callouts.iter().all(|other| {
                        let other_normal = geom::cross(
                            geom::sub(other[1], other[0]),
                            geom::sub(other[3], other[0]),
                        );
                        super::linear::VIEWS.into_iter().all(|view| {
                            (geom::dot(normal, view).abs() < 1e-8
                                && geom::dot(other_normal, view).abs() < 1e-8)
                                || polygon_clearance(&rectangle(centre), other, view) >= 0.2 * h
                        })
                    }))
            {
                break;
            }
        }
        self.occupied.push(chosen_box);
        if projected {
            self.callouts.push(rectangle(chosen));
        }
        let local = c.polylines();
        let mut ink = Bounds::empty();
        for p in local.iter().flatten() {
            ink.add([p[0], p[1], 0.]);
        }
        let mid = if ink.max_extent().is_finite() {
            ink.centre()
        } else {
            [0.; 3]
        };
        let round = |p: V3| p.map(|x| (x * 1e6).round() / 1e6 + 0.);
        let at = |p: [f64; 2]| {
            round(geom::add(
                chosen,
                geom::add(
                    geom::scale(along, (p[0] - mid[0]) * scale),
                    geom::scale(up, (p[1] - mid[1]) * scale),
                ),
            ))
        };
        let mut lines: Vec<_> = local
            .iter()
            .map(|l| l.iter().map(|&p| at(p)).collect())
            .collect();
        let mut end = geom::sub(chosen, geom::scale(outward, height / 2.));
        if feature_extent.is_some() {
            // Attach to the nearest point on the frame edge, not its midpoint.
            let a = geom::dot(geom::sub(anchor, end), along).clamp(-width / 2., width / 2.);
            end = geom::add(end, geom::scale(along, a));
        }
        let elbow = geom::add(
            anchor,
            geom::scale(outward, geom::dot(geom::sub(end, anchor), outward)),
        );
        for ring in 0..c.leader_rings {
            let r = 0.7 * 2.7 * scale * if ring == 0 { 1. } else { 1.7 };
            let centre = geom::sub(elbow, geom::scale(outward, 2.4 * h));
            lines.push(
                (0..=32)
                    .map(|i| {
                        let t = (i as f64 * 360. / 32.).to_radians();
                        round(geom::add(
                            centre,
                            geom::add(
                                geom::scale(along, r * t.cos()),
                                geom::scale(up, r * t.sin()),
                            ),
                        ))
                    })
                    .collect(),
            );
        }
        if c.leader_rings > 0 {
            let ring_centre = geom::sub(elbow, geom::scale(outward, 2.4 * h));
            lines.push(vec![round(anchor), round(ring_centre), round(end)]);
        } else if geom::dist(elbow, end) > 1e-8 {
            lines.push(vec![round(anchor), round(elbow), round(end)]);
        } else {
            lines.push(vec![round(anchor), round(end)]);
        }
        Placed {
            anchor: round(anchor),
            origin: at([ink.min[0], ink.min[1]]),
            normal,
            along,
            lines,
        }
    }
}
/// Include coordinate axes for edge-on projections as well as polygon edge normals.
fn polygon_clearance(a: &[V3], b: &[V3], view: V3) -> f64 {
    let n = geom::unit(view).unwrap_or(view);
    [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]]
        .into_iter()
        .filter_map(|axis| geom::unit(geom::sub(axis, geom::scale(n, geom::dot(axis, n)))))
        .map(|axis| {
            let interval = |p: &[V3]| {
                p.iter()
                    .map(|p| geom::dot(*p, axis))
                    .fold([f64::INFINITY, f64::NEG_INFINITY], |r, t| {
                        [r[0].min(t), r[1].max(t)]
                    })
            };
            let x = interval(a);
            let y = interval(b);
            (x[0] - y[1]).max(y[0] - x[1])
        })
        .fold(super::linear::projected_clearance(a, b, view), f64::max)
}

/// Compare annotations in parallel planes by their projected boxes too.
pub(crate) fn boxes_overlap(a: Bounds, b: Bounds, padding: f64) -> bool {
    (0..3)
        .filter(|&k| (a.max[k] - a.min[k]).abs() > 1e-9 || (b.max[k] - b.min[k]).abs() > 1e-9)
        .all(|k| a.min[k] < b.max[k] + padding && a.max[k] + padding > b.min[k])
}

/// Separating-axis clearance of projected 3D bounds, including non-coplanar ink.
/// A projected box has edges along the projected world axes; their three normals
/// are sufficient for the separating-axis test. Unlike a 3D overlap, this detects
/// annotations which collide only after projection.
fn projected_bounds_clearance(a: Bounds, b: Bounds, view: V3) -> f64 {
    [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]]
        .into_iter()
        .filter_map(|axis| geom::unit(geom::cross(view, axis)))
        .map(|axis| {
            let interval = |bounds: Bounds| {
                let centre = geom::dot(bounds.centre(), axis);
                let radius = (0..3)
                    .map(|k| (bounds.max[k] - bounds.min[k]) * axis[k].abs() / 2.)
                    .sum::<f64>();
                [centre - radius, centre + radius]
            };
            let a = interval(a);
            let b = interval(b);
            (a[0] - b[1]).max(b[0] - a[1])
        })
        .fold(f64::NEG_INFINITY, f64::max)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notes_clear_complete_ink_in_all_views_without_changing_size_or_front_plane() {
        let mut layout = Layout::new(Bounds {
            min: [0., -4., -4.],
            max: [37.6, 4., 4.],
        });
        let obstacles = vec![
            vec![[37., -8., -5.], [43., 8., 10.]],
            vec![[-2., -30., 25.], [30., 12., 15.]],
        ];
        layout.reserve(&obstacles);
        let c = super::super::label("GENERAL NOTES\nMATERIAL: Steel C45");
        let first = layout.clone().notes(&c);
        let placed = layout.notes(&c);
        assert_eq!(first.lines, placed.lines, "deterministic placement");
        assert_eq!(placed.normal, [0., -1., 0.]);
        assert_eq!(placed.along, [1., 0., 0.]);
        for (actual, expected) in placed
            .lines
            .iter()
            .flatten()
            .zip(c.polylines().iter().flatten())
        {
            assert_eq!(actual[1], placed.origin[1]);
            assert!(
                (actual[0] - placed.origin[0] - expected[0] * layout.text_height() / 3.).abs()
                    < 1e-12
            );
            assert!(
                (actual[2] - placed.origin[2] - expected[1] * layout.text_height() / 3.).abs()
                    < 1e-12
            );
        }
        for view in super::super::linear::VIEWS {
            assert!(
                projected_bounds_clearance(
                    *layout.occupied.last().unwrap(),
                    layout.occupied[0],
                    view
                ) >= 2. * layout.text_height()
            );
        }
    }
}
