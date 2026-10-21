//! OpenCascade corpus references and Rust-owned recognition snapshots. The inventory, mass, UV and arc bands remain unchanged.

use super::common;

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
    /// The face (`"12"`) or neighbour pair (`"3-7"`) of a per-face kernel problem.
    face: Option<String>,
    detail: String,
}

impl Problem {
    fn new(file: &str, family: &str, key: impl Into<String>, detail: impl Into<String>) -> Self {
        Problem {
            file: file.to_owned(),
            family: family.to_owned(),
            key: key.into(),
            face: None,
            detail: detail.into(),
        }
    }

    fn on(mut self, face: String) -> Self {
        self.face = Some(face);
        self
    }

    fn id(&self) -> (&str, &str, &str) {
        (&self.file, &self.family, &self.key)
    }
}

/// A known divergence: file, family, key, the faces it is limited to, and its count.
type Entry = (String, String, String, Option<Vec<String>>, u64);

#[test]
fn corpus_matches_opencascade() {
    let Some(dir) = common::corpus_dir() else {
        assert!(
            std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
            "QUIDDITY_CORPUS_REQUIRED is set but the corpus was not found"
        );
        eprintln!("corpus not found; set QUIDDITY_CORPUS");
        return;
    };
    let files = common::load("corpus.json")["files"]
        .as_array()
        .unwrap()
        .clone();
    // Each file on its own, spread over every core; the results come back in corpus order, so
    // the problems are listed as one file after another would list them.
    let found = common::parallel::map(&files, |entry| {
        let mut problems = Vec::new();
        let counts = BTreeMap::<String, (usize, usize)>::new();
        let name = entry["file"].as_str().unwrap();
        let part = match read_step_file(&dir.join(name)) {
            Ok(p) => p,
            Err(e) => {
                problems.push(Problem::new(name, "read", "read failed", e.to_string()));
                return (problems, counts, None);
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
        let masses = entry["kernel"]["solids"].as_array().unwrap().len() == part.solids.len();
        (problems, counts, Some((masses, aligned)))
    });
    let mut problems = Vec::new();
    let mut counts = BTreeMap::<String, (usize, usize)>::new();
    let (mut kernel_runs, mut mass_runs, mut per_face_runs) = (0, 0, 0);
    for (found, tallies, ran) in found {
        problems.extend(found);
        for (function, (ok, bad)) in tallies {
            let tally = counts.entry(function).or_default();
            tally.0 += ok;
            tally.1 += bad;
        }
        if let Some((masses, aligned)) = ran {
            kernel_runs += 1;
            mass_runs += usize::from(masses);
            per_face_runs += usize::from(aligned);
        }
    }
    for (function, (ok, bad)) in &counts {
        eprintln!("{function}: {ok} runs match, {bad} differ");
    }
    eprintln!(
        "kernel checks ran on {kernel_runs} of {} files (masses compared on {mass_runs}, \
         per-face answers on {per_face_runs})",
        files.len()
    );
    let known: Vec<Value> =
        serde_json::from_value(common::load("references/occt_corpus.json")).unwrap();
    common::check_verdicts("references/occt_corpus.json", &known);
    let entry = |d: &Value| -> Entry {
        let field = |k: &str| {
            d[k].as_str()
                .unwrap_or_else(|| panic!("references/occt_corpus.json: entry without {k}: {d}"))
                .to_owned()
        };
        let count = d["count"]
            .as_u64()
            .unwrap_or_else(|| panic!("references/occt_corpus.json: entry without count: {d}"));
        let faces = d.get("faces").map(|faces| {
            faces
                .as_array()
                .unwrap_or_else(|| panic!("references/occt_corpus.json: `faces` not a list: {d}"))
                .iter()
                .map(|f| match f {
                    Value::String(pair) => pair.clone(),
                    face => face.to_string(),
                })
                .collect::<Vec<_>>()
        });
        (field("file"), field("family"), field("key"), faces, count)
    };
    let known: Vec<Entry> = known.iter().map(entry).collect();
    let mut seen = std::collections::BTreeSet::new();
    for (file, family, key, faces, _) in &known {
        let subjects = match faces {
            Some(faces) => faces.iter().map(Some).collect(),
            None => vec![None],
        };
        for face in subjects {
            assert!(
                seen.insert((file, family, key, face)),
                "references/occt_corpus.json: two entries for {file} {family} {key:?} {face:?}"
            );
        }
    }
    // The entry that names a problem: the one listing its face, else the one without faces.
    let named = |p: &Problem| -> Option<usize> {
        let same = |e: &Entry| (e.0.as_str(), e.1.as_str(), e.2.as_str()) == p.id();
        let listed = known.iter().position(|e| {
            same(e)
                && e.3
                    .as_ref()
                    .is_some_and(|faces| p.face.as_ref().is_some_and(|f| faces.contains(f)))
        });
        listed.or_else(|| known.iter().position(|e| same(e) && e.3.is_none()))
    };
    let mut matched = vec![0u64; known.len()];
    let mut unlisted = BTreeMap::<(&str, &str, &str), Vec<&Problem>>::new();
    for p in &problems {
        match named(p) {
            Some(i) => matched[i] += 1,
            None => unlisted.entry(p.id()).or_default().push(p),
        }
    }
    let unexpected: Vec<String> = unlisted
        .iter()
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
        .zip(&matched)
        .filter(|((.., count), n)| *n != count)
        .map(|((file, family, key, faces, count), n)| {
            format!("{file} {family} {key:?} {faces:?}: {count} listed, {n} found")
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
            problems.push(
                Problem::new(
                    name,
                    "kernel",
                    key,
                    format!("face {face} ({kind:?}) {got_json} vs {want:?}"),
                )
                .on(face.to_string()),
            );
        }
    }
    for pair in kernel["arcs"].as_array().unwrap() {
        let (a, b) = (
            pair[0].as_u64().unwrap() as usize,
            pair[1].as_u64().unwrap() as usize,
        );
        let got = part.arc(a, b).map(|arc| format!("{arc:?}").to_lowercase());
        if got.as_deref() != pair[2].as_str() {
            problems.push(
                Problem::new(
                    name,
                    "kernel",
                    "arcs",
                    format!("faces {a}-{b} {got:?} vs {}", pair[2]),
                )
                .on(format!("{a}-{b}")),
            );
        }
    }
}

#[test]
fn rust_snapshot_corpus() {
    let Some(dir) = common::corpus_dir() else {
        assert!(std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none());
        return;
    };
    let corpus = common::load("corpus.json");
    let actual = common::parallel::map(corpus["files"].as_array().unwrap(), |entry| {
        let file = entry["file"].as_str().unwrap();
        let part = read_step_file(&dir.join(file)).unwrap();
        let mut results = serde_json::Map::new();
        for (function, runs) in entry["results"].as_object().unwrap() {
            let runs: Vec<Value> = runs.as_array().unwrap().iter().map(|run| {
                let mut result = serde_json::json!({"options":run["options"], "records":common::recognise(function, &part, &run["options"])});
                if run["check_evidence"] == true {
                    result["defining"] = serde_json::to_value(common::defining(function,&part,&run["options"])).unwrap();
                }
                result
            }).collect();
            results.insert(function.clone(), Value::Array(runs));
        }
        common::snapshot::check_with(
            &common::fixtures(),
            &format!("kernel/{file}"),
            &common::rust_kernel(&part),
            &common::same_kernel_snapshot,
        );
        let recognised = quiddity::correspondence::recognise(&part);
        serde_json::json!({"file":file,"results":results,"document":quiddity::recognition::document(&recognised,part.faces.len()),"fingerprints":recognised.fingerprints})
    });
    common::snapshot::check(&common::fixtures(), "corpus", &serde_json::json!(actual));
}
