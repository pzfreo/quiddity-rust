//! Rust retry and publication snapshots over the legacy degradation questions, with unchanged placement invariance checks.

use super::common;

use std::path::PathBuf;

use quiddity::Part;
use quiddity::features::evidence::EvidenceError;
use quiddity::features::{Context, Occurrence, countersinks, holes, levels, plates, pockets};
use quiddity::kernel::step::{Placement, read_step_file, read_step_file_placed};
use serde_json::{Value, json};

const CAPTURE: &str = "captured/local_degradation/capture.json";

fn corpus() -> Option<PathBuf> {
    let corpus = common::corpus_dir();
    if corpus.is_none() {
        assert!(
            std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
            "QUIDDITY_CORPUS_REQUIRED is set but the corpus was not found"
        );
    }
    corpus
}

fn parts(capture: &Value) -> &Vec<Value> {
    capture["parts"].as_array().unwrap()
}

/// The capture is of this corpus, at the revision `corpus.json` records.
#[test]
fn capture_covers_the_corpus() {
    let capture = common::load(CAPTURE);
    let corpus = common::load("corpus.json");
    assert_eq!(capture["quiddity_revision"], corpus["quiddity_revision"]);
    let want: Vec<(&Value, &Value)> = corpus["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| (&f["file"], &f["sha256"]))
        .collect();
    let got: Vec<(&Value, &Value)> = parts(&capture)
        .iter()
        .map(|p| (&p["file"], &p["sha256"]))
        .collect();
    assert_eq!(got, want);
}

/// The strict and locally degraded hole evidence paths over one part, countersinks composed.
fn hole_paths(part: &Part) -> (Result<usize, EvidenceError>, Vec<Vec<usize>>) {
    let ctx = Context::new(part);
    let seats = countersinks::discover(&ctx);
    let strict = holes::discover_verified(&ctx, &seats).map(|found| found.len());
    let degraded = holes::discover_locally_degraded(&ctx, &seats)
        .unwrap_or_else(|e| panic!("locally degraded holes refused: {e}"))
        .into_iter()
        .map(|o| {
            let mut faces = o.defining;
            faces.sort_unstable();
            faces
        })
        .collect();
    (strict, degraded)
}

/// The locally degraded pocket evidence path over one part: each pocket's defining faces.
fn degraded_pockets(part: &Part) -> Result<Vec<Vec<usize>>, EvidenceError> {
    pockets::discover_locally_degraded(&Context::new(part)).map(|found| {
        found
            .into_iter()
            .map(|o| {
                let mut faces = o.defining;
                faces.sort_unstable();
                faces
            })
            .collect()
    })
}

/// Whether the port reaches the retry on a part: its strict hole evidence refused for want of a
/// valid solid, and its solids not all valid (`BRepCheck_Analyzer(solid).IsValid()`, so faulted
/// faces count).
fn port_retries(part: &Part) -> bool {
    let all_valid = !part.solids.is_empty()
        && (0..part.solids.len()).all(|s| part.solid_is_geometrically_valid(s));
    matches!(hole_paths(part).0, Err(EvidenceError::NoValidSolid)) && !all_valid
}

#[test]
fn rust_snapshot_retry_reachability() {
    let Some(dir) = corpus() else { return };
    let inputs = common::load(CAPTURE);
    let actual = common::parallel::map(parts(&inputs), |p| {
        let file = p["file"].as_str().unwrap();
        let part = read_step_file(&dir.join(file)).unwrap();
        json!({"file":file,"retried":port_retries(&part)})
    });
    common::snapshot::check(
        &common::fixtures(),
        "retry",
        &serde_json::to_value(actual).unwrap(),
    );
}

/// The port's face with this unmoved Rust face's box (to the inventory's 5e-3), if exactly one.
fn port_face(part: &Part, unmoved: &Value) -> Option<usize> {
    let close = |got: [f64; 3], want: &Value| {
        (0..3).all(|k| (got[k] - want[k].as_f64().unwrap()).abs() <= 5e-3 + 1e-6 * got[k].abs())
    };
    let matches: Vec<usize> = (0..part.faces.len())
        .filter(|&f| {
            let b = part.face_bounds(f);
            close(b.min, &unmoved["min"]) && close(b.max, &unmoved["max"])
        })
        .collect();
    (matches.len() == 1).then(|| matches[0])
}

/// The unmoved Rust entry point whose evidence path a skipping family function is part of (for step
/// levels and risers, the aggregate's own discovery).
fn entry_point(module: &str, function: &str) -> &'static str {
    match (module, function) {
        ("holes", "_discover_holes") => "recognise_holes",
        ("fillets", "_discover_fillets") => "recognise_fillets",
        ("_recess_features", "_discover_pockets") => "recognise_pockets",
        ("plates", "_discover_plates") => "recognise_plates",
        ("levels", "_discover_step_levels") => "_discover_step_levels",
        ("levels", "_discover_risers") => "_discover_risers",
        other => panic!("{other:?}: no evidence path mapped for this skip"),
    }
}

/// The locally degraded plate evidence path over one part: each plate's defining faces.
fn degraded_plates(part: &Part) -> Result<Vec<Vec<usize>>, EvidenceError> {
    plates::discover_locally_degraded(&Context::new(part), &Default::default()).map(|found| {
        found
            .into_iter()
            .map(|o| {
                let mut faces = o.defining;
                faces.sort_unstable();
                faces
            })
            .collect()
    })
}

/// The aggregate's step levels (or risers) that its proof under local degradation keeps
/// (`levels::proved`): each one's defining faces.
fn degraded_levels(part: &Part, risers: bool) -> Result<Vec<Vec<usize>>, String> {
    fn faces<R>(found: Vec<Occurrence<R>>) -> Vec<Vec<usize>> {
        found
            .into_iter()
            .map(|o| {
                let mut faces = o.defining;
                faces.sort_unstable();
                faces
            })
            .collect()
    }
    let ctx = Context::new(part);
    let step_levels = levels::discover_step_levels(&ctx);
    let found = if risers {
        let found = levels::discover_risers(&ctx, &step_levels);
        levels::proved(part, found, true).map(faces)
    } else {
        levels::proved(part, step_levels, true).map(faces)
    };
    found.map_err(|e| e.to_string())
}

/// What the port's evidence path does with the record whose proof faces are *faces*.
fn port_outcome(part: &Part, function: &str, faces: &[usize]) -> &'static str {
    let defining = match function {
        "recognise_holes" => Ok(hole_paths(part).1),
        "recognise_pockets" => degraded_pockets(part).map_err(|e| e.to_string()),
        "_discover_step_levels" => degraded_levels(part, false),
        "_discover_risers" => degraded_levels(part, true),
        "recognise_plates" => degraded_plates(part).map_err(|e| e.to_string()),
        f => common::defining(f, part, &json!({})),
    };
    match defining {
        Err(_) => "refused",
        Ok(found) => {
            // A record's own faces are among those its proof asked about (a plate's proof asks
            // face by face, so there the asked face is one of the record's).
            let on = |d: &Vec<usize>| {
                !d.is_empty()
                    && (d.iter().all(|f| faces.contains(f)) || faces.iter().all(|f| d.contains(f)))
            };
            if found.iter().any(on) {
                "published"
            } else {
                "skipped"
            }
        }
    }
}

#[test]
fn rust_snapshot_degradation_questions() {
    let Some(dir) = corpus() else { return };
    let inputs = common::load(CAPTURE);
    let selected: Vec<&Value> = parts(&inputs)
        .iter()
        .filter(|p| p["retry_input"] == true)
        .collect();
    let actual = common::parallel::map(&selected, |p| {
        let file = p["file"].as_str().unwrap();
        let part = read_step_file(&dir.join(file)).unwrap();
        let answers: Vec<Value> = p["skips"].as_array().unwrap().iter().map(|skip| {
            let faces: Option<Vec<usize>> = skip["boxes"].as_array().unwrap().iter().map(|b|port_face(&part,b)).collect();
            let rust = faces.map_or("faces not found", |faces|port_outcome(&part,entry_point(skip["module"].as_str().unwrap(),skip["function"].as_str().unwrap()),&faces));
            json!({"module":skip["module"],"function":skip["function"],"faces":skip["faces"],"outcome":rust})
        }).collect();
        json!({"file":file,"answers":answers})
    });
    common::snapshot::check(&common::fixtures(), "degradation_questions", &json!(actual));
}

/// On every part both sides retry, `features::recognise` takes the locally degraded run as
/// unmoved Rust's aggregate does: no record unmoved Rust's retry skips is published, and the section-recess
/// projection, which strictly refuses there for want of a valid solid, publishes unmoved Rust's
/// section recesses and refusals.
#[test]
fn rust_snapshot_retry_publication() {
    let Some(dir) = corpus() else { return };
    let inputs = common::load(CAPTURE);
    let selected: Vec<&Value> = parts(&inputs)
        .iter()
        .filter(|p| p["retry_input"] == true)
        .collect();
    let actual = common::parallel::map(&selected, |p| {
        let file = p["file"].as_str().unwrap();
        let part = read_step_file(&dir.join(file)).unwrap();
        let features = quiddity::features::recognise(&part);
        json!({"file":file,"retry":port_retries(&part),"features":features,"defining":features.defining,
               "section_recesses":features.section_recesses,"section_recess_refusals":features.section_recess_refusals,
               "refusal":features.section_recess_refusal.map(|e|e.to_string())})
    });
    assert!(!actual.is_empty());
    common::snapshot::check(&common::fixtures(), "retry_publication", &json!(actual));
}

/// Two of `tests/invariance.rs`'s rigid motions: one moving every axis and the origin, and one
/// reversing two axes.
const MOTIONS: [(&str, Placement); 2] = [
    (
        "rot_zx_moved",
        [
            [0.0, 0.0, 1.0, 123.456],
            [1.0, 0.0, 0.0, -78.9],
            [0.0, 1.0, 0.0, 41.3],
        ],
    ),
    (
        "rot_y180",
        [
            [-1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, -1.0, 0.0],
        ],
    ),
];

/// On every part unmoved Rust retries, the locally degraded holes and pockets (and whether the port
/// retries) are the same faces however the part is placed.
#[test]
fn locally_degraded_paths_are_placement_independent() {
    let Some(dir) = corpus() else { return };
    let capture = common::load(CAPTURE);
    for p in parts(&capture).iter().filter(|p| p["retry_input"] == true) {
        let path = dir.join(p["file"].as_str().unwrap());
        let base = read_step_file(&path).unwrap();
        let want = (
            port_retries(&base),
            hole_paths(&base).1,
            degraded_pockets(&base),
        );
        for (motion, placement) in &MOTIONS {
            let moved = read_step_file_placed(&path, placement).unwrap();
            let got = (
                port_retries(&moved),
                hole_paths(&moved).1,
                degraded_pockets(&moved),
            );
            assert_eq!(got, want, "{} {motion}", p["file"]);
        }
    }
}
