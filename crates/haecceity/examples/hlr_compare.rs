//! Compare haecceity's hidden-line projection with OpenCascade's (`tools/capture_hlr.py`), or
//! its section views (`tools/capture_section.py`), as `tests/drawings.rs` does but printing
//! every score: `cargo run --release -p haecceity --example hlr_compare -- hlr.json.gz
//! [corpus-dir] [file]`.
//!
//! `HLR_MISS=<view>` lists OpenCascade's curves haecceity does not draw alike (`cut` for a
//! section's outline), `HLR_EXTRA=<view>` haecceity's that OpenCascade does not, and
//! `HLR_INK=<view>` OpenCascade's hidden lines as inked that haecceity does not ink.

#[path = "../tests/common/drawing.rs"]
mod drawing;

use std::path::Path;

use drawing::{
    Seg, compare_cut, compare_views, covered, distance, even_odd_area, ink_covered, load_gz,
    segments,
};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let all = load_gz(Path::new(&args[1]));
    let corpus = args
        .get(2)
        .map_or("../quiddity/tests/corpus".to_string(), |s| s.clone());
    let only = args.get(3);
    let length = |segs: &[Seg]| -> f64 {
        segs.iter()
            .map(|(a, b)| (b[0] - a[0]).hypot(b[1] - a[1]))
            .sum()
    };
    let wanted = |var: &str, name: &str| std::env::var(var).is_ok_and(|m| m == name);
    // How far *segs* stray from *to*: the largest distance from one of their points to it.
    let stray = |segs: &[Seg], to: &[Seg]| -> f64 {
        segs.iter()
            .flat_map(|s| [s.0, s.1])
            .map(|p| {
                to.iter()
                    .map(|t| distance(p, *t))
                    .fold(f64::INFINITY, f64::min)
            })
            .fold(0.0, f64::max)
    };
    for record in all["parts"].as_array().unwrap() {
        let file = record["file"].as_str().unwrap();
        if only.is_some_and(|o| !file.contains(o.as_str())) {
            continue;
        }
        let Ok(part) = haecceity::read_step_file(&Path::new(&corpus).join(file)) else {
            println!("{file}: unreadable");
            continue;
        };
        if let Some((theirs, mine)) = compare_cut(record, &part) {
            if wanted("HLR_MISS", "cut") {
                for (f, face) in record["cut_faces"].as_array().unwrap().iter().enumerate() {
                    for edge in face.as_array().unwrap() {
                        let segs = segments(edge);
                        let share = covered(&segs, &mine);
                        if share < 0.99 {
                            println!(
                                "  cut miss face {f} covered {share:.2} from {:?} to {:?}",
                                segs[0].0,
                                segs[segs.len() - 1].1
                            );
                        }
                    }
                }
            }
            let (area, occ_area) = (even_odd_area(&mine), even_odd_area(&theirs));
            println!(
                "{file} cut: outline occ→rust {:.4} rust→occ {:.4}; area occ {occ_area:.4} rust {area:.4} ({:+.2e}; occ faces {:.4})",
                covered(&theirs, &mine),
                covered(&mine, &theirs),
                (area - occ_area) / occ_area.max(1e-12),
                record["cut_area"].as_f64().unwrap()
            );
        }
        for c in compare_views(record, &part) {
            let name = c.name.as_str();
            if wanted("HLR_MISS", name) {
                let v = if record["views"].is_object() {
                    &record["views"][name]
                } else {
                    &record["view"]
                };
                for e in v["edges"].as_array().unwrap() {
                    let segs = segments(&e["points"]);
                    let vis = e["visible"].as_bool().unwrap() as usize;
                    let share = covered(&segs, &c.mine[vis]);
                    if share < 0.99 && !segs.is_empty() {
                        let other = covered(&segs, &c.mine[1 - vis]);
                        println!(
                            "  miss {} vis={} len={:.3} covered {share:.2} (other visibility {other:.2}) strays {:.4} from {:?} to {:?}",
                            e["kind"],
                            vis == 1,
                            length(&segs),
                            stray(&segs, &c.mine[vis]),
                            segs[0].0,
                            segs[segs.len() - 1].1
                        );
                    }
                }
            }
            if wanted("HLR_INK", name) {
                // OpenCascade's hidden lines as inked that haecceity does not ink alike.
                let v = if record["views"].is_object() {
                    &record["views"][name]
                } else {
                    &record["view"]
                };
                for e in v["edges"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|e| !e["visible"].as_bool().unwrap())
                {
                    let segs = segments(&e["points"]);
                    let share = ink_covered(&[segs.clone(), c.theirs[1].clone()], &c.mine);
                    if share < 0.99 && !segs.is_empty() {
                        println!(
                            "  ink miss {} len={:.3} matched {share:.2} strays {:.4} from hidden, {:.4} from visible; from {:?} to {:?}",
                            e["kind"],
                            length(&segs),
                            stray(&segs, &c.mine[0]),
                            stray(&segs, &c.mine[1]),
                            segs[0].0,
                            segs[segs.len() - 1].1
                        );
                    }
                }
            }
            if wanted("HLR_EXTRA", name) {
                for p in &c.ours {
                    let pts: Vec<[f64; 2]> = p
                        .points
                        .iter()
                        .map(|q| [q[0] + c.offset[0], q[1] + c.offset[1]])
                        .collect();
                    let segs: Vec<Seg> = pts.windows(2).map(|w| (w[0], w[1])).collect();
                    let share = covered(&segs, &c.theirs[p.visible as usize]);
                    if share < 0.99 {
                        println!(
                            "  extra {:?} vis={} len={:.3} covered {share:.2} from {:?} to {:?}",
                            p.class,
                            p.visible,
                            length(&segs),
                            pts[0],
                            pts[pts.len() - 1]
                        );
                    }
                }
            }
            let s = c.scores();
            println!(
                "{file} {name} ({:.2}s): visible occ→rust {:.4} rust→occ {:.4}; hidden occ→rust {:.4} rust→occ {:.4}; hidden ink occ→rust {:.4} rust→occ {:.4}",
                c.seconds, s.visible.0, s.visible.1, s.hidden.0, s.hidden.1, s.ink.0, s.ink.1
            );
        }
    }
}
