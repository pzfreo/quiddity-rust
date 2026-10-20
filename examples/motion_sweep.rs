//! Read-only occurrence census for raw and inferred-frame routes under seeded motions.
//! Usage: motion_sweep STEPS_DIR [COUNT=0] [SEED=17] [raw|both=both]
#[allow(dead_code)]
#[path = "../tests/support/motion_breakdown.rs"]
mod motion_breakdown;
use motion_breakdown::{Breakdown, Occurrence};
use quiddity::kernel::step::{IDENTITY, Placement, read_step_file_placed};
use quiddity::{Part, correspondence, features, frames};
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};
fn rotation([x, y, z]: [f64; 3], angle: f64) -> Placement {
    let (s, c) = angle.to_radians().sin_cos();
    let t = 1. - c;
    [
        [t * x * x + c, t * x * y - s * z, t * x * z + s * y, 0.],
        [t * x * y + s * z, t * y * y + c, t * y * z - s * x, 0.],
        [t * x * z - s * y, t * y * z + s * x, t * z * z + c, 0.],
    ]
}
fn compose(a: Placement, b: Placement) -> Placement {
    [0, 1, 2].map(|i| {
        [0, 1, 2, 3].map(|j| {
            (0..3).map(|k| a[i][k] * b[k][j]).sum::<f64>() + if j == 3 { a[i][3] } else { 0. }
        })
    })
}
const SWEEP_FAMILIES: [(&str, &str); 28] = [
    ("hole", "holes"),
    ("hole_pattern", "hole_patterns"),
    ("boss", "bosses"),
    ("step", "turned_steps"),
    ("groove", "grooves"),
    ("flat", "flats"),
    ("slot", "slots"),
    ("oriented_slot", "oriented_slots"),
    ("rectangular_blind_slot", "rectangular_blind_slots"),
    ("round_bottom_blind_slot", "round_bottom_blind_slots"),
    ("channel", "channels"),
    ("pocket", "pockets"),
    ("prismatic_pocket", "prismatic_pockets"),
    ("edge_open_circular_pocket", "edge_open_circular_pockets"),
    ("edge_open_prismatic_recess", "edge_open_prismatic_recesses"),
    ("passage", "passages"),
    ("chamfer", "chamfers"),
    ("oriented_chamfer", "oriented_chamfers"),
    ("angled_step", "angled_steps"),
    ("gusset_rib", "gusset_ribs"),
    ("paired_ramp_step", "paired_ramp_steps"),
    ("through_step", "through_steps"),
    ("oblique_through_step", "oblique_through_steps"),
    ("circular_blind_step", "circular_blind_steps"),
    ("blend", "blends"),
    ("fillet", "fillets"),
    ("countersink", "countersinks"),
    ("plate", "plates"),
];

fn occurrences(part: &Part) -> Vec<Occurrence> {
    use features::reconcile::ReasonCode;
    let inventory = features::inventory(part).unwrap();
    let compatible: Vec<_> = inventory
        .dispositions
        .iter()
        .filter(|d| d.reason == ReasonCode::TurnedStepGrooveCompatible)
        .map(|d| {
            let mut f = d.candidate.defining.clone();
            f.sort_unstable();
            f
        })
        .collect();
    let accepted = inventory.accepted();
    let prints = correspondence::fingerprint(part, &accepted);
    SWEEP_FAMILIES
        .iter()
        .flat_map(|&(family, field)| {
            let compatible = &compatible;
            prints
                .features
                .iter()
                .filter(move |f| f.family == field)
                .filter(move |f| field != "turned_steps" || !compatible.contains(&f.faces))
                .map(move |f| Occurrence {
                    family: family.into(),
                    faces: f.faces.clone(),
                })
        })
        .collect()
}

fn census(path: &std::path::Path, framed: bool, at: &Placement) -> Result<Vec<Occurrence>, String> {
    if framed {
        frames::prepare_framed_file(path, at)
            .map(|p| occurrences(&p.part))
            .map_err(|e| e.to_string())
    } else {
        read_step_file_placed(path, at)
            .map(|p| occurrences(&p))
            .map_err(|e| e.to_string())
    }
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
            ("Z30", rotation([0., 0., 1.], 30.)),
            ("X30", rotation([1., 0., 0.], 30.)),
            (
                "Z17X23",
                compose(rotation([1., 0., 0.], 23.), rotation([0., 0., 1.], 17.)),
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
                    rotation([r * phi.cos(), r * phi.sin(), z], 360. * next()),
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
