//! draftwright's text against build123d's: the box, the outline both ways (the share of each
//! side's length within `TOL` of the other's) and the area the outlines bound.

#![allow(dead_code)]

use std::io::Read;
use std::path::Path;

use haecceity::presentation::text::Font;
use serde_json::Value;

pub const TOL: f64 = 1e-4;
const STEP: f64 = 0.005;

type Seg = ([f64; 2], [f64; 2]);

pub fn load(path: &Path) -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(path).unwrap())
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}

pub struct Scores {
    /// The largest difference in any coordinate of the bounding box.
    pub box_error: f64,
    pub outline: (f64, f64),
    /// build123d's faces' area, then the area draftwright's outline bounds (even-odd).
    pub area: (f64, f64),
    pub faces: usize,
    pub contours: usize,
}

pub fn compare(case: &Value) -> Scores {
    let fonts = Path::new(env!("CARGO_MANIFEST_DIR")).join("fonts");
    let font = Font::open(&fonts.join(case["font"].as_str().unwrap())).unwrap();
    let laid = font.text(
        case["text"].as_str().unwrap(),
        case["size"].as_f64().unwrap(),
    );
    let b: [[f64; 2]; 2] = serde_json::from_value(case["box"].clone()).unwrap();
    let box_error = [
        b[0][0] - laid.min[0],
        b[0][1] - laid.min[1],
        b[1][0] - laid.max[0],
        b[1][1] - laid.max[1],
    ]
    .iter()
    .fold(0.0f64, |m, d| m.max(d.abs()));
    let mut theirs: Vec<Seg> = Vec::new();
    let mut area = 0.0;
    for face in case["faces"].as_array().unwrap() {
        area += face["area"].as_f64().unwrap();
        for e in face["edges"].as_array().unwrap() {
            let pts: Vec<[f64; 2]> = serde_json::from_value(e.clone()).unwrap();
            theirs.extend(pts.windows(2).map(|w| (w[0], w[1])));
        }
    }
    // Each segment sampled finely enough that its chords stand within a hundredth of TOL.
    let mut mine: Vec<Seg> = Vec::new();
    for s in laid.contours.iter().flatten() {
        let n = 256;
        mine.extend((0..n).map(|i| (s.at(i as f64 / n as f64), s.at((i + 1) as f64 / n as f64))));
    }
    Scores {
        box_error,
        outline: (covered(&theirs, &mine), covered(&mine, &theirs)),
        area: (area, even_odd_area(&mine)),
        faces: case["faces"].as_array().unwrap().len(),
        contours: laid.contours.len(),
    }
}

fn covered(from: &[Seg], to: &[Seg]) -> f64 {
    let cell = 0.05;
    let key = |p: [f64; 2]| ((p[0] / cell).floor() as i64, (p[1] / cell).floor() as i64);
    let mut grid: std::collections::HashMap<(i64, i64), Vec<usize>> = Default::default();
    for (i, (a, b)) in to.iter().enumerate() {
        let (ka, kb) = (key(*a), key(*b));
        for x in ka.0.min(kb.0) - 1..=ka.0.max(kb.0) + 1 {
            for y in ka.1.min(kb.1) - 1..=ka.1.max(kb.1) + 1 {
                grid.entry((x, y)).or_default().push(i);
            }
        }
    }
    let near = |p: [f64; 2]| {
        grid.get(&key(p))
            .is_some_and(|ids| ids.iter().any(|&i| distance(p, to[i]) <= TOL))
    };
    let (mut total, mut hit) = (0.0, 0.0);
    for (a, b) in from {
        let len = (b[0] - a[0]).hypot(b[1] - a[1]);
        let n = (len / STEP).ceil().max(1.0) as usize;
        for k in 0..n {
            let t = (k as f64 + 0.5) / n as f64;
            total += len / n as f64;
            if near([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]) {
                hit += len / n as f64;
            }
        }
    }
    if total == 0.0 { 1.0 } else { hit / total }
}

fn distance(p: [f64; 2], (a, b): Seg) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2).clamp(0.0, 1.0)
    };
    (p[0] - a[0] - t * dx).hypot(p[1] - a[1] - t * dy)
}

/// The area the segments enclose by the even-odd rule, integrated over scanlines.
fn even_odd_area(segs: &[Seg]) -> f64 {
    let (lo, hi) = segs
        .iter()
        .flat_map(|s| [s.0[1], s.1[1]])
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), y| {
            (a.min(y), b.max(y))
        });
    if hi <= lo {
        return 0.0;
    }
    let n = 20000;
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
