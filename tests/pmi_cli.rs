//! `quiddity parts`, `quiddity pmi read` and `quiddity pmi check`, and the JSON form of the PMI
//! model (`quiddity::pmi_json`), over the AP242 fixtures: the committed NIST models, the
//! specify-core-written inputs and the two-part assembly, plus every NIST AP242 file when
//! `HAECCEITY_NIST_PMI` names their directory (required when `HAECCEITY_NIST_PMI_REQUIRED=1`).

use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use quiddity::kernel::p21::Document;
use quiddity::kernel::pmi::{self, PmiRead};
use quiddity::kernel::step::{PartDefinition, read_part_definitions};
use quiddity::pmi_json::{self, Binding};
use serde_json::{Value as Json, json};

fn quiddity(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_quiddity"))
        .args(args)
        .output()
        .expect("quiddity runs")
}

fn ap242() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ap242")
}

fn scratch() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("pmi_cli");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The committed AP242 inputs: NIST models, specify-core outputs, the assembly.
fn fixtures() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for (dir, suffix) in [("nist", ".stp.gz"), ("specify", ".step.gz")] {
        let mut names: Vec<PathBuf> = std::fs::read_dir(ap242().join(dir))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.to_string_lossy().ends_with(suffix))
            .collect();
        names.sort();
        out.extend(names);
    }
    out.push(ap242().join("assembly/assembly.step"));
    out
}

/// Every NIST AP242 file with a B-rep part, when `HAECCEITY_NIST_PMI` names their directory.
/// The tessellated-only FTC-08 edition (`-tg`) has no B-rep part to anchor PMI to: `parts` and
/// `pmi read` refuse it (checked here), never report it as a file without PMI.
fn nist_files() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for path in all_nist_files() {
        if name(&path).contains("-tg.") {
            let p = path.to_str().unwrap();
            for args in [&["parts", p][..], &["pmi", "read", p]] {
                let err = refused(args);
                assert!(err.contains("no solid or shell with faces"), "{err}");
            }
        } else {
            out.push(path);
        }
    }
    out
}

fn all_nist_files() -> Vec<PathBuf> {
    let Some(dir) = std::env::var_os("HAECCEITY_NIST_PMI").map(PathBuf::from) else {
        assert!(
            std::env::var_os("HAECCEITY_NIST_PMI_REQUIRED").is_none(),
            "HAECCEITY_NIST_PMI_REQUIRED is set but HAECCEITY_NIST_PMI is not"
        );
        return Vec::new();
    };
    let mut names: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| {
            let n = p.file_name().unwrap().to_string_lossy().to_lowercase();
            n.contains("ap242") && (n.ends_with(".stp") || n.ends_with(".step"))
        })
        .collect();
    names.sort();
    assert!(!names.is_empty(), "no AP242 files in {}", dir.display());
    names
}

fn bytes(path: &Path) -> Vec<u8> {
    let raw = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    if path.extension().is_some_and(|e| e == "gz") {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(raw.as_slice())
            .read_to_end(&mut out)
            .unwrap();
        out
    } else {
        raw
    }
}

fn read(path: &Path) -> (Vec<u8>, Vec<PartDefinition>, PmiRead) {
    let b = bytes(path);
    let defs = read_part_definitions(&b).expect("part definitions");
    let doc = Document::parse(b.clone()).expect("document");
    let r = pmi::read(&doc, &defs).expect("pmi");
    (b, defs, r)
}

fn name(path: &Path) -> String {
    path.file_name().unwrap().to_string_lossy().into_owned()
}

/// Runs a command that must succeed with empty stderr; its stdout as JSON and as bytes.
fn ok(args: &[&str]) -> (Json, Vec<u8>) {
    let out = quiddity(args);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "{args:?}: {stderr}");
    assert!(stderr.is_empty(), "{args:?}: {stderr}");
    let json = serde_json::from_slice(&out.stdout).expect("stdout is JSON");
    (json, out.stdout)
}

/// Runs a command that must be refused with exit code 1, nothing on stdout; its stderr.
fn refused(args: &[&str]) -> String {
    let out = quiddity(args);
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(1), "{args:?}: {stderr}");
    assert!(out.stdout.is_empty(), "{args:?} printed a result");
    assert!(stderr.starts_with("quiddity: "), "{stderr}");
    stderr
}

#[test]
fn parts_lists_every_distinct_part_with_its_binding() {
    for path in fixtures() {
        let p = path.to_str().unwrap();
        let (json, _) = ok(&["parts", p]);
        let b = bytes(&path);
        let defs = read_part_definitions(&b).unwrap();
        assert_eq!(json["format"], "quiddity-parts", "{p}");
        assert_eq!(json["version"], pmi_json::VERSION, "{p}");
        assert_eq!(json["binding"]["sha256"], Binding::of(&b).sha256, "{p}");
        assert_eq!(json["binding"]["reader"], pmi_json::READER, "{p}");
        let parts = json["parts"].as_array().unwrap();
        assert_eq!(parts.len(), defs.len(), "{p}");
        for (i, (j, d)) in parts.iter().zip(&defs).enumerate() {
            assert_eq!(j["part"], i, "{p}");
            assert_eq!(j["name"], d.name.as_str(), "{p}");
            assert_eq!(j["faces"], d.faces.len(), "{p}");
            assert_eq!(j["edges"], d.edges.len(), "{p}");
            assert_eq!(
                j["placements"].as_array().unwrap().len(),
                d.placements.len(),
                "{p}"
            );
        }
    }
    // The assembly: a plate and a pin placed twice, one part with two placements. Its sha256
    // as `shasum -a 256` gives it.
    let (json, _) = ok(&[
        "parts",
        ap242().join("assembly/assembly.step").to_str().unwrap(),
    ]);
    assert_eq!(
        json["binding"]["sha256"],
        "ca5711668102f0e8bb9920469a46008e8a51d60e0bea7eb89a03089b0e464291"
    );
    let parts = json["parts"].as_array().unwrap();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0]["name"], "plate");
    assert_eq!(parts[1]["name"], "pin");
    assert_eq!(parts[1]["placements"].as_array().unwrap().len(), 2);
    assert_eq!(parts[1]["placements"][0][0][3], -30.0);
}

#[test]
fn pmi_read_gives_every_part_and_finding_in_the_json_form() {
    for path in fixtures() {
        let p = path.to_str().unwrap();
        let (json, stdout) = ok(&["pmi", "read", p]);
        assert_eq!(json["format"], pmi_json::FORMAT, "{p}");
        assert_eq!(json["version"], pmi_json::VERSION, "{p}");
        let doc = pmi_json::document_from_json(&json).unwrap_or_else(|e| panic!("{p}: {e}"));
        let (b, defs, r) = read(&path);
        assert_eq!(doc.binding, Binding::of(&b), "{p}");
        assert_eq!(doc.parts.len(), r.parts.len(), "{p}");
        for (i, part) in doc.parts.iter().enumerate() {
            assert_eq!(part.part.0, i, "{p}");
            assert_eq!(part.name.as_deref(), Some(defs[i].name.as_str()), "{p}");
            assert_eq!(part.pmi, r.parts[i], "{p}: part {i}");
        }
        assert_eq!(doc.findings, r.findings, "{p}");
        // Deterministic: the same bytes on a second run (fresh hash seeds).
        assert_eq!(quiddity(&["pmi", "read", p]).stdout, stdout, "{p}");
    }
}

#[test]
fn pmi_read_of_one_part() {
    let path = ap242().join("specify/assembly_plate_pin.step.gz");
    let p = path.to_str().unwrap();
    let (all, _) = ok(&["pmi", "read", p]);
    let n = all["parts"].as_array().unwrap().len();
    assert!(n >= 2, "the assembly has two parts");
    for i in 0..n {
        let (one, _) = ok(&["pmi", "read", p, "--part", &i.to_string()]);
        let parts = one["parts"].as_array().unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0], all["parts"][i]);
        for f in one["findings"].as_array().unwrap() {
            assert!(f.get("part").is_none_or(|q| *q == i), "{f}");
        }
    }
    let err = refused(&["pmi", "read", p, "--part", &n.to_string()]);
    assert!(err.contains(&format!("there is no part {n}")), "{err}");
    let out = quiddity(&["pmi", "read", p, "--part", "x"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("usage: quiddity"));
}

#[test]
fn every_read_part_round_trips_through_json() {
    let mut files = fixtures();
    files.extend(nist_files());
    let mut parts = 0;
    for path in files {
        let (_, _, r) = read(&path);
        for (i, p) in r.parts.iter().enumerate() {
            let json = pmi_json::to_json(p);
            let text = serde_json::to_string(&json).unwrap();
            let back = pmi_json::from_json(&json, "pmi")
                .unwrap_or_else(|e| panic!("{}: part {i}: {e}", name(&path)));
            assert_eq!(&back, p, "{}: part {i}", name(&path));
            // The bytes are stable: the same text again, and from the decoded part.
            assert_eq!(serde_json::to_string(&pmi_json::to_json(p)).unwrap(), text);
            let again = serde_json::to_string(&pmi_json::to_json(&back)).unwrap();
            assert_eq!(again, text, "{}: part {i}", name(&path));
            // From the text itself, as another process would read it.
            let reparsed: Json = serde_json::from_str(&text).unwrap();
            assert_eq!(&pmi_json::from_json(&reparsed, "pmi").unwrap(), p);
            parts += 1;
        }
    }
    assert!(parts >= 16, "{parts} parts");
}

/// Every REAL-like token of a STEP text.
fn real_tokens(text: &str) -> BTreeSet<&str> {
    text.split(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'E' | 'e')))
        .filter(|t| t.contains('.'))
        .collect()
}

/// Every `{"value", "unit"}` object of a JSON tree.
fn values<'a>(j: &'a Json, out: &mut Vec<(&'a str, &'a Json)>) {
    match j {
        Json::Object(m) => {
            if let (Some(Json::String(v)), Some(u)) = (m.get("value"), m.get("unit")) {
                out.push((v, u));
            }
            m.values().for_each(|x| values(x, out));
        }
        Json::Array(a) => a.iter().for_each(|x| values(x, out)),
        _ => {}
    }
}

#[test]
fn inch_values_keep_their_text() {
    let mut files: Vec<PathBuf> = fixtures()
        .into_iter()
        .filter(|p| name(p).starts_with("nist_"))
        .collect();
    files.extend(nist_files());
    let mut inch_files = 0;
    for path in files {
        let p = path.to_str().unwrap();
        let (json, _) = ok(&["pmi", "read", p]);
        let b = bytes(&path);
        let text = String::from_utf8_lossy(&b);
        let tokens = real_tokens(&text);
        let mut vs = Vec::new();
        values(&json["parts"], &mut vs);
        let inch: Vec<&str> = vs
            .iter()
            .filter(|(_, u)| *u == "in")
            .map(|(v, _)| *v)
            .collect();
        if inch.is_empty() {
            continue;
        }
        inch_files += 1;
        for v in inch {
            assert!(
                tokens.contains(v),
                "{}: inch value {v:?} is not a REAL token of the file",
                name(&path)
            );
        }
    }
    // ftc_07, stc_06 and stc_09 among the committed files.
    assert!(inch_files >= 3, "{inch_files} inch files");
}

/// `pmi read` of spool_fits (one part: datums, fits, tolerances, material, general tolerance)
/// as JSON, and the STEP path.
fn spool() -> (Json, String) {
    let path = ap242().join("specify/spool_fits.step.gz");
    let p = path.to_str().unwrap().to_string();
    let (json, _) = ok(&["pmi", "read", &p]);
    (json, p)
}

/// `pmi check` of `doc` against `step`.
fn check(step: &str, doc: &str, label: &str) -> Output {
    let file = scratch().join(format!("{label}.json"));
    std::fs::write(&file, doc).unwrap();
    quiddity(&["pmi", "check", step, file.to_str().unwrap()])
}

fn check_refused(step: &str, doc: &Json, label: &str) -> String {
    let out = check(step, &doc.to_string(), label);
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(1), "{label}: {stderr}");
    assert!(out.stdout.is_empty(), "{label}");
    stderr
}

#[test]
fn pmi_check_accepts_what_pmi_read_writes_and_prints_it_canonically() {
    for path in fixtures() {
        let p = path.to_str().unwrap();
        let (_, read_out) = ok(&["pmi", "read", p]);
        let out = check(p, &String::from_utf8_lossy(&read_out), &name(&path));
        assert_eq!(
            out.status.code(),
            Some(0),
            "{p}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, read_out, "{p}");
    }
}

#[test]
fn pmi_check_refusals_name_the_json_path() {
    let (json, step) = spool();
    let pmi = &json["parts"][0]["pmi"];
    assert!(pmi["dimensions"].as_array().unwrap().len() >= 4);

    // Not JSON.
    let out = check(&step, "{\"format\": ", "invalid");
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("not JSON"));

    let mut bad = json.clone();
    bad["parts"][0]["pmi"]["tolerances"][0]["kind"] = json!("runout");
    let err = check_refused(&step, &bad, "unknown-kind");
    assert!(
        err.contains("parts[0].pmi.tolerances[0].kind: unknown term \"runout\""),
        "{err}"
    );

    let mut bad = json.clone();
    bad["parts"][0]["pmi"]["tolerances"][0]["modifiers"] = json!(["MAXIMUM_MATERIAL_REQUIREMENT"]);
    let err = check_refused(&step, &bad, "upper-case-modifier");
    assert!(
        err.contains("parts[0].pmi.tolerances[0].modifiers[0]: unknown term"),
        "{err}"
    );

    let mut bad = json.clone();
    bad["parts"][0]["pmi"]["dimensions"][0]["tolerance"] =
        json!({"fit": {"deviation": "Hx", "grade": "IT7"}});
    let err = check_refused(&step, &bad, "unknown-deviation");
    assert!(
        err.contains("parts[0].pmi.dimensions[0].tolerance.fit.deviation"),
        "{err}"
    );

    let mut bad = json.clone();
    bad["parts"][0]["pmi"]["datums"][0]["colour"] = json!("red");
    let err = check_refused(&step, &bad, "unknown-field");
    assert!(
        err.contains("parts[0].pmi.datums[0].colour: unknown field"),
        "{err}"
    );

    let mut bad = json.clone();
    bad["extra"] = json!(1);
    let err = check_refused(&step, &bad, "unknown-top-field");
    assert!(err.contains("extra: unknown field"), "{err}");

    let mut bad = json.clone();
    bad["parts"][0]["pmi"]["features"][0] = json!({"items": [{"face": 100000}]});
    let err = check_refused(&step, &bad, "face-out-of-range");
    assert!(
        err.contains("parts[0].pmi.features[0].items[0].face: part 0 has"),
        "{err}"
    );
    assert!(err.contains("there is no face 100000"), "{err}");

    // Invariants: deviations with upper below lower; a datum label twice; a value as a number;
    // a reference to a missing feature; a flatness with datums.
    let mut bad = json.clone();
    bad["parts"][0]["pmi"]["dimensions"][2]["tolerance"] = json!({"deviations": {
        "upper": {"value": "-0.039", "unit": "mm"},
        "lower": {"value": "-0.014", "unit": "mm"}}});
    let err = check_refused(&step, &bad, "upper-below-lower");
    assert!(
        err.contains("parts[0].pmi.dimensions[2].tolerance.deviations: upper bound"),
        "{err}"
    );

    let mut bad = json.clone();
    bad["parts"][0]["pmi"]["datums"][1]["label"] = json!("A");
    let err = check_refused(&step, &bad, "label-twice");
    assert!(
        err.contains("parts[0].pmi: ") && err.contains("datum label A is used twice"),
        "{err}"
    );

    let mut bad = json.clone();
    bad["parts"][0]["pmi"]["dimensions"][0]["nominal"]["value"] = json!(62.0);
    let err = check_refused(&step, &bad, "float-value");
    assert!(
        err.contains("parts[0].pmi.dimensions[0].nominal.value: expected a string"),
        "{err}"
    );

    let mut bad = json.clone();
    bad["parts"][0]["pmi"]["dimensions"][0]["nominal"]["value"] = json!("6,2");
    let err = check_refused(&step, &bad, "not-decimal");
    assert!(err.contains("is not a decimal"), "{err}");

    let mut bad = json.clone();
    bad["parts"][0]["pmi"]["datums"][0]["feature"] = json!(999);
    let err = check_refused(&step, &bad, "missing-feature");
    assert!(err.contains("names missing feature 999"), "{err}");

    let mut bad = json.clone();
    let flat = bad["parts"][0]["pmi"]["tolerances"]
        .as_array()
        .unwrap()
        .iter()
        .position(|t| t["kind"] == "flatness")
        .unwrap();
    bad["parts"][0]["pmi"]["tolerances"][flat]["datums"] = json!([{"references": [{"datum": 0}]}]);
    let err = check_refused(&step, &bad, "flatness-datums");
    assert!(err.contains("takes no datums"), "{err}");

    let mut bad = json.clone();
    bad["parts"][0]["pmi"]["tolerances"][0]["datums"] = json!([]);
    let err = check_refused(&step, &bad, "empty-system");
    assert!(
        err.contains("parts[0].pmi.tolerances[0].datums: a datum system has 0"),
        "{err}"
    );

    // A length where an angle belongs, and an angle unit on a tolerance magnitude.
    let mut bad = json.clone();
    bad["parts"][0]["pmi"]["tolerances"][0]["magnitude"]["unit"] = json!("deg");
    let err = check_refused(&step, &bad, "angle-magnitude");
    assert!(
        err.contains("parts[0].pmi.tolerances[0].magnitude.unit: expected a length"),
        "{err}"
    );

    // The binding: another file, another reader, another format version.
    let mut bad = json.clone();
    bad["binding"]["sha256"] = json!("0".repeat(64));
    let err = check_refused(&step, &bad, "sha256");
    assert!(err.contains("binding.sha256"), "{err}");
    let mut bad = json.clone();
    bad["binding"]["reader"] = json!("haecceity-pmi-read/0");
    let err = check_refused(&step, &bad, "reader");
    assert!(err.contains("binding.reader"), "{err}");
    let mut bad = json.clone();
    bad["version"] = json!(2);
    let err = check_refused(&step, &bad, "version");
    assert!(err.contains("version: version 2 is not supported"), "{err}");
    let mut bad = json.clone();
    bad.as_object_mut().unwrap().remove("version");
    let err = check_refused(&step, &bad, "no-version");
    assert!(err.contains("version: missing"), "{err}");

    // A part the file does not have, a part listed twice, a wrong name.
    let mut bad = json.clone();
    bad["parts"][0]["part"] = json!(5);
    let err = check_refused(&step, &bad, "no-part");
    assert!(err.contains("parts[0].part: the file has 1 parts"), "{err}");
    let mut bad = json.clone();
    let p0 = bad["parts"][0].clone();
    bad["parts"].as_array_mut().unwrap().push(p0);
    let err = check_refused(&step, &bad, "part-twice");
    assert!(
        err.contains("parts[1].part: part 0 is listed twice"),
        "{err}"
    );
    let mut bad = json.clone();
    bad["parts"][0]["name"] = json!("not the part");
    let err = check_refused(&step, &bad, "name");
    assert!(err.contains("parts[0].name"), "{err}");
}

#[test]
fn hand_written_pmi_is_accepted_in_the_documented_form() {
    // What a consumer such as specify-core writes: datums A and B, Ø62 H7, a g6 shaft with
    // deviations wholly below nominal, a position with Ⓜ to A|B, a flatness, a general
    // tolerance; optional fields left out.
    let (json, step) = spool();
    let doc = json!({
        "format": "quiddity-pmi",
        "version": 1,
        "binding": json["binding"],
        "parts": [{"part": 0, "pmi": {
            "features": [
                {"items": [{"face": 69}]},
                {"items": [{"face": 55}]},
                {"items": [{"face": 0}, {"face": 16}]},
            ],
            "datums": [{"label": "A", "feature": 0}, {"label": "B", "feature": 1}],
            "dimensions": [
                {"kind": {"size": {"feature": 1, "kind": "diameter"}},
                 "nominal": {"value": "62", "unit": "mm"},
                 "tolerance": {"fit": {"deviation": "H", "grade": "IT7"}}},
                {"kind": {"size": {"feature": 2, "kind": "diameter"}},
                 "nominal": {"value": "130", "unit": "mm"},
                 "tolerance": {"deviations": {"upper": {"value": "-0.014", "unit": "mm"},
                                              "lower": {"value": "-0.039", "unit": "mm"}}}},
            ],
            "tolerances": [
                {"kind": "position", "target": {"feature": 2},
                 "magnitude": {"value": "0.2", "unit": "mm"},
                 "zone": {"form": "cylindrical or circular"},
                 "modifiers": ["maximum_material_requirement"],
                 "datums": [{"references": [{"datum": 0}]}, {"references": [{"datum": 1}]}]},
                {"kind": "flatness", "target": {"feature": 0},
                 "magnitude": {"value": "0.02", "unit": "mm"}},
            ],
            "general": [{"class": {"text": "ISO 2768-mK"}}],
            "material": {"id": "Aluminium 6082-T6"},
        }}],
    });
    let out = check(&step, &doc.to_string(), "hand-written");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let back: Json = serde_json::from_slice(&out.stdout).unwrap();
    let p = &back["parts"][0]["pmi"];
    // The text is kept as given; the ISO 2768 classes are recognised from it.
    assert_eq!(p["dimensions"][0]["nominal"]["value"], "62");
    assert_eq!(
        p["general"][0],
        json!({"class": {"text": "ISO 2768-mK", "standard": {"linear": "m", "geometric": "K"}}})
    );
    // A stated standard that disagrees with the text is refused.
    let mut bad = doc.clone();
    bad["parts"][0]["pmi"]["general"][0]["class"]["standard"] = json!({"linear": "f"});
    let err = check_refused(&step, &bad, "standard-disagrees");
    assert!(
        err.contains("parts[0].pmi.general[0].class.standard: does not agree"),
        "{err}"
    );
}
