//! The `quiddity` command line: usage, its output (the recognition document beside the records
//! and fingerprints), and honest failure on broken STEP files.
//!
//! The broken files under `tests/fixtures/broken/` are derived from `rejected_cylinder.step`
//! (a three-face cylinder): an empty DATA section, a write cut off mid-entity, a file cut at an
//! entity boundary and closed, one `ADVANCED_FACE` deleted, a closed edge's circle replaced by
//! a hyperbola (which the kernel does not model), and, read but reported, an open edge's line
//! replaced by a hyperbola.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn quiddity(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_quiddity"))
        .args(args)
        .output()
        .expect("quiddity runs")
}

fn broken(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/broken")
        .join(name)
}

/// Runs `quiddity <file>` and returns its stderr, asserting it failed with nothing on stdout.
fn refused(name: &str) -> String {
    let out = quiddity(&[broken(name).to_str().unwrap()]);
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(1), "{name}: {stderr}");
    assert!(out.stdout.is_empty(), "{name} printed a result");
    stderr
}

#[test]
fn help_prints_usage_and_succeeds() {
    for flag in ["-h", "--help"] {
        let out = quiddity(&[flag]);
        assert!(out.status.success(), "{flag}");
        let usage = String::from_utf8_lossy(&out.stdout);
        assert!(usage.starts_with("usage: quiddity"));
        for command in [
            "quiddity correspond",
            "quiddity parts <file.step>",
            "quiddity pmi read <file.step> [--part N]",
            "quiddity pmi check <file.step> <pmi.json>",
            "quiddity pmi write <file.step> <pmi.json> -o <out.step> [--mode add|replace|remove]",
            "[--presentation refuse|remove]",
        ] {
            assert!(usage.contains(command), "{flag}: {usage}");
        }
        assert!(out.stderr.is_empty());
    }
}

#[test]
fn the_usage_lists_serve() {
    let out = quiddity(&["--help"]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("quiddity serve"));
}

#[test]
fn serve_on_an_empty_input_succeeds_silently() {
    let out = Command::new(env!("CARGO_BIN_EXE_quiddity"))
        .arg("serve")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("quiddity runs");
    assert!(out.status.success());
    assert!(out.stdout.is_empty() && out.stderr.is_empty());
}

#[test]
fn bad_arguments_print_usage_and_fail() {
    let step = broken("../rejected_cylinder.step");
    let step = step.to_str().unwrap();
    for args in [
        &[][..],
        &["parts"],
        &["parts", step, "extra"],
        &["pmi"],
        &["pmi", step],
        &["pmi", "read"],
        &["pmi", "write", step],
        &["pmi", "read", step, "--part"],
        &["pmi", "read", step, "--part", "-1"],
        &["pmi", "read", step, "--parts", "0"],
        &["pmi", "check", step],
        &["pmi", "write", step, "pmi.json"],
        &["pmi", "write", step, "-o", "out.step"],
        &["pmi", "write", step, "pmi.json", "-o"],
        &["pmi", "write", step, "pmi.json", "extra", "-o", "out.step"],
        &[
            "pmi", "write", step, "pmi.json", "-o", "a.step", "-o", "b.step",
        ],
        &[
            "pmi", "write", step, "pmi.json", "-o", "out.step", "--mode", "merge",
        ],
        &["pmi", "write", step, "pmi.json", "-o", "out.step", "--mode"],
        &[
            "pmi",
            "write",
            step,
            "pmi.json",
            "-o",
            "out.step",
            "--presentation",
            "keep",
        ],
        &[
            "pmi",
            "write",
            step,
            "pmi.json",
            "-o",
            "out.step",
            "--presentation",
            "remove",
        ],
        &[
            "pmi", "write", step, "pmi.json", "-o", "out.step", "--force",
        ],
        &["correspond"],
    ] {
        let out = quiddity(args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(out.stdout.is_empty(), "{args:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("usage: quiddity"),
            "{args:?}"
        );
    }
}

#[test]
fn parts_and_pmi_read_refuse_a_broken_file() {
    for name in ["truncated.step", "deleted_face.step"] {
        let path = broken(name);
        let path = path.to_str().unwrap();
        for args in [&["parts", path][..], &["pmi", "read", path]] {
            let out = quiddity(args);
            assert_eq!(out.status.code(), Some(1), "{args:?}");
            assert!(out.stdout.is_empty(), "{args:?}");
            let err = String::from_utf8_lossy(&out.stderr);
            assert!(err.starts_with("quiddity: "), "{args:?}: {err}");
        }
    }
    let missing = broken("no_such_file.step");
    let out = quiddity(&["pmi", "read", missing.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn a_sound_file_is_recognised() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rejected_cylinder.step");
    let out = quiddity(&[path.to_str().unwrap()]);
    assert!(out.status.success());
    assert!(out.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(json.get("fingerprints").is_some());
    let document = &json["document"];
    assert_eq!(
        document["schema_version"],
        quiddity::recognition::SCHEMA_VERSION
    );
    assert!(document["records"].is_array());
    // Recognised in its own frame, reported in the file's coordinates.
    assert_eq!(document["coordinate_space"], "file");
    assert_eq!(document["frame"]["status"], "inferred");
}

/// A part with no plane or cylinder has no frame: it is recognised as placed in the file, and
/// says so on stderr and in the document.
#[test]
fn a_part_without_a_frame_is_recognised_as_placed_with_a_warning() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/captured/frames/sphere.step");
    let out = quiddity(&[path.to_str().unwrap()]);
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("warning: no part frame (no-analytic-direction); recognised as placed"),
        "{stderr}"
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        json["document"]["frame"],
        serde_json::json!({"status": "refused", "reason": "no-analytic-direction"})
    );
}

#[test]
fn the_document_lists_each_feature_with_its_record_type_and_faces() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/filleted_plate_with_holes.step");
    let out = quiddity(&[path.to_str().unwrap()]);
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let records: Vec<(&str, &str, &serde_json::Value)> = json["document"]["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["id"].as_str().unwrap(),
                r["record_type"].as_str().unwrap(),
                &r["faces"],
            )
        })
        .collect();
    let faces = |v: &[usize]| serde_json::json!(v);
    // Records are in the part's frame's order (its x is the file's z, its z the file's -x),
    // which does not depend on where the part is placed.
    assert_eq!(
        records,
        [
            ("fillets/0", "Fillet", &faces(&[7])),
            ("fillets/1", "Fillet", &faces(&[2])),
            ("fillets/2", "Fillet", &faces(&[8])),
            ("fillets/3", "Fillet", &faces(&[3])),
            ("holes/0", "HoleRecord", &faces(&[10])),
            ("holes/1", "HoleRecord", &faces(&[11])),
            ("holes/2", "HoleRecord", &faces(&[12])),
            ("hole_patterns/0", "LinearArray", &faces(&[10, 11, 12])),
            // recognise_blends finds the fillets' faces too, as Python's does (4 blends); Python's
            // aggregate preference of Fillet over Blend for one chain is not ported.
            ("blends/0", "Blend", &faces(&[7])),
            ("blends/1", "Blend", &faces(&[2])),
            ("blends/2", "Blend", &faces(&[8])),
            ("blends/3", "Blend", &faces(&[3])),
        ]
    );
    // The existing keys are kept: the records at the top level, the fingerprints beside them.
    assert_eq!(
        json["fillets"][0],
        json["document"]["records"][0]["parameters"]
    );
    assert!(json["fingerprints"]["features"].is_array());
}

#[test]
fn an_empty_data_section_is_refused() {
    let err = refused("empty_data.step");
    assert!(err.contains("no solid or shell"), "{err}");
}

#[test]
fn a_file_cut_off_mid_entity_is_refused() {
    let err = refused("truncated.step");
    assert!(err.contains("parse error"), "{err}");
}

#[test]
fn a_file_cut_at_an_entity_and_closed_is_refused() {
    let err = refused("truncated_closed.step");
    assert!(
        err.contains("#105 is referenced but not in the file"),
        "{err}"
    );
    assert!(
        err.contains("#109 is referenced but not in the file"),
        "{err}"
    );
}

#[test]
fn a_deleted_face_is_refused() {
    let err = refused("deleted_face.step");
    assert!(
        err.contains("#109 is referenced but not in the file"),
        "{err}"
    );
}

#[test]
fn an_unresolvable_closed_edge_curve_is_refused() {
    let err = refused("unresolved_closed_curve.step");
    assert!(err.contains("closed edge's curve (Hyperbola"), "{err}");
}

#[test]
fn an_unresolved_open_edge_curve_is_reported() {
    let out = quiddity(&[broken("unresolved_open_curve.step").to_str().unwrap()]);
    assert!(out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("geometry did not resolve (faces [], edges [1])"),
        "{err}"
    );
}
