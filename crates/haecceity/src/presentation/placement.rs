//! Short, feature-local model annotation placement; no sheet layout or leader routing.
use super::Callout;
use crate::geom::Bounds;

#[derive(Clone, Debug)]
pub struct Placed {
    pub anchor: [f64; 3],
    pub origin: [f64; 3],
    pub normal: [f64; 3],
    pub along: [f64; 3],
    pub lines: Vec<Vec<[f64; 3]>>,
}
/// A small deterministic stagger beside the closest side of the part's narrow extent.
/// Text is at most 0.8% of the largest extent high; label width is capped at 24%.
/// Each callout starts again at its feature, never at the preceding label's right edge.
pub struct Layout {
    bounds: Bounds,
    largest: f64,
    axes: [usize; 3],
    occupied: Vec<(usize, usize, [f64; 4])>,
}
impl Layout {
    pub fn new(bounds: Bounds) -> Self {
        let mut axes = [0, 1, 2];
        axes.sort_by(|&a, &b| {
            (bounds.max[b] - bounds.min[b])
                .total_cmp(&(bounds.max[a] - bounds.min[a]))
                .then(a.cmp(&b))
        });
        let largest = bounds.max_extent();
        let largest = if largest.is_finite() && largest > 0. {
            largest
        } else {
            1.
        };
        Self {
            bounds,
            largest,
            axes,
            occupied: Vec::new(),
        }
    }
    pub fn text_height(&self) -> f64 {
        self.largest / 125.
    }
    pub fn place(&mut self, anchor: [f64; 3], axis: Option<[f64; 3]>, c: &Callout) -> Placed {
        let u = axis.map_or(self.axes[0], crate::geom::dominant_axis);
        let v = *self
            .axes
            .iter()
            .rev()
            .find(|&&k| k != u)
            .unwrap_or(&self.axes[1]);
        let w = 3 - u - v;
        let h = self.text_height();
        let scale = (h / 3.).min(0.24 * self.largest / c.size[0].max(1.));
        let width = c.size[0] * scale;
        let height = c.size[1] * scale;
        let sign = if anchor[v] >= (self.bounds.min[v] + self.bounds.max[v]) / 2. {
            1.
        } else {
            -1.
        };
        let boundary = if sign > 0. {
            self.bounds.max[v]
        } else {
            self.bounds.min[v]
        };
        let mut chosen = [0.; 4];
        'candidate: for row in 0..10 {
            for column in [0., 1., -1., 2., -2.] {
                let left = anchor[u] - width / 2. + column * 0.08 * self.largest;
                let near = boundary + sign * (0.025 * self.largest + row as f64 * 3. * h);
                let bottom = if sign > 0. { near } else { near - height };
                let rect = [left, bottom, left + width, bottom + height];
                chosen = rect;
                let overlaps = self.occupied.iter().any(|&(a, b, r)| {
                    a == u
                        && b == v
                        && rect[0] < r[2] + h
                        && rect[2] + h > r[0]
                        && rect[1] < r[3] + h
                        && rect[3] + h > r[1]
                });
                if !overlaps {
                    break 'candidate;
                }
            }
        }
        self.occupied.push((u, v, chosen));
        let local = c.polylines();
        // Datum frames are centred, while dimensions/control frames start at their corner.
        let mut min = [f64::INFINITY; 2];
        for p in local.iter().flatten() {
            for k in 0..2 {
                min[k] = min[k].min(p[k]);
            }
        }
        for x in &mut min {
            if !x.is_finite() {
                *x = 0.;
            }
        }
        let round = |p: [f64; 3]| p.map(|x| (x * 1e6).round() / 1e6 + 0.0);
        let at = |p: [f64; 2]| {
            let mut xyz = anchor;
            xyz[u] = chosen[0] + (p[0] - min[0]) * scale;
            xyz[v] = chosen[1] + (p[1] - min[1]) * scale;
            round(xyz)
        };
        let origin = at(min);
        let mut normal = [0.; 3];
        normal[w] = if (u + 1) % 3 == v { 1. } else { -1. };
        let mut along = [0.; 3];
        along[u] = 1.;
        let mut lines: Vec<_> = local
            .iter()
            .map(|line| line.iter().map(|&p| at(p)).collect())
            .collect();
        let mut end = origin;
        end[u] += width / 2.;
        if sign < 0. {
            end[v] += height;
        }
        // Draftwright's scope rings stand at the leader bend, clear of the frame.
        // Add a short normal shelf only for scope symbols; retain all other leaders.
        let mut elbow = end;
        if c.leader_rings > 0 {
            elbow[v] -= sign * 2.4 * h;
        }
        for ring in 0..c.leader_rings {
            let r = 0.7 * 2.7 * scale * if ring == 0 { 1. } else { 1.7 };
            lines.push(
                (0..=32)
                    .map(|i| {
                        let t = (i as f64 * 360. / 32.).to_radians();
                        let mut p = elbow;
                        p[u] += r * t.cos();
                        p[v] += r * t.sin();
                        round(p)
                    })
                    .collect(),
            );
        }
        lines.push(if c.leader_rings > 0 {
            vec![round(anchor), round(elbow), round(end)]
        } else {
            vec![round(anchor), round(end)]
        });
        Placed {
            anchor: round(anchor),
            origin,
            normal,
            along,
            lines,
        }
    }
}
