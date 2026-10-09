//! The section-recess records against Python's (`tools/capture_section_recess.py`, fixture
//! `tests/fixtures/captured/section_recess/values.json.gz`): every construction of a
//! `quiddity._section_recess` record that the section-recess Python tests and
//! `build_section_recess_document` over the corpus make, with its inputs and its `to_dict()` or
//! its refusal. Each is rebuilt here from the same inputs (nested records through their own
//! constructors) and must give the same dictionary (floats to one part in a million,
//! `common::same`) or the same message.
//!
//! The geometric records are also built moved by `tests/invariance.rs`'s motions: a pattern's
//! directions and centre under every motion, and a geometry on a principal run under the
//! translation (its frame origin across the run, its interval, cylinder axis height and plane
//! term heights along it, each on its grid). Each must be accepted and publish the moved
//! values. A geometry under a rotation is a different canonical value (its profile re-expressed
//! in the rotated run's canonical basis and re-canonicalised), which the projection that builds
//! it, not these records, owns.
//!
//! Differences are listed in `tests/fixtures/captured/section_recess/known_differences.json`:
//! per entry its `case` (`value` or `invariance`), the record's `class` and `given` inputs (and
//! an invariance entry's `motion`), a verdict and a reason. A failing run prints the entries it
//! needs.

mod common;

use std::io::Read;

use quiddity::features::cylindrical_end_surface::CylindricalEndSurface;
use quiddity::features::passages::{PassageFrame, PassageSectionVertex};
use quiddity::features::section_recess::{
    ClosedSectionProfile, EndSurface, OpenSectionProfile, PlanarEndSurface, PlanarEndTerm,
    PlanarEnvelopeEndSurface, SectionEnd, SectionProfile, SectionRecess, SectionRecessArray,
    SectionRecessBodyRef, SectionRecessClassification, SectionRecessDocument, SectionRecessEnds,
    SectionRecessEvidence, SectionRecessFaceRef, SectionRecessGeometry, SectionRecessGrid,
    SectionRecessPattern, SectionRecessRefusal,
};
use serde_json::{Value, json};

const KNOWN: &str = "captured/section_recess/known_differences.json";

fn load_gz(name: &str) -> Value {
    let path = common::fixtures()
        .join("captured/section_recess")
        .join(name);
    let mut text = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(path).unwrap())
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}

/// A captured float: a number, or a non-finite one written as its Python string.
fn float(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap(),
        Value::String(s) => match s.as_str() {
            "nan" => f64::NAN,
            "inf" => f64::INFINITY,
            "-inf" => f64::NEG_INFINITY,
            _ => panic!("not a float: {s}"),
        },
        _ => panic!("not a float: {v}"),
    }
}

fn floats<const N: usize>(v: &Value) -> [f64; N] {
    let a = v.as_array().unwrap();
    assert_eq!(a.len(), N, "{v}");
    std::array::from_fn(|i| float(&a[i]))
}

fn text(v: &Value) -> &str {
    v.as_str().unwrap_or_else(|| panic!("not a string: {v}"))
}

fn index(v: &Value) -> usize {
    v.as_u64().unwrap_or_else(|| panic!("not an index: {v}")) as usize
}

fn indices(v: &Value) -> Vec<usize> {
    v.as_array().unwrap().iter().map(index).collect()
}

fn class(v: &Value) -> &str {
    text(&v["record"])
}

fn items(v: &Value) -> &Vec<Value> {
    v.as_array().unwrap()
}

/// A nested record's refusal, named so it cannot pass for the outer record's own.
type Built<T> = Result<T, String>;

fn nested<T, E: std::fmt::Display>(name: &str, r: Result<T, E>) -> Built<T> {
    r.map_err(|e| format!("nested {name} refused: {e}"))
}

fn own<T, E: std::fmt::Display>(r: Result<T, E>) -> Built<T> {
    r.map_err(|e| e.to_string())
}

fn frame(v: &Value) -> Built<PassageFrame> {
    nested(
        "PassageFrame",
        PassageFrame::new(
            floats(&v["origin"]),
            floats(&v["run"]),
            floats(&v["u"]),
            floats(&v["v"]),
        ),
    )
}

fn vertices(v: &Value) -> Built<Vec<PassageSectionVertex>> {
    items(v)
        .iter()
        .map(|p| {
            nested(
                "PassageSectionVertex",
                PassageSectionVertex::new(floats(&p["point"]), float(&p["bulge"])),
            )
        })
        .collect()
}

fn closed_profile(v: &Value) -> Built<ClosedSectionProfile> {
    own(ClosedSectionProfile::new(
        text(&v["closure"]),
        vertices(&v["boundary"])?,
    ))
}

fn open_profile(v: &Value) -> Built<OpenSectionProfile> {
    let opening = items(&v["opening"]);
    own(OpenSectionProfile::new(
        text(&v["closure"]),
        vertices(&v["boundary"])?,
        [floats(&opening[0]), floats(&opening[1])],
        v["material_side"].as_str(),
    ))
}

fn profile(v: &Value) -> Built<SectionProfile> {
    match class(v) {
        "ClosedSectionProfile" => Ok(SectionProfile::Closed(nested(
            "ClosedSectionProfile",
            closed_profile(v),
        )?)),
        "OpenSectionProfile" => Ok(SectionProfile::Open(nested(
            "OpenSectionProfile",
            open_profile(v),
        )?)),
        other => panic!("not a profile: {other}"),
    }
}

fn planar(v: &Value) -> Built<PlanarEndSurface> {
    own(PlanarEndSurface::new(
        text(&v["type"]),
        floats(&v["gradient"]),
    ))
}

fn term(v: &Value) -> Built<PlanarEndTerm> {
    own(PlanarEndTerm::new(
        float(&v["height"]),
        floats(&v["gradient"]),
    ))
}

fn envelope(v: &Value) -> Built<PlanarEnvelopeEndSurface> {
    let terms = items(&v["terms"]);
    own(PlanarEnvelopeEndSurface::new(
        text(&v["type"]),
        text(&v["operator"]),
        [
            nested("PlanarEndTerm", term(&terms[0]))?,
            nested("PlanarEndTerm", term(&terms[1]))?,
        ],
    ))
}

fn surface(v: &Value) -> Built<EndSurface> {
    Ok(match class(v) {
        "PlanarEndSurface" => EndSurface::Planar(nested("PlanarEndSurface", planar(v))?),
        "PlanarEnvelopeEndSurface" => {
            EndSurface::PlanarEnvelope(nested("PlanarEnvelopeEndSurface", envelope(v))?)
        }
        "CylindricalEndSurface" => EndSurface::Cylindrical(nested(
            "CylindricalEndSurface",
            CylindricalEndSurface::new(
                text(&v["type"]),
                floats(&v["axis_point"]),
                floats(&v["axis_direction"]),
                float(&v["radius"]),
                text(&v["branch"]),
            ),
        )?),
        other => panic!("not an end surface: {other}"),
    })
}

fn end(v: &Value) -> Built<SectionEnd> {
    own(SectionEnd::new(
        text(&v["condition"]),
        surface(&v["surface"])?,
    ))
}

fn ends(v: &Value) -> Built<SectionRecessEnds> {
    own(SectionRecessEnds::new(
        nested("SectionEnd", end(&v["low"]))?,
        nested("SectionEnd", end(&v["high"]))?,
    ))
}

fn geometry(v: &Value) -> Built<SectionRecessGeometry> {
    let [lo, hi] = floats(&v["run_interval"]);
    own(SectionRecessGeometry::new(
        text(&v["type"]),
        frame(&v["frame"])?,
        (lo, hi),
        profile(&v["profile"])?,
        nested("SectionRecessEnds", ends(&v["ends"]))?,
    ))
}

fn classification(v: &Value) -> Built<SectionRecessClassification> {
    own(SectionRecessClassification::new(
        text(&v["feature_kind"]),
        text(&v["section_shape"]),
    ))
}

fn evidence(v: &Value) -> Built<SectionRecessEvidence> {
    own(SectionRecessEvidence::new(
        indices(&v["defining_faces"]),
        indices(&v["constituent_faces"]),
    ))
}

fn recess(v: &Value) -> Built<SectionRecess> {
    own(SectionRecess::new(
        index(&v["index"]),
        index(&v["body"]),
        nested("SectionRecessGeometry", geometry(&v["geometry"]))?,
        nested(
            "SectionRecessClassification",
            classification(&v["classification"]),
        )?,
        nested("SectionRecessEvidence", evidence(&v["evidence"]))?,
    ))
}

fn refusal(v: &Value) -> Built<SectionRecessRefusal> {
    own(SectionRecessRefusal::new(
        index(&v["body"]),
        text(&v["reason"]),
        nested("SectionRecessEvidence", evidence(&v["evidence"]))?,
    ))
}

fn array(v: &Value) -> Built<SectionRecessArray> {
    own(SectionRecessArray::new(
        indices(&v["members"]),
        float(&v["pitch"]),
        floats(&v["direction"]),
    ))
}

fn grid(v: &Value) -> Built<SectionRecessGrid> {
    own(SectionRecessGrid::new(
        indices(&v["members"]),
        index(&v["rows"]),
        index(&v["cols"]),
        float(&v["row_pitch"]),
        float(&v["col_pitch"]),
        floats(&v["row_direction"]),
        floats(&v["col_direction"]),
        floats(&v["center"]),
    ))
}

fn pattern(v: &Value) -> Built<SectionRecessPattern> {
    Ok(match class(v) {
        "SectionRecessArray" => {
            SectionRecessPattern::Array(nested("SectionRecessArray", array(v))?)
        }
        "SectionRecessGrid" => SectionRecessPattern::Grid(nested("SectionRecessGrid", grid(v))?),
        other => panic!("not a pattern: {other}"),
    })
}

fn roster<T>(v: &Value, f: impl Fn(&Value) -> Built<T>) -> Built<Vec<T>> {
    items(v).iter().map(f).collect()
}

fn document(v: &Value) -> Built<SectionRecessDocument> {
    own(SectionRecessDocument::new(
        v["schema_version"].as_i64().unwrap(),
        text(&v["reference_scope"]),
        items(&v["bodies"])
            .iter()
            .map(|b| SectionRecessBodyRef {
                index: index(&b["index"]),
            })
            .collect(),
        items(&v["faces"])
            .iter()
            .map(|f| SectionRecessFaceRef {
                index: index(&f["index"]),
            })
            .collect(),
        roster(&v["occurrences"], |o| nested("SectionRecess", recess(o)))?,
        roster(&v["refusals"], |r| {
            nested("SectionRecessRefusal", refusal(r))
        })?,
        roster(&v["patterns"], pattern)?,
    ))
}

fn json_of<T: serde::Serialize>(r: Built<T>) -> Built<Value> {
    r.map(|value| serde_json::to_value(value).unwrap())
}

/// The port's answer for one captured construction: its dictionary or its refusal.
fn build(given: &Value) -> Built<Value> {
    match class(given) {
        "ClosedSectionProfile" => json_of(closed_profile(given)),
        "OpenSectionProfile" => json_of(open_profile(given)),
        "PlanarEndSurface" => json_of(planar(given)),
        "PlanarEndTerm" => json_of(term(given)),
        "PlanarEnvelopeEndSurface" => json_of(envelope(given)),
        "SectionEnd" => json_of(end(given)),
        "SectionRecessEnds" => json_of(ends(given)),
        "SectionRecessGeometry" => json_of(geometry(given)),
        "SectionRecessClassification" => json_of(classification(given)),
        "SectionRecessEvidence" => json_of(evidence(given)),
        "SectionRecessBodyRef" => Ok(json!({"index": index(&given["index"])})),
        "SectionRecessFaceRef" => Ok(json!({"index": index(&given["index"])})),
        "SectionRecess" => json_of(recess(given)),
        "SectionRecessRefusal" => json_of(refusal(given)),
        "SectionRecessArray" => json_of(array(given)),
        "SectionRecessGrid" => json_of(grid(given)),
        "SectionRecessDocument" => json_of(document(given)),
        other => panic!("unknown record {other}"),
    }
}

fn answer(r: &Built<Value>) -> Value {
    match r {
        Ok(dict) => json!({"dict": dict}),
        Err(message) => json!({"refused": message}),
    }
}

fn check_known(case: &str, found: Vec<(Value, String)>) {
    let known = common::load(KNOWN);
    let known = known.as_array().unwrap();
    common::check_verdicts("known_differences.json", known);
    let listed: Vec<&Value> = known.iter().filter(|k| k["case"] == case).collect();
    let mut problems = Vec::new();
    let mut seen = vec![false; listed.len()];
    for (identity, text) in &found {
        let position = listed.iter().position(|k| {
            identity
                .as_object()
                .unwrap()
                .iter()
                .all(|(key, value)| &k[key] == value)
        });
        match position {
            Some(i) if seen[i] => problems.push(format!("listed twice: {identity}")),
            Some(i) => seen[i] = true,
            None => problems.push(format!("unlisted difference: {text}\n  entry: {identity}")),
        }
    }
    for (i, k) in listed.iter().enumerate() {
        if !seen[i] {
            problems.push(format!("listed entry no longer differs: {k}"));
        }
    }
    assert!(problems.is_empty(), "{case}:\n{}", problems.join("\n"));
}

#[test]
fn records_agree_with_python() {
    let captured = load_gz("values.json.gz");
    let values = items(&captured["values"]);
    assert!(!values.is_empty());
    let mut found = Vec::new();
    let mut refused = 0;
    for record in values {
        let given = &record["given"];
        let want = if record.get("dict").is_some() {
            json!({"dict": record["dict"]})
        } else {
            refused += 1;
            json!({"refused": record["refused"]})
        };
        let got = answer(&build(given));
        if !common::same(&got, &want) {
            found.push((
                json!({"case": "value", "class": class(given), "given": given}),
                format!("{} {given}: {}", class(given), common::diff(&got, &want)),
            ));
        }
    }
    eprintln!(
        "section-recess records: {} captured ({refused} refused), {} differ, skipped by the \
         capture {}",
        values.len(),
        found.len(),
        captured["skipped"]
    );
    check_known("value", found);
}

/// `tests/invariance.rs`'s motions: a non-round translation and right-angle rotations.
const T: [f64; 3] = [123.456, -78.9, 41.3];
/// A named motion: rotation rows and translation.
type Motion = (&'static str, [[f64; 3]; 3], [f64; 3]);
const MOTIONS: [Motion; 6] = [
    (
        "translate",
        [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        T,
    ),
    (
        "rot_z90",
        [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        [0.0; 3],
    ),
    (
        "rot_x90",
        [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]],
        [0.0; 3],
    ),
    (
        "rot_y180",
        [[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]],
        [0.0; 3],
    ),
    (
        "cycle_xyz",
        [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        [0.0; 3],
    ),
    (
        "rot_zx_moved",
        [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        T,
    ),
];

fn rotate(r: &[[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    // Exact: every entry is ±1 or 0, so each component is one input component, signed.
    r.map(|row| row[0] * v[0] + row[1] * v[1] + row[2] * v[2])
}

fn round(value: f64, digits: i32) -> f64 {
    let text = format!("{value:.*}", digits as usize);
    text.parse().unwrap()
}

/// A pattern's inputs moved: directions turned, the centre turned and translated.
fn moved_pattern(given: &Value, r: &[[f64; 3]; 3], t: [f64; 3]) -> Value {
    let mut moved = given.clone();
    for key in ["direction", "row_direction", "col_direction"] {
        if given.get(key).is_some() {
            moved[key] = json!(rotate(r, floats(&given[key])));
        }
    }
    if given.get("center").is_some() {
        let c = rotate(r, floats(&given["center"]));
        moved["center"] = json!([0, 1, 2].map(|i| c[i] + t[i]));
    }
    moved
}

/// A geometry on a principal run moved by *t*: the frame origin across the run, and the
/// interval, a cylinder's axis height and the plane terms' heights along it, each on its grid.
/// `None` for an oblique run, where the moved values leave their grids.
fn translated_geometry(given: &Value, t: [f64; 3]) -> Option<Value> {
    let run: [f64; 3] = floats(&given["frame"]["run"]);
    let axis = (0..3).find(|&i| run[i] == 1.0)?;
    let along = t[axis];
    let mut moved = given.clone();
    let origin: [f64; 3] = floats(&given["frame"]["origin"]);
    moved["frame"]["origin"] = json!([0, 1, 2].map(|i| if i == axis {
        origin[i]
    } else {
        round(origin[i] + t[i], 3)
    }));
    let [lo, hi] = floats(&given["run_interval"]);
    moved["run_interval"] = json!([round(lo + along, 3), round(hi + along, 3)]);
    for side in ["low", "high"] {
        let surface = &mut moved["ends"][side]["surface"];
        match surface["record"].as_str().unwrap() {
            "CylindricalEndSurface" => {
                let z = float(&surface["axis_point"][2]);
                surface["axis_point"][2] = json!(round(z + along, 6));
            }
            "PlanarEnvelopeEndSurface" => {
                for term in surface["terms"].as_array_mut().unwrap() {
                    let h = float(&term["height"]);
                    term["height"] = json!(round(h + along, 6));
                }
            }
            _ => {}
        }
    }
    Some(moved)
}

/// The dictionary a record publishes for these inputs: its fields without the class tag, the
/// end surfaces and nested records alike.
fn published(given: &Value) -> Value {
    match given {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(k, _)| k.as_str() != "record")
                .map(|(k, v)| (k.clone(), published(v)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(published).collect()),
        other => other.clone(),
    }
}

#[test]
fn geometric_records_do_not_depend_on_placement() {
    let captured = load_gz("values.json.gz");
    let accepted: Vec<&Value> = items(&captured["values"])
        .iter()
        .filter(|r| r.get("dict").is_some())
        .map(|r| &r["given"])
        .collect();
    let mut found = Vec::new();
    let (mut checked, mut oblique) = (0, 0);
    for given in &accepted {
        let moves: Vec<(&str, Value)> = match class(given) {
            "SectionRecessArray" | "SectionRecessGrid" => MOTIONS
                .iter()
                .map(|(name, r, t)| (*name, moved_pattern(given, r, *t)))
                .collect(),
            "SectionRecessGeometry" => match translated_geometry(given, T) {
                Some(moved) => vec![("translate", moved)],
                None => {
                    oblique += 1;
                    continue;
                }
            },
            _ => continue,
        };
        for (motion, moved) in moves {
            checked += 1;
            let got = answer(&build(&moved));
            let want = json!({"dict": published(&moved)});
            if !common::same(&got, &want) {
                found.push((
                    json!({"case": "invariance", "class": class(given), "given": given,
                           "motion": motion}),
                    format!(
                        "{} {motion} {given}: {}",
                        class(given),
                        common::diff(&got, &want)
                    ),
                ));
            }
        }
    }
    eprintln!(
        "moved records: {checked} built, {} differ; {oblique} geometries on oblique runs not moved",
        found.len()
    );
    assert!(checked > 0);
    check_known("invariance", found);
}
