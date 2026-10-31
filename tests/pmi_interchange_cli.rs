#![cfg(feature = "json")]
use quiddity::kernel::{
    json::{self, Anchors},
    p21::Document,
    pmi::*,
    step::read_part_definitions,
};
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};
fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_quiddity"))
        .args(args)
        .output()
        .unwrap()
}
fn scratch() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("pmi_interchange_cli");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
fn model() -> PartPmi {
    PartPmi {
        features: vec![Feature::Items(vec![Anchor::Face(FaceIndex(0))])],
        dimensions: vec![Dimension {
            kind: DimensionKind::Size {
                feature: FeatureId(0),
                kind: SizeKind::Thickness,
                path: None,
                angle: None,
            },
            nominal: Some(Value::length(
                Decimal::parse("10.000").unwrap(),
                LengthUnit::Millimetre,
            )),
            tolerance: DimTolerance::Basic,
            qualifier: None,
            modifiers: vec![],
            principle: None,
        }],
        notes: vec![Note {
            kind: "general note".into(),
            text: "Deburr all edges".into(),
            on: None,
            callouts: vec![],
        }],
        ..PartPmi::default()
    }
}
#[test]
fn export_validate_import_and_presentation() {
    let cube = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ties/cube.step");
    let dir = scratch();
    let path = dir.join("pmi.json");
    let output = dir.join("output.step.gz");
    let exported = cli(&["pmi", "json", "export", cube]);
    assert!(
        exported.status.success(),
        "{}",
        String::from_utf8_lossy(&exported.stderr)
    );
    std::fs::write(&path, &exported.stdout).unwrap();
    let checked = cli(&["pmi", "json", "validate", path.to_str().unwrap()]);
    assert!(checked.status.success());
    let mut semantic = json::identify(model(), None).unwrap();
    semantic.ids.get_mut("dimensions").unwrap()[0] = "caller inspection 17".into();
    std::fs::write(&path, json::to_string(&semantic).unwrap()).unwrap();
    let imported = cli(&[
        "pmi",
        "json",
        "import",
        cube,
        path.to_str().unwrap(),
        "-o",
        output.to_str().unwrap(),
    ]);
    assert!(
        imported.status.success(),
        "{}",
        String::from_utf8_lossy(&imported.stderr)
    );
    let exported = cli(&["pmi", "json", "export", output.to_str().unwrap()]);
    assert!(
        exported.status.success(),
        "{}",
        String::from_utf8_lossy(&exported.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&exported.stdout).unwrap();
    assert_eq!(value["pmi"]["dimensions"][0]["id"], "caller inspection 17");
    let input = std::fs::read(cube).unwrap();
    let doc = Document::parse(input.clone()).unwrap();
    let parts = read_part_definitions(&input).unwrap();
    let decoded = json::from_value(&value, &Anchors::read(&doc, &parts[0])).unwrap();
    assert!(json::identity_differences(&model(), &decoded.pmi).is_empty());
    let nist = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/ap242/nist/nist_ctc_01_asme1_ap242-e1.stp.gz"
    );
    let drawn = cli(&["pmi", "json", "presentation", nist]);
    assert!(
        drawn.status.success(),
        "{}",
        String::from_utf8_lossy(&drawn.stderr)
    );
    std::fs::write(&path, &drawn.stdout).unwrap();
    assert!(
        cli(&["pmi", "json", "validate", path.to_str().unwrap()])
            .status
            .success()
    );
}
#[test]
fn malformed_and_unknown_faces_refuse_without_output() {
    let cube = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ties/cube.step");
    let dir = scratch();
    let path = dir.join("invalid.json");
    let out = dir.join("refused.step");
    let _ = std::fs::remove_file(&out);
    let mut value = json::to_value(&json::identify(model(), None).unwrap()).unwrap();
    value["pmi"]["features"][0]["value"]["items"][0]["face"] = json!({"name":"missing"});
    std::fs::write(&path, value.to_string()).unwrap();
    let result = cli(&[
        "pmi",
        "json",
        "import",
        cube,
        path.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("unknown face name"));
    assert!(!out.exists());
    std::fs::write(
        &path,
        r#"{"schema":"haecceity.pmi/1","schema":"haecceity.pmi/1"}"#,
    )
    .unwrap();
    let result = cli(&["pmi", "json", "validate", path.to_str().unwrap()]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("duplicate key"));
}

/// Every authored semantic fixture accepted by CLI validate must import with its geometry.
#[test]
fn every_accepted_semantic_fixture_imports() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/haecceity/tests/fixtures");
    let mut pending = vec![root];
    let mut accepted = 0;
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            let value: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            if value["schema"] != "haecceity.pmi/1" {
                continue;
            }
            let checked = cli(&["pmi", "json", "validate", path.to_str().unwrap()]);
            if !checked.status.success() {
                continue;
            }
            accepted += 1;
            let parent = path.parent().unwrap();
            let geometry = if parent.ends_with("xcaf") {
                parent.join("pmi_plate.geom.step.gz")
            } else {
                parent.join("geometry.step.gz")
            };
            assert!(
                geometry.is_file(),
                "accepted fixture needs geometry: {}",
                path.display()
            );
            let output = scratch().join(format!("fixture-{accepted}.step"));
            let imported = cli(&[
                "pmi",
                "json",
                "import",
                geometry.to_str().unwrap(),
                path.to_str().unwrap(),
                "-o",
                output.to_str().unwrap(),
            ]);
            assert!(
                imported.status.success(),
                "{}: {}",
                path.display(),
                String::from_utf8_lossy(&imported.stderr)
            );
            let exported = cli(&["pmi", "json", "export", output.to_str().unwrap()]);
            assert!(
                exported.status.success(),
                "{}",
                String::from_utf8_lossy(&exported.stderr)
            );
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&exported.stdout).unwrap(),
                value
            );
        }
    }
    assert!(
        accepted >= 3,
        "the xcaf fixture and both #142 regressions must be accepted"
    );
}
