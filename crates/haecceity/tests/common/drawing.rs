//! Drawings compared with OpenCascade's (`tools/capture_hlr.py`, `tools/capture_section.py`):
//! for each view, the share of one side's visible (hidden) curve length lying within `TOL` of
//! the other's visible (hidden) curves, each way; for a section, its cut outline alike and the
//! area it bounds.

#![allow(dead_code)]

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use haecceity::Part;
use haecceity::hlr::{Plane, Projected, View, project, project_section, section};
use serde_json::Value;

pub const TOL: f64 = 1e-3;
const STEP: f64 = 0.05;

pub type Seg = ([f64; 2], [f64; 2]);

/// A view's lines: hidden, then visible.
pub type Lines = [Vec<Seg>; 2];

pub fn load_gz(path: &Path) -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(path).unwrap())
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}

/// One view drawn both ways, in OpenCascade's coordinates.
pub struct Compared {
    pub name: String,
    pub theirs: Lines,
    pub mine: Lines,
    pub ours: Vec<Projected>,
    /// Added to haecceity's view coordinates to give OpenCascade's.
    pub offset: [f64; 2],
    pub seconds: f64,
}

pub struct Scores {
    pub visible: (f64, f64),
    pub hidden: (f64, f64),
    /// Hidden lines as inked: only where no visible line is drawn over them.
    pub ink: (f64, f64),
}

impl Compared {
    /// Each pair is OpenCascade's length found in haecceity's, then the reverse.
    pub fn scores(&self) -> Scores {
        let both = |a: &[Seg], b: &[Seg]| (covered(a, b), covered(b, a));
        Scores {
            visible: both(&self.theirs[1], &self.mine[1]),
            hidden: both(&self.theirs[0], &self.mine[0]),
            ink: (
                ink_covered(&self.theirs, &self.mine),
                ink_covered(&self.mine, &self.theirs),
            ),
        }
    }
}

/// The record's cutting plane, for a section.
pub fn plane(record: &Value) -> Option<Plane> {
    record["cut_y"]
        .as_f64()
        .and_then(|y| Plane::new([0.0, y, 0.0], [0.0, 1.0, 0.0]))
}

/// The record's views drawn by haecceity: a section record's one view, of the part cut.
pub fn compare_views(record: &Value, part: &Part) -> Vec<Compared> {
    let plane = plane(record);
    let views: Vec<(String, &Value)> = match plane {
        Some(_) => vec![("section".to_string(), &record["view"])],
        None => record["views"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v))
            .collect(),
    };
    let mut out = Vec::new();
    for (name, v) in views {
        let vec3 = |k: &str| -> [f64; 3] { serde_json::from_value(v[k].clone()).unwrap() };
        let view = View::new(vec3("direction"), vec3("up")).unwrap();
        // The axes must agree with OpenCascade's before any edge can.
        let frame: Vec<[f64; 2]> = serde_json::from_value(v["frame"].clone()).unwrap();
        let centre = vec3("centre");
        let c = view.map(centre);
        for k in 0..3 {
            let mut p = centre;
            p[k] += 1.0;
            let m = view.map(p);
            let (dx, dy) = (
                m[0] - c[0] + frame[0][0] - frame[k + 1][0],
                m[1] - c[1] + frame[0][1] - frame[k + 1][1],
            );
            assert!(
                dx.abs() < 1e-9 && dy.abs() < 1e-9,
                "{name}: view axes differ"
            );
        }
        let started = std::time::Instant::now();
        let ours = match &plane {
            Some(plane) => project_section(part, &view, plane),
            None => project(part, &view),
        };
        let seconds = started.elapsed().as_secs_f64();
        let mut theirs: Lines = [Vec::new(), Vec::new()];
        for e in v["edges"].as_array().unwrap() {
            theirs[e["visible"].as_bool().unwrap() as usize].extend(segments(&e["points"]));
        }
        // OpenCascade's hidden-line output is measured from the camera, `Project`'s from the
        // origin.
        let camera = view.map(vec3("camera"));
        let offset = [-camera[0], -camera[1]];
        let mut mine: Lines = [Vec::new(), Vec::new()];
        for p in &ours {
            let pts: Vec<[f64; 2]> = p
                .points
                .iter()
                .map(|q| [q[0] + offset[0], q[1] + offset[1]])
                .collect();
            mine[p.visible as usize].extend(pts.windows(2).map(|w| (w[0], w[1])));
        }
        out.push(Compared {
            name,
            theirs,
            mine,
            ours,
            offset,
            seconds,
        });
    }
    out
}

/// A section's cut outline both ways, in world (x, z): OpenCascade's cut faces' edges, and
/// haecceity's section.
pub fn compare_cut(record: &Value, part: &Part) -> Option<(Vec<Seg>, Vec<Seg>)> {
    let plane = plane(record)?;
    let mine: Vec<Seg> = section(part, &plane)
        .iter()
        .flat_map(|c| {
            c.windows(2)
                .map(|w| ([w[0][0], w[0][2]], [w[1][0], w[1][2]]))
        })
        .collect();
    let theirs: Vec<Seg> = record["cut_faces"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|face| face.as_array().unwrap().iter().flat_map(segments))
        .collect();
    Some((theirs, mine))
}

/// A recorded polyline's segments.
pub fn segments(points: &Value) -> Vec<Seg> {
    let pts: Vec<[f64; 2]> = serde_json::from_value(points.clone()).unwrap();
    pts.windows(2).map(|w| (w[0], w[1])).collect()
}

/// Segments indexed by a grid, for "is anything within TOL" queries.
pub struct Index<'a> {
    segs: &'a [Seg],
    grid: HashMap<(i64, i64), Vec<usize>>,
}

const CELL: f64 = 0.5;

fn key(p: [f64; 2]) -> (i64, i64) {
    ((p[0] / CELL).floor() as i64, (p[1] / CELL).floor() as i64)
}

impl<'a> Index<'a> {
    pub fn new(segs: &'a [Seg]) -> Self {
        let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for (i, (a, b)) in segs.iter().enumerate() {
            let (ka, kb) = (key(*a), key(*b));
            for x in ka.0.min(kb.0) - 1..=ka.0.max(kb.0) + 1 {
                for y in ka.1.min(kb.1) - 1..=ka.1.max(kb.1) + 1 {
                    grid.entry((x, y)).or_default().push(i);
                }
            }
        }
        Index { segs, grid }
    }

    fn near(&self, p: [f64; 2]) -> bool {
        self.grid
            .get(&key(p))
            .is_some_and(|ids| ids.iter().any(|&i| distance(p, self.segs[i]) <= TOL))
    }

    /// The share of *from*'s length lying within TOL of these segments.
    pub fn covers(&self, from: &[Seg]) -> f64 {
        share(from, |p| Some(self.near(p)))
    }
}

/// The share of *from*'s length (sampled every STEP) where *matched* holds, of the length where
/// it gives an answer.
fn share(from: &[Seg], matched: impl Fn([f64; 2]) -> Option<bool>) -> f64 {
    let (mut total, mut near) = (0.0, 0.0);
    for (a, b) in from {
        let len = (b[0] - a[0]).hypot(b[1] - a[1]);
        let n = (len / STEP).ceil().max(1.0) as usize;
        for k in 0..n {
            let t = (k as f64 + 0.5) / n as f64;
            if let Some(m) = matched([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]) {
                total += len / n as f64;
                if m {
                    near += len / n as f64;
                }
            }
        }
    }
    if total == 0.0 { 1.0 } else { near / total }
}

/// The share of *from*'s length lying within TOL of *to*.
pub fn covered(from: &[Seg], to: &[Seg]) -> f64 {
    Index::new(to).covers(from)
}

/// The share of *from*'s hidden lines as inked (where none of its visible lines is drawn over
/// them) that *to* inks alike: hidden there, and not drawn visible.
pub fn ink_covered(from: &Lines, to: &Lines) -> f64 {
    let (own, hidden, visible) = (Index::new(&from[1]), Index::new(&to[0]), Index::new(&to[1]));
    share(&from[0], |p| {
        (!own.near(p)).then(|| hidden.near(p) && !visible.near(p))
    })
}

pub fn distance(p: [f64; 2], (a, b): Seg) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2).clamp(0.0, 1.0)
    };
    (p[0] - a[0] - t * dx).hypot(p[1] - a[1] - t * dy)
}

/// The area the segments enclose by the even-odd rule (as draftwright hatches), integrated over
/// scanlines.
pub fn even_odd_area(segs: &[Seg]) -> f64 {
    let (lo, hi) = segs
        .iter()
        .flat_map(|s| [s.0[1], s.1[1]])
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), y| {
            (a.min(y), b.max(y))
        });
    if hi <= lo {
        return 0.0;
    }
    let n = 4000;
    let h = (hi - lo) / n as f64;
    let mut area = 0.0;
    for i in 0..n {
        let y = lo + h * (i as f64 + 0.5);
        let mut xs: Vec<f64> = segs
            .iter()
            .filter(|(a, b)| (a[1] > y) != (b[1] > y))
            .map(|(a, b)| a[0] + (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]))
            .collect();
        xs.sort_by(f64::total_cmp);
        area += xs
            .as_chunks::<2>()
            .0
            .iter()
            .map(|[a, b]| b - a)
            .sum::<f64>()
            * h;
    }
    area
}
