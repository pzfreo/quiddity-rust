//! Serializer-only builds retain the exact pre-split wire form, without schemas/import.
#![cfg(feature = "json")]
use haecceity::{json, p21::Document, pmi, step::read_part_definitions};
use std::io::Read;
fn unzip(raw: &[u8]) -> Vec<u8> {
    let mut bytes = vec![];
    flate2::read::GzDecoder::new(raw)
        .read_to_end(&mut bytes)
        .unwrap();
    bytes
}
#[test]
fn export_matches_the_pre_split_cli_with_index_anchors_and_identical_ids() {
    let doc = Document::parse(unzip(include_bytes!(
        "fixtures/presentation/spindle.step.gz"
    )))
    .unwrap();
    let parts = read_part_definitions(doc.bytes()).unwrap();
    let read = pmi::read(&doc, &parts).unwrap();
    let semantic = json::identify(read.parts[0].clone(), Some(&read.identifiers.parts[0])).unwrap();
    let expected: serde_json::Value = serde_json::from_slice(&unzip(include_bytes!(
        "fixtures/presentation/spindle.semantic.json.gz"
    )))
    .unwrap();
    assert_eq!(json::to_value(&semantic).unwrap(), expected);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&json::to_string(&semantic).unwrap()).unwrap(),
        expected
    );
    let drawn = json::presentation::read(&doc, &read, pmi::PartId(0), &semantic).unwrap();
    assert_eq!(drawn.planes.len(), 14);
    assert!(drawn.items.values().flatten().all(|g| g.plane_id.is_some()));
    assert_eq!(
        drawn
            .views
            .iter()
            .find(|v| v.name == "PMI callouts")
            .unwrap()
            .graphics
            .len(),
        14
    );
}
