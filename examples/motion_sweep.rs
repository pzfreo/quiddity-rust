//! Read-only occurrence census for raw and inferred-frame routes under seeded motions.
//! Usage: motion_sweep STEPS_DIR [COUNT=0] [SEED=17] [raw|both=both]
// Internal diagnostic: use the exact engine implementation without exporting caller-space APIs.
#[path = "../src/engine.rs"]
mod engine;
pub use engine::*;
extern crate self as quiddity;
#[allow(dead_code)]
#[path = "../tests/support/motion_breakdown.rs"]
mod motion_breakdown;
use motion_breakdown::{Breakdown, Occurrence};
#[path = "../tests/support/motion_sweep.rs"]
mod motion_sweep;
use motion_sweep::{compose, occurrences, rotation};
use quiddity::kernel::step::{IDENTITY, Placement, read_step_file_placed};
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};
fn census(path: &std::path::Path, framed: bool, at: &Placement) -> Result<Vec<Occurrence>, String> {
    let part = read_step_file_placed(path, at).map_err(|e| e.to_string())?;
    let part = if framed {
        let frame = quiddity::frames::infer_part_frame(&part).map_err(|e| e.as_str().to_owned())?;
        part.transformed(&frame.normalisation())
            .map_err(|e| e.to_string())?
    } else {
        part
    };
    Ok(occurrences(&part))
}

fn record(o: &Occurrence) -> serde_json::Value {
    serde_json::json!({"family":o.family,"faces":o.faces})
}

fn measure(
    file: &str,
    route: &str,
    motion: &str,
    baseline: &[Occurrence],
    moved: &[Occurrence],
) -> serde_json::Value {
    let pairs = motion_breakdown::matching(baseline, moved);
    let mut used_before = vec![false; baseline.len()];
    let mut used_after = vec![false; moved.len()];
    for &(i, j) in &pairs {
        used_before[i] = true;
        used_after[j] = true;
    }
    let absent: Vec<_> = baseline
        .iter()
        .zip(used_before)
        .filter(|(_, used)| !used)
        .map(|(o, _)| record(o))
        .collect();
    let introduced: Vec<_> = moved
        .iter()
        .zip(used_after)
        .filter(|(_, used)| !used)
        .map(|(o, _)| record(o))
        .collect();
    let reclassified: Vec<_> = pairs
        .iter()
        .filter(|&&(i, j)| baseline[i].family != moved[j].family)
        .map(|&(i, j)| {
            serde_json::json!({
                "from":baseline[i].family,"to":moved[j].family,
                "before":baseline[i].faces,"after":moved[j].faces
            })
        })
        .collect();
    let mut denominators = std::collections::BTreeMap::<String, usize>::new();
    for o in baseline {
        *denominators.entry(o.family.clone()).or_default() += 1;
    }
    serde_json::json!({
        "file":file,"route":route,"motion":motion,"denominators":denominators,
        "counts":Breakdown::compare(baseline,moved),
        "absent":absent,"introduced":introduced,"reclassified":reclassified,
        "baseline_records":baseline.iter().map(record).collect::<Vec<_>>(),
        "moved_records":moved.iter().map(record).collect::<Vec<_>>()
    })
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert!(
        args.len() >= 2,
        "usage: motion_sweep STEPS_DIR [COUNT=0] [SEED=17] [raw|both=both]"
    );
    let count: usize = args.get(2).map_or(0, |s| s.parse().unwrap());
    let mut seed: u64 = args.get(3).map_or(17, |s| s.parse().unwrap());
    let mode = args.get(4).map_or("both", String::as_str);
    assert!(["raw", "both"].contains(&mode), "route must be raw or both");
    let mut paths: Vec<_> = std::fs::read_dir(&args[1])
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| p.extension().is_some_and(|s| s == "step"))
        .filter(|p| {
            count != 0
                || p.file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .starts_with("golden_")
        })
        .collect();
    paths.sort();
    if count != 0 {
        // Seeded sample without replacement, independent of directory enumeration order.
        assert!(paths.len() >= count, "fewer STEP files than requested");
        for i in (1..paths.len()).rev() {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            paths.swap(i, (seed % (i as u64 + 1)) as usize);
        }
        paths.truncate(count);
        paths.sort();
    }
    let motions = if count == 0 {
        vec![
            ("Z30", rotation([0., 0., 1.], 30., [0.; 3])),
            ("X30", rotation([1., 0., 0.], 30., [0.; 3])),
            (
                "Z17X23",
                compose(
                    rotation([1., 0., 0.], 23., [0.; 3]),
                    rotation([0., 0., 1.], 17., [0.; 3]),
                ),
            ),
        ]
    } else {
        // Two seeded axis-angle draws applied to every sampled part. The exact matrices are
        // printed so the experiment can be replayed independently of this generator.
        (0..2)
            .map(|i| {
                let mut next = || {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    (seed >> 11) as f64 / ((1u64 << 53) as f64)
                };
                let z = 2. * next() - 1.;
                let phi = std::f64::consts::TAU * next();
                let r = (1. - z * z).sqrt();
                (
                    if i == 0 { "random0" } else { "random1" },
                    rotation([r * phi.cos(), r * phi.sin(), z], 360. * next(), [0.; 3]),
                )
            })
            .collect()
    };
    eprintln!(
        "seed {} paths {} motions {:?}",
        args.get(3).map_or("17", String::as_str),
        paths.len(),
        motions
    );
    let next = AtomicUsize::new(0);
    let stdout = Mutex::new(std::io::stdout());
    std::thread::scope(|scope| {
        for _ in 0..6 {
            std::thread::Builder::new().stack_size(256<<20).spawn_scoped(scope,|| {
                loop {
                    let i=next.fetch_add(1,Ordering::Relaxed);
                    let Some(path)=paths.get(i) else {break};
                    let run=std::panic::catch_unwind(|| {
                        let mut rows=Vec::new();
                        let routes: &[&str]=if mode=="both" {&["raw","framed"]} else {&["raw"]};
                        for &route in routes {
                            let baseline=census(path,route=="framed",&IDENTITY)?;
                            for &(motion,at) in &motions {
                                let moved=census(path,route=="framed",&at)?;
                                rows.push(measure(path.file_name().unwrap().to_str().unwrap(),route,motion,&baseline,&moved));
                            }
                        }
                        Ok::<_,String>(rows)
                    });
                    use std::io::Write;
                    let mut out=stdout.lock().unwrap();
                    match run {
                        Ok(Ok(rows))=>for row in rows {writeln!(out,"{row}").unwrap();},
                        error=>writeln!(out,"{}",serde_json::json!({"file":path.display().to_string(),"error":format!("{error:?}")})).unwrap(),
                    }
                    out.flush().unwrap();
                }
            }).unwrap();
        }
    });
}
