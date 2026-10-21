//! Rust semantic PMI baseline on every former draftwright-oracle input. NIST, intent,
//! OpenCascade, analytic writer and round-trip tests remain in haecceity.
#[path = "common/references.rs"]
mod common;
use quiddity::{
    kernel::{p21::Document, pmi, step::read_part_definitions},
    pmi_json,
};
use serde_json::{Value, json};
#[test]
fn rust_snapshot_pmi() {
    let inputs = common::load("pmi_inputs.json");
    let actual: Vec<Value> = inputs["files"].as_array().unwrap().iter().map(|name| {
        let name = name.as_str().unwrap();
        let path = common::fixtures().join(name);
        let bytes = std::fs::read(&path).unwrap();
        let bytes = if path.extension().is_some_and(|e|e=="gz") {
            use std::io::Read;
            let mut out = Vec::new();flate2::read::GzDecoder::new(bytes.as_slice()).read_to_end(&mut out).unwrap();out
        } else {bytes};
        let parts = read_part_definitions(&bytes).unwrap();
        let doc = Document::parse(bytes.clone()).unwrap();
        let read = pmi::read(&doc,&parts).unwrap();
        let interchange = pmi_json::read_document(&read,&parts,pmi_json::Binding::of(&bytes),None);
        json!({"file":name,"pmi":pmi_json::document_to_json(&interchange),"parts":pmi_json::parts_json(&parts,&pmi_json::Binding::of(&bytes))})
    }).collect();
    assert!(!actual.is_empty());
    common::snapshot::check_with(&common::fixtures(), "pmi", &json!(actual), &|a, b| {
        common::snapshot::close(a, b, 1e-9)
    });
}
