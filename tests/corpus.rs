//! Parity over the shared STEP corpus: for every corpus file, the face inventory, the kernel
//! answers recognisers lean on (and solid masses) and each ported recogniser's answer must match
//! what `tools/export_corpus.py` recorded from Python, except for the differences listed (with
//! reasons) in `tests/fixtures/known_divergences.json`.
//!
//! Every difference is a problem with an exact identity: the file, a family (`read`,
//! `inventory`, `kernel`, or the recogniser's Python name) and a key, and a list entry names that
//! identity exactly with the number of problems it explains:
//!
//! ```json
//! {"file": "nist/nist_ctc_01_asme1_rd.stp", "family": "inventory", "key": "edge count",
//!  "count": 13, "verdict": "rust-correct", "reason": "..."}
//! ```
//!
//! Keys: `inventory` has `face count`, `face type`, `edge count` and `face bounds` (one problem
//! per face); `kernel` has `solid count`, `solid masses differ by up to 1e<n>` (one per file),
//! `uv_bounds <surface kind>` or, when an angular direction differs only by whole turns or by
//! where a full turn starts, `uv_bounds <surface kind> (same range, another turn)` (one per
//! face), and `arcs` (one per neighbour pair); a recogniser has
//! `<options> records: <n> vs <m> records, fields {...}` and `<options> evidence ...` (one per
//! run), where `<options>` is the run's options as compact JSON with sorted keys. The test fails
//! on a problem no entry names, on an entry whose count differs from the problems it names, and
//! on two entries with one identity (a problem must have one explanation). A failure prints each
//! unlisted problem group as the entry it needs (verdict and reason to be added), with the first
//! few details.

mod common;

use std::collections::BTreeMap;

use quiddity::kernel::geom::SurfaceType;
use quiddity::read_step_file;
use serde_json::Value;

/// One difference from Python: which file, which family (`read`, `inventory`, `kernel` or a
/// recogniser), its exact key, and what was seen.
struct Problem {
    file: String,
    family: String,
    key: String,
    detail: String,
}

impl Problem {
    fn new(file: &str, family: &str, key: impl Into<String>, detail: impl Into<String>) -> Self {
        Problem {
            file: file.to_owned(),
            family: family.to_owned(),
            key: key.into(),
            detail: detail.into(),
        }
    }

    fn id(&self) -> (&str, &str, &str) {
        (&self.file, &self.family, &self.key)
    }
}

#[test]
fn corpus_matches_python() {
    let Some(dir) = common::corpus_dir() else {
        assert!(
            std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
            "QUIDDITY_CORPUS_REQUIRED is set but the corpus was not found"
        );
        eprintln!("corpus not found; set QUIDDITY_CORPUS");
        return;
    };
    let mut problems = Vec::new();
    let mut counts = BTreeMap::<String, (usize, usize)>::new();
    let files = common::load("corpus.json")["files"]
        .as_array()
        .unwrap()
        .clone();
    let (mut kernel_runs, mut per_face_runs) = (0, 0);
    for entry in &files {
        let name = entry["file"].as_str().unwrap();
        let part = match read_step_file(&dir.join(name)) {
            Ok(p) => p,
            Err(e) => {
                problems.push(Problem::new(name, "read", "read failed", e.to_string()));
                continue;
            }
        };
        let (aligned, found) =
            common::inventory_problems(&part, entry["inventory"].as_array().unwrap());
        for (key, detail) in found {
            problems.push(Problem::new(name, "inventory", key, detail));
        }
        // Face-indexed kernel answers compare only where the faces align (same count and
        // types); edge counts and boxes may differ (seams OpenCascade's healing adds).
        check_kernel(name, &part, &entry["kernel"], aligned, &mut problems);
        kernel_runs += 1;
        per_face_runs += usize::from(aligned);
        for (function, runs) in entry["results"].as_object().unwrap() {
            for run in runs.as_array().unwrap() {
                let tally = counts.entry(function.clone()).or_default();
                let got = common::recognise(function, &part, &run["options"]);
                if !common::same(&got, &run["result"]) {
                    tally.1 += 1;
                    problems.push(Problem::new(
                        name,
                        function,
                        format!(
                            "{} records: {}",
                            run["options"],
                            common::diff_summary(&got, &run["result"])
                        ),
                        common::diff(&got, &run["result"]),
                    ));
                    continue;
                }
                tally.0 += 1;
                if run.get("defining").is_some() || run.get("evidence_error").is_some() {
                    check_evidence(name, function, &part, run, &mut problems);
                }
            }
        }
    }
    for (function, (ok, bad)) in &counts {
        eprintln!("{function}: {ok} runs match, {bad} differ");
    }
    eprintln!(
        "kernel checks ran on {kernel_runs} of {} files (per-face answers on {per_face_runs})",
        files.len()
    );
    let known: Vec<Value> = serde_json::from_value(common::load("known_divergences.json")).unwrap();
    common::check_verdicts("known_divergences.json", &known);
    let id = |d: &Value| -> (String, String, String, u64) {
        let field = |k: &str| {
            d[k].as_str()
                .unwrap_or_else(|| panic!("known_divergences.json: entry without {k}: {d}"))
                .to_owned()
        };
        let count = d["count"]
            .as_u64()
            .unwrap_or_else(|| panic!("known_divergences.json: entry without count: {d}"));
        (field("file"), field("family"), field("key"), count)
    };
    let known: Vec<_> = known.iter().map(id).collect();
    let mut seen = std::collections::BTreeSet::new();
    for (file, family, key, _) in &known {
        assert!(
            seen.insert((file, family, key)),
            "known_divergences.json: two entries for {file} {family} {key:?}"
        );
    }
    let mut found = BTreeMap::<(&str, &str, &str), Vec<&Problem>>::new();
    for p in &problems {
        found.entry(p.id()).or_default().push(p);
    }
    let listed = |id: (&str, &str, &str)| {
        known
            .iter()
            .any(|(f, fam, k, _)| (f.as_str(), fam.as_str(), k.as_str()) == id)
    };
    let unexpected: Vec<String> = found
        .iter()
        .filter(|(id, _)| !listed(**id))
        .map(|((file, family, key), ps)| {
            let entry = serde_json::json!({
                "file": file, "family": family, "key": key, "count": ps.len()
            });
            let details: Vec<&str> = ps.iter().take(5).map(|p| p.detail.as_str()).collect();
            format!("{entry}\n    {}", details.join("\n    "))
        })
        .collect();
    let miscounted: Vec<String> = known
        .iter()
        .filter_map(|(file, family, key, count)| {
            let n = found
                .get(&(file.as_str(), family.as_str(), key.as_str()))
                .map_or(0, Vec::len);
            (n as u64 != *count)
                .then(|| format!("{file} {family} {key:?}: {count} listed, {n} found"))
        })
        .collect();
    assert!(
        unexpected.is_empty() && miscounted.is_empty(),
        "{} unexpected problems (as entries, with the first details):\n{}\nknown divergences \
         whose count changed: {miscounted:#?}",
        unexpected.len(),
        unexpected.join("\n")
    );
}

/// The defining faces (or the refusal) of the evidence path agree with Python's.
fn check_evidence(
    name: &str,
    function: &str,
    part: &quiddity::Part,
    run: &Value,
    problems: &mut Vec<Problem>,
) {
    let ours = common::defining(function, part, &run["options"]);
    let opts = &run["options"];
    let mut problem = |what: &str, detail: String| {
        problems.push(Problem::new(
            name,
            function,
            format!("{opts} evidence {what}"),
            detail,
        ));
    };
    match (ours, run.get("evidence_error"), run.get("defining")) {
        (Err(_), Some(_), _) => {}
        (Ok(faces), None, Some(want)) => {
            let want: Vec<Vec<usize>> = serde_json::from_value(want.clone()).unwrap();
            if faces != want {
                problem("faces differ", format!("{faces:?}, Python {want:?}"));
            }
        }
        (Ok(faces), Some(error), _) => {
            problem(
                "refused by Python only",
                format!("{faces:?}, Python {error}"),
            );
        }
        (got, want, _) => problem("differs", format!("{got:?}, Python {want:?}")),
    }
}

/// Whether two UV ranges `[u0, u1, v0, v1]` cover the same parameters: equal in each
/// non-periodic direction and, in an angular one, the same interval a whole number of turns
/// apart or both a full turn (started at different seams).
fn same_turns(kind: SurfaceType, got: [f64; 4], want: [f64; 4]) -> bool {
    let turn = std::f64::consts::TAU;
    let close = |a: f64, b: f64| (a - b).abs() <= 1e-6 * a.abs().max(b.abs()).max(1.0);
    let periodic = match kind {
        SurfaceType::Cylinder | SurfaceType::Cone | SurfaceType::Sphere => [true, false],
        SurfaceType::Torus => [true, true],
        _ => [false, false],
    };
    (0..2).all(|d| {
        let (g0, g1, w0, w1) = (got[2 * d], got[2 * d + 1], want[2 * d], want[2 * d + 1]);
        if !periodic[d] {
            return close(g0, w0) && close(g1, w1);
        }
        let shift = ((g0 - w0) / turn).round() * turn;
        (close(g0 - shift, w0) && close(g1 - shift, w1))
            || (close(g1 - g0, turn) && close(w1 - w0, turn))
    })
}

/// Each solid's volume and area and, when the faces align with OpenCascade's, each face's UV
/// range (`BRepTools::UVBounds`) and the arc between each pair of neighbours agree with
/// OpenCascade's: one problem per face or pair.
///
/// Masses agree to 1e-9 except where OpenCascade integrates approximations (B-spline pcurves)
/// or the file's boundary does not close; the problem names the worst relative difference as a
/// power of ten, so a known divergence pins how far apart the answers may be.
fn check_kernel(
    name: &str,
    part: &quiddity::Part,
    kernel: &Value,
    aligned: bool,
    problems: &mut Vec<Problem>,
) {
    let want = kernel["solids"].as_array().unwrap();
    if want.len() != part.solids.len() {
        problems.push(Problem::new(
            name,
            "kernel",
            "solid count",
            format!(
                "{} solids, OpenCascade has {}",
                part.solids.len(),
                want.len()
            ),
        ));
    } else {
        let mut worst: f64 = 0.0;
        let mut detail = Vec::new();
        for (s, w) in want.iter().enumerate() {
            let (v, a) = (w[0].as_f64().unwrap(), w[1].as_f64().unwrap());
            let (gv, ga) = part.solid_mass(s).unwrap_or((f64::NAN, f64::NAN));
            let off = ((gv - v).abs() / v.abs()).max((ga - a).abs() / a.abs());
            worst = worst.max(if off.is_nan() { f64::INFINITY } else { off });
            detail.push(format!("solid {s} {gv} {ga} vs {v} {a}"));
        }
        if worst > 1e-9 {
            problems.push(Problem::new(
                name,
                "kernel",
                format!("solid masses differ by up to 1e{}", worst.log10().ceil()),
                detail.join("; "),
            ));
        }
    }
    if !aligned {
        return;
    }
    for (face, want) in kernel["uv_bounds"].as_array().unwrap().iter().enumerate() {
        let got = part
            .uv_bounds(face)
            .map(|(u0, u1, v0, v1)| [u0, u1, v0, v1]);
        let got_json = serde_json::to_value(got).unwrap();
        if !common::same(&got_json, want) {
            let kind = part.faces[face].surface.kind();
            let want: Option<[f64; 4]> = serde_json::from_value(want.clone()).unwrap();
            let key = match (got, want) {
                (Some(g), Some(w)) if same_turns(kind, g, w) => {
                    format!("uv_bounds {kind:?} (same range, another turn)")
                }
                _ => format!("uv_bounds {kind:?}"),
            };
            problems.push(Problem::new(
                name,
                "kernel",
                key,
                format!("face {face} ({kind:?}) {got_json} vs {want:?}"),
            ));
        }
    }
    for pair in kernel["arcs"].as_array().unwrap() {
        let (a, b) = (
            pair[0].as_u64().unwrap() as usize,
            pair[1].as_u64().unwrap() as usize,
        );
        let got = part.arc(a, b).map(|arc| format!("{arc:?}").to_lowercase());
        if got.as_deref() != pair[2].as_str() {
            problems.push(Problem::new(
                name,
                "kernel",
                "arcs",
                format!("faces {a}-{b} {got:?} vs {}", pair[2]),
            ));
        }
    }
}
