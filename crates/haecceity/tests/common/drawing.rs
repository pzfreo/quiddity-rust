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
            Some(plane) => project_section(part, &view, plane).unwrap(),
            None => project(part, &view).unwrap(),
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
        .unwrap()
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

    /// The distance from *p* to the nearest segment, and which, when one is within `CELL`
    /// (else `CELL` and none).
    pub fn nearest(&self, p: [f64; 2]) -> (f64, Option<usize>) {
        let mut best = (CELL, None);
        for &i in self.grid.get(&key(p)).into_iter().flatten() {
            let d = distance(p, self.segs[i]);
            if d < best.0 {
                best = (d, Some(i));
            }
        }
        best
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

/// One stretch of one side's drawing that the other side does not draw alike, as the scores
/// count it: a visible line with no visible line of the other side within `TOL`; a hidden line
/// as inked (no visible line of its own side over it) with no hidden line of the other side
/// within `TOL`, or a visible one. Distances are capped at `CELL`.
#[derive(Clone, Debug)]
pub struct Run {
    /// OpenCascade's line, else haecceity's.
    pub theirs: bool,
    pub visible: bool,
    pub length: f64,
    pub from: [f64; 2],
    pub to: [f64; 2],
    /// The largest distance from the other side's line of the same visibility.
    pub stray: f64,
    /// The largest distance from the part's true geometry: its drawn faces' edges, projected
    /// from their samples (chords within 0.2 µm of the curve).
    pub off_edges: f64,
    /// The length within `TOL` of the other side's line of the other visibility.
    pub flipped: f64,
    /// For a hidden run, the largest distance from its own side's visible line (how far that
    /// misses covering it).
    pub own_visible: f64,
    /// The edges nearest its points.
    pub edges: Vec<usize>,
}

/// Where a view's two drawings differ, against the part's true geometry.
#[derive(Debug, Default)]
pub struct Differences {
    pub runs: Vec<Run>,
    /// The largest distance of the runs' edges (their samples and the curve halfway between)
    /// from each of their faces' surfaces.
    pub edges_from_faces: f64,
}

/// How much of a view's differences is OpenCascade approximating curves that haecceity draws
/// on the part's edges (see [`Differences::explained`]).
#[derive(Debug, Default)]
pub struct Explained {
    /// The largest distance between the two sides over the runs so explained.
    pub stray: f64,
    /// The furthest OpenCascade's approximating runs lie from every projected edge.
    pub off_edges: f64,
    /// The length of the runs not so explained.
    pub other: f64,
}

impl Differences {
    /// Each run read as one of: OpenCascade's line off every projected edge (by more than
    /// `TOL` less the samples' chords), where haecceity's line of the same visibility lies
    /// `stray` away (within `CELL`), and mostly not where haecceity draws the other visibility;
    /// OpenCascade's hidden line on an edge, under haecceity's visible line there, left inked
    /// because OpenCascade's own visible line misses it (by `own_visible`); haecceity's line on
    /// a projected edge (within 1e-6), visible, or mostly not where OpenCascade draws a visible
    /// line, or where OpenCascade draws it hidden too (within `TOL`: it differs only in what is
    /// drawn over it), with OpenCascade's line of its visibility `stray` away.
    pub fn explained(&self) -> Explained {
        let mut out = Explained::default();
        for r in &self.runs {
            let mostly_alike = r.flipped < 0.5 * r.length;
            let stray = if r.theirs && r.off_edges > 0.8 * TOL && r.stray < CELL && mostly_alike {
                out.off_edges = out.off_edges.max(r.off_edges);
                r.stray
            } else if r.theirs && !r.visible && r.flipped >= r.length * (1.0 - 1e-9) {
                r.own_visible
            } else if !r.theirs
                && r.off_edges <= 1e-6
                && (r.visible || mostly_alike || r.stray <= TOL)
            {
                r.stray
            } else {
                out.other += r.length;
                continue;
            };
            out.stray = out.stray.max(stray);
        }
        out
    }
}

/// [`Differences`] of one view of *part* drawn both ways (`compare_views`' of *record*).
pub fn differences(record: &Value, part: &Part, compared: &Compared) -> Differences {
    let v = if record["views"].is_object() {
        &record["views"][compared.name.as_str()]
    } else {
        &record["view"]
    };
    let vec3 = |k: &str| -> [f64; 3] { serde_json::from_value(v[k].clone()).unwrap() };
    let view = View::new(vec3("direction"), vec3("up")).unwrap();
    // Every edge of the drawn faces, projected into OpenCascade's coordinates.
    let mut faces: Vec<usize> = part.solids.iter().flat_map(|s| s.faces.clone()).collect();
    if faces.is_empty() {
        faces = (0..part.faces.len()).collect();
    }
    let mut edge_ids: Vec<usize> = faces.iter().flat_map(|&f| part.face_edges(f)).collect();
    edge_ids.sort_unstable();
    edge_ids.dedup();
    let (mut truth, mut owner): (Vec<Seg>, Vec<usize>) = (Vec::new(), Vec::new());
    for &e in &edge_ids {
        let pts: Vec<[f64; 2]> = part.edges[e]
            .samples
            .iter()
            .map(|p| {
                let q = view.map(*p);
                [q[0] + compared.offset[0], q[1] + compared.offset[1]]
            })
            .collect();
        for w in pts.windows(2) {
            truth.push((w[0], w[1]));
            owner.push(e);
        }
    }
    let truth = Index::new(&truth);
    let theirs = [
        Index::new(&compared.theirs[0]),
        Index::new(&compared.theirs[1]),
    ];
    let mine = [Index::new(&compared.mine[0]), Index::new(&compared.mine[1])];
    // The distance to the other side's line of the same visibility, where a point is unmatched.
    let unmatched = |own: &[Index; 2], other: &[Index; 2], vis: usize, p: [f64; 2]| {
        if vis == 1 {
            let d = other[1].nearest(p).0;
            return (d > TOL).then_some(d);
        }
        if own[1].nearest(p).0 <= TOL {
            return None;
        }
        let d = other[0].nearest(p).0;
        (d > TOL || other[1].nearest(p).0 <= TOL).then_some(d)
    };
    let mut runs: Vec<Run> = Vec::new();
    for (side, lines) in [(true, &compared.theirs), (false, &compared.mine)] {
        let (own, other) = if side {
            (&theirs, &mine)
        } else {
            (&mine, &theirs)
        };
        for vis in 0..2 {
            // A run goes on while its points are unmatched along one polyline.
            let mut open = false;
            let mut last: Option<[f64; 2]> = None;
            for (a, b) in &lines[vis] {
                if last != Some(*a) {
                    open = false;
                }
                last = Some(*b);
                let len = (b[0] - a[0]).hypot(b[1] - a[1]);
                let n = (len / STEP).ceil().max(1.0) as usize;
                for k in 0..n {
                    let t = (k as f64 + 0.5) / n as f64;
                    let p = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
                    let Some(stray) = unmatched(own, other, vis, p) else {
                        open = false;
                        continue;
                    };
                    if !open {
                        runs.push(Run {
                            theirs: side,
                            visible: vis == 1,
                            length: 0.0,
                            from: p,
                            to: p,
                            stray: 0.0,
                            off_edges: 0.0,
                            flipped: 0.0,
                            own_visible: 0.0,
                            edges: Vec::new(),
                        });
                        open = true;
                    }
                    let run = runs.last_mut().unwrap();
                    run.length += len / n as f64;
                    run.to = p;
                    run.stray = run.stray.max(stray);
                    let (to_edge, seg) = truth.nearest(p);
                    run.off_edges = run.off_edges.max(to_edge);
                    if let Some(seg) = seg
                        && !run.edges.contains(&owner[seg])
                    {
                        run.edges.push(owner[seg]);
                    }
                    if other[1 - vis].nearest(p).0 <= TOL {
                        run.flipped += len / n as f64;
                    }
                    if vis == 0 {
                        run.own_visible = run.own_visible.max(own[1].nearest(p).0);
                    }
                }
            }
        }
    }
    let mut edges: Vec<usize> = runs.iter().flat_map(|r| r.edges.iter().copied()).collect();
    edges.sort_unstable();
    edges.dedup();
    let mut edges_from_faces: f64 = 0.0;
    for e in edges {
        let edge = &part.edges[e];
        let s = &edge.samples;
        let halves = s.windows(2).map(|w| {
            let mid = [
                0.5 * (w[0][0] + w[1][0]),
                0.5 * (w[0][1] + w[1][1]),
                0.5 * (w[0][2] + w[1][2]),
            ];
            edge.curve.value(edge.curve.parameter(mid))
        });
        let along: Vec<[f64; 3]> = s.iter().copied().chain(halves).collect();
        for &f in &part.edge_faces()[e] {
            let surface = &part.faces[f].surface;
            for q in &along {
                let gap = surface
                    .parameters(*q, None)
                    .map_or(f64::INFINITY, |(u, v)| {
                        let r = surface.value(u, v);
                        ((r[0] - q[0]).powi(2) + (r[1] - q[1]).powi(2) + (r[2] - q[2]).powi(2))
                            .sqrt()
                    });
                edges_from_faces = edges_from_faces.max(gap);
            }
        }
    }
    Differences {
        runs,
        edges_from_faces,
    }
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

/// The material's area in the record's cutting plane by another route than its traced outline:
/// the part classified (ray parity, `haecceity::classify`) at the centres of an `n` by `n` grid
/// of cells across the part's box in the plane. Returns the area of the cells found inside, and
/// of those the outline (*cut*, in world (x, z)) passes through or that the classifier cannot
/// call, within which the true area's difference from the first must lie.
pub fn classified_area(part: &Part, record: &Value, cut: &[Seg], n: usize) -> (f64, f64) {
    use haecceity::classify::{Classifier, State};
    let y = record["cut_y"].as_f64().unwrap();
    let b = part.bounds();
    let (x0, z0) = (b.min[0], b.min[2]);
    let (hx, hz) = ((b.max[0] - x0) / n as f64, (b.max[2] - z0) / n as f64);
    let cell = |x: f64, z: f64| {
        (
            ((x - x0) / hx).floor().clamp(0.0, (n - 1) as f64) as usize,
            ((z - z0) / hz).floor().clamp(0.0, (n - 1) as f64) as usize,
        )
    };
    let mut crossed = vec![false; n * n];
    for (a, c) in cut {
        let steps = ((c[0] - a[0]).abs() / hx)
            .max((c[1] - a[1]).abs() / hz)
            .mul_add(2.0, 1.0)
            .ceil() as usize;
        for k in 0..=steps {
            let t = k as f64 / steps as f64;
            let (i, j) = cell(a[0] + (c[0] - a[0]) * t, a[1] + (c[1] - a[1]) * t);
            crossed[i * n + j] = true;
        }
    }
    let faces: Vec<usize> = part.solids.iter().flat_map(|s| s.faces.clone()).collect();
    let classifier = Classifier::for_faces(part, faces);
    let (mut inside, mut doubt) = (0usize, 0usize);
    for i in 0..n {
        for j in 0..n {
            let p = [x0 + hx * (i as f64 + 0.5), y, z0 + hz * (j as f64 + 0.5)];
            let state = classifier.classify(p);
            inside += usize::from(state == State::In);
            doubt += usize::from(crossed[i * n + j] || matches!(state, State::On | State::Unknown));
        }
    }
    (inside as f64 * hx * hz, doubt as f64 * hx * hz)
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
