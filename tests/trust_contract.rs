//! Additive source identity and completed-run provenance, without changing feature payloads.
mod common;

use quiddity::correspondence;
use quiddity::kernel::step::{IDENTITY, read_step_file};
use quiddity::recognition::{self, RunMode};
use quiddity::serve;
use sha2::{Digest, Sha256};

#[test]
fn source_digest_is_of_the_exact_input_bytes_in_cli_and_serve() {
    use std::io::Write;
    use std::process::Command;
    let path = common::fixtures().join("rejected_cylinder.step");
    let bytes = std::fs::read(&path).unwrap();
    let expected = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let (output, _) = serve::recognise_file(path.to_str().unwrap()).unwrap();
    assert_eq!(
        output.document.source_sha256.as_deref(),
        Some(expected.as_str())
    );
    assert_eq!(output.document.run_mode, RunMode::Strict);
    let cli = Command::new(env!("CARGO_BIN_EXE_quiddity"))
        .arg(&path)
        .output()
        .unwrap();
    assert!(cli.status.success());
    let cli: serde_json::Value = serde_json::from_slice(&cli.stdout).unwrap();
    let response =
        serve::respond(&serde_json::json!({"id":1,"op":"recognise","step":path}).to_string())
            .unwrap();
    let response: serde_json::Value = serde_json::from_str(&response).unwrap();
    assert_eq!(cli, response["result"]);
    assert_eq!(cli["document"]["source_sha256"], expected);

    let gz_path =
        std::env::temp_dir().join(format!("quiddity-trust-{}.step.gz", std::process::id()));
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&bytes).unwrap();
    let compressed = encoder.finish().unwrap();
    std::fs::write(&gz_path, &compressed).unwrap();
    let (gz, _) = serve::recognise_file(gz_path.to_str().unwrap()).unwrap();
    std::fs::remove_file(gz_path).unwrap();
    assert_eq!(
        gz.document.source_sha256,
        Some(
            Sha256::digest(&compressed)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        )
    );
    assert_ne!(gz.document.source_sha256, output.document.source_sha256);
    assert_eq!(gz.document.records, output.document.records);
}

#[test]
fn in_memory_geometry_has_no_invented_source_digest() {
    let part = read_step_file(&common::fixtures().join("rejected_cylinder.step")).unwrap();
    let rec = correspondence::recognise(&part);
    let doc = recognition::document(&rec, part.faces.len());
    assert_eq!(doc.source_sha256, None);
    assert_eq!(doc.run_mode, RunMode::Strict);
    let mut old = serde_json::to_value(doc).unwrap();
    let old = old.as_object_mut().unwrap();
    old.remove("source_sha256");
    old.remove("run_mode");
    let old: recognition::RecognitionDocument =
        serde_json::from_value(serde_json::json!(old)).unwrap();
    assert_eq!(old.source_sha256, None);
    assert_eq!(old.run_mode, RunMode::Unknown);
}

#[test]
fn retry_mode_survives_frame_mapping_and_thread_local_restoration() {
    let Some(dir) = common::corpus_dir() else {
        assert!(
            std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
            "corpus required"
        );
        return;
    };
    for name in ["13975", "14052"] {
        let path = dir.join(format!("inventory_refusal/{name}.step.gz"));
        let part = read_step_file(&path).unwrap();
        let rec = correspondence::recognise(&part);
        assert_eq!(
            recognition::document(&rec, part.faces.len()).run_mode,
            RunMode::LocallyDegraded,
            "{name}"
        );
        let (part, rec, _) = serve::recognise_step(path.to_str().unwrap(), &IDENTITY).unwrap();
        assert_eq!(
            recognition::document(&rec, part.faces.len()).run_mode,
            RunMode::LocallyDegraded,
            "{name}"
        );
        assert!(!quiddity::features::evidence::local_degradation());
    }
    let (sound, _) = serve::recognise_file(
        common::fixtures()
            .join("rejected_cylinder.step")
            .to_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(sound.document.run_mode, RunMode::Strict);
}
