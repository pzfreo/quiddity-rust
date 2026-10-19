//! External-data accuracy ratchet. Run with QUIDDITY_MFINSTSEG=/path/to/MFInstSeg.
//! No dataset files are redistributed: permission to redistribute was not established.
//! `tools/update_mfinstseg_baseline.py` records improvements without allowing regressions.
#[path = "../examples/support/mfinstseg.rs"]
mod mfinstseg;

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Deserialize)]
struct Model {
    id: String,
    step_sha256: String,
    labels_sha256: String,
}
#[derive(Deserialize)]
struct Baseline {
    schema: usize,
    mapping_sha256: String,
    models: Vec<Model>,
    scores: Vec<mfinstseg::Score>,
}
fn hash(path: &Path) -> String {
    Sha256::digest(std::fs::read(path).unwrap())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Exact rational comparison, with the scorer's zero-denominator convention.
fn no_drop(got: &mfinstseg::Counts, baseline: &mfinstseg::Counts) -> bool {
    fn ratio_ge(n: u64, d: u64, bn: u64, bd: u64) -> bool {
        let (n, d) = if d == 0 { (0, 1) } else { (n, d) };
        let (bn, bd) = if bd == 0 { (0, 1) } else { (bn, bd) };
        u128::from(n) * u128::from(bd) >= u128::from(bn) * u128::from(d)
    }
    ratio_ge(
        got.true_positive,
        got.predicted,
        baseline.true_positive,
        baseline.predicted,
    ) && ratio_ge(
        got.true_positive,
        got.labelled,
        baseline.true_positive,
        baseline.labelled,
    )
}

#[test]
fn rational_ratchet_detects_precision_and_recall_regressions() {
    use mfinstseg::Counts;
    let base = Counts {
        true_positive: 3,
        predicted: 4,
        labelled: 5,
    };
    assert!(!no_drop(
        &Counts {
            true_positive: 3,
            predicted: 5,
            labelled: 5
        },
        &base
    ));
    assert!(!no_drop(
        &Counts {
            true_positive: 2,
            predicted: 2,
            labelled: 5
        },
        &base
    ));
    assert!(no_drop(
        &Counts {
            true_positive: 4,
            predicted: 5,
            labelled: 5
        },
        &base
    ));
    assert!(!no_drop(&Counts::default(), &base));
    assert!(no_drop(&base, &Counts::default()));
}

#[test]
fn independent_label_accuracy_ratchet() {
    let Some(root) = std::env::var_os("QUIDDITY_MFINSTSEG") else {
        eprintln!("external MFInstSeg ratchet skipped: set QUIDDITY_MFINSTSEG to enable");
        return;
    };
    let root = Path::new(&root);
    let baseline: Baseline =
        serde_json::from_str(include_str!("fixtures/mfinstseg_baseline.json")).unwrap();
    assert_eq!(baseline.schema, 1);
    assert_eq!(baseline.models.len(), 200);
    assert_eq!(
        hash(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mfinstseg_mapping.json")),
        baseline.mapping_sha256,
        "mapping changed: review evidence and rebaseline explicitly"
    );
    let classes = mfinstseg::mapping();
    assert_eq!(baseline.scores.len(), classes.len());
    let next = AtomicUsize::new(0);
    let results = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..6 {
            std::thread::Builder::new()
                .stack_size(256 << 20)
                .spawn_scoped(scope, || {
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(model) = baseline.models.get(i) else {
                            break;
                        };
                        assert_eq!(
                            hash(&root.join("steps").join(format!("{}.step", model.id))),
                            model.step_sha256,
                            "{} STEP changed",
                            model.id
                        );
                        assert_eq!(
                            hash(&root.join("labels").join(format!("{}.json", model.id))),
                            model.labels_sha256,
                            "{} labels changed",
                            model.id
                        );
                        results
                            .lock()
                            .unwrap()
                            .push(mfinstseg::evaluate(root, &model.id, false, &classes).unwrap());
                    }
                })
                .unwrap();
        }
    });
    let mut scores = vec![mfinstseg::Score::default(); classes.len()];
    for model in results.into_inner().unwrap() {
        assert!(
            model.instances_available,
            "{}: incomplete instance labels",
            model.id
        );
        assert!(
            model.label_warnings.is_empty(),
            "{}: {:?}",
            model.id,
            model.label_warnings
        );
        assert!(model.error.is_none(), "{}: {:?}", model.id, model.error);
        assert!(
            model.section_recess_refusal.is_none(),
            "{}: {:?}",
            model.id,
            model.section_recess_refusal
        );
        for (total, s) in scores.iter_mut().zip(&model.scores) {
            total.add(s);
        }
    }
    for ((c, got), base) in classes.iter().zip(&scores).zip(&baseline.scores) {
        assert_eq!(got.faces.labelled, base.faces.labelled);
        assert!(
            no_drop(&got.faces, &base.faces),
            "{} faces regressed: {:?} -> {:?}",
            c.name,
            base.faces,
            got.faces
        );
        let (got_i, base_i) = (
            got.instances.as_ref().unwrap(),
            base.instances.as_ref().unwrap(),
        );
        assert_eq!(got_i.labelled, base_i.labelled);
        assert!(
            no_drop(got_i, base_i),
            "{} instances regressed: {:?} -> {:?}",
            c.name,
            base_i,
            got_i
        );
        eprintln!(
            "{}: face P/R {:.6}/{:.6}, instance P/R {:.6}/{:.6}",
            c.name,
            got.faces.precision(),
            got.faces.recall(),
            got_i.precision(),
            got_i.recall()
        );
        if got != base {
            eprintln!(
                "{} counts changed without lowering a score; record improvements with tools/update_mfinstseg_baseline.py",
                c.name
            );
        }
    }
}
