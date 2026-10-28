//! Format dispatch and the Fusion → AP242 PMI workflow; no private designs are fixtures.

use std::path::PathBuf;
use std::process::{Command, Output};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("crates/haecceity/tests/fixtures/generated_unit_cube.f3d")
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_quiddity"))
        .args(args)
        .output()
        .unwrap()
}

#[cfg(not(feature = "f3d"))]
#[test]
fn every_part_command_explains_the_disabled_feature() {
    let path = fixture();
    let path = path.to_str().unwrap();
    for args in [
        vec![path],
        vec!["parts", path],
        vec!["correspond", path, path],
        vec!["brep", path, "-o", "unused.step"],
        vec!["pmi", "read", path],
        vec!["pmi", "write", path, "unused.json", "-o", "unused.step"],
    ] {
        let out = run(&args);
        assert!(!out.status.success(), "{args:?}");
        let error = String::from_utf8(out.stderr).unwrap();
        assert!(
            error.contains("cargo install quiddity --features f3d"),
            "{error}"
        );
    }
    let reply = service(path);
    assert!(
        reply["error"]["message"]
            .as_str()
            .unwrap()
            .contains("--features f3d"),
        "{reply}"
    );
}

fn service(path: &str) -> serde_json::Value {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new(env!("CARGO_BIN_EXE_quiddity"))
        .arg("serve")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(
        child.stdin.take().unwrap(),
        "{}",
        serde_json::json!({"id": 1, "op": "recognise", "step": path})
    )
    .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    serde_json::from_slice(&out.stdout).unwrap()
}

#[cfg(feature = "f3d")]
#[test]
fn cube_recognition_parts_correspond_service_and_pmi_export() {
    use quiddity::kernel::pmi::{Note, PartPmi};
    use quiddity::pmi_json;
    use serde_json::Value;
    use sha2::{Digest, Sha256};

    fn ok(args: &[&str]) -> Value {
        let out = run(args);
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stderr = String::from_utf8(out.stderr).unwrap();
        assert_eq!(
            stderr.matches("converted from Fusion geometry").count(),
            if args[0] == "correspond" { 2 } else { 1 }
        );
        serde_json::from_slice(&out.stdout).unwrap()
    }

    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("f3d_cli");
    std::fs::create_dir_all(&dir).unwrap();
    let upper = dir.join("cube.F3D");
    std::fs::copy(fixture(), &upper).unwrap();
    let path = upper.to_str().unwrap();
    let raw = std::fs::read(&upper).unwrap();
    let hash = Sha256::digest(&raw)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let recognition = ok(&[path]);
    assert_eq!(recognition["document"]["face_count"], 6);
    assert_eq!(recognition["document"]["source_sha256"], hash);
    assert!(
        recognition["document"]["records"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let parts = ok(&["parts", path]);
    assert_eq!(parts["binding"]["sha256"], hash);
    ok(&["correspond", path, path]);
    let reply = service(path);
    assert_eq!(reply["result"], recognition);
    assert!(reply["warnings"].as_array().unwrap().iter().any(|w| {
        w.as_str()
            .unwrap()
            .contains("converted from Fusion geometry")
    }));

    let empty = ok(&["pmi", "read", path]);
    let mut pmi = pmi_json::document_from_json(&empty).unwrap();
    assert_eq!(pmi.parts.len(), 1);
    assert_eq!(pmi.binding.sha256, hash);
    assert_eq!(pmi.parts[0].pmi, PartPmi::default());
    pmi.parts[0].pmi.notes.push(Note {
        kind: "manufacturing requirement".into(),
        text: "Deburr all edges".into(),
        on: None,
        callouts: vec![],
    });
    let json_path = dir.join("cube-pmi.json");
    std::fs::write(
        &json_path,
        serde_json::to_vec(&pmi_json::document_to_json(&pmi)).unwrap(),
    )
    .unwrap();
    let json_path = json_path.to_str().unwrap();
    ok(&["pmi", "check", path, json_path]);
    let step_path = dir.join("cube.step");
    let step_path = step_path.to_str().unwrap();
    let brep = ok(&["brep", path, "-o", step_path, "--pmi", json_path]);
    assert_eq!(brep["faces"], 6);
    assert_eq!(brep["pmi"]["carried_items"], 1);
    let read = run(&["pmi", "read", step_path]);
    assert!(
        read.status.success(),
        "{}",
        String::from_utf8_lossy(&read.stderr)
    );
    let read: Value = serde_json::from_slice(&read.stdout).unwrap();
    let read = pmi_json::document_from_json(&read).unwrap();
    // The AP242 writer migrates legacy notes to standard semantic-text attributes.
    assert!(
        pmi_json::document_to_json(&read)
            .to_string()
            .contains("Deburr all edges")
    );
    assert!(read.findings.is_empty());
    let out = run(&["pmi", "write", path, json_path, "-o", step_path]);
    assert!(!out.status.success());
    assert!(
        String::from_utf8(out.stderr)
            .unwrap()
            .contains("requires original STEP text")
    );
}
