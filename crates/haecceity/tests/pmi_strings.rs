//! Structural interoperability checks for bounded Part 21 text, without byte pins.
use haecceity::{
    p21::{self, Attribute as A, Document, Edit, RawEntity},
    pmi::{self, AttributeSet, AttributeValue, Material, Mode, PartId, PartPmi},
    step::read_part_definitions,
};
use std::{io::Read, path::Path};

fn fixture(path: &str) -> Document {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(path);
    let raw = std::fs::read(&path).unwrap();
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(raw.as_slice())
        .read_to_end(&mut bytes)
        .unwrap();
    Document::parse(bytes).unwrap()
}

/// An independent literal scanner: comments and binaries must not look like text.
fn longest_literal(bytes: &[u8]) -> usize {
    let (mut i, mut longest) = (0, 0);
    while i < bytes.len() {
        if bytes[i..].starts_with(b"/*") {
            i += 2 + bytes[i + 2..].windows(2).position(|s| s == b"*/").unwrap() + 2;
        } else if bytes[i] == b'"' {
            i += 1 + bytes[i + 1..].iter().position(|&c| c == b'"').unwrap() + 1;
        } else if bytes[i] == b'\'' {
            let start = i;
            loop {
                i += 1 + bytes[i + 1..].iter().position(|&c| c == b'\'').unwrap();
                i += 1;
                if bytes.get(i) != Some(&b'\'') {
                    break;
                }
            }
            longest = longest.max(i - start);
        } else {
            i += 1;
        }
    }
    longest
}

fn write(doc: &Document, pmi: &PartPmi) -> Document {
    let parts = read_part_definitions(doc.bytes()).unwrap();
    let (edit, _) = pmi::write(
        doc,
        &parts,
        &[(PartId(0), pmi.clone())],
        Mode::Replace,
        pmi::write::PresentationPolicy::RemovePresentation,
    )
    .unwrap();
    let bytes = doc.apply(&edit).unwrap().bytes;
    assert!(longest_literal(&bytes) <= pmi::write::MAX_STRING_LITERAL_BYTES);
    Document::parse(bytes).unwrap()
}

fn chunk_ids(doc: &Document) -> Vec<u64> {
    doc.ids()
        .filter(|&id| {
            matches!(doc.get(id), Some(RawEntity::Simple {name, attributes, ..})
        if name == "DESCRIPTIVE_REPRESENTATION_ITEM"
        && attributes.first() == Some(&A::String("haecceity text chunk/1".into())))
        })
        .collect()
}

#[test]
fn long_attribute_and_material_text_round_trip_and_replace_without_orphan_chunks() {
    let doc = fixture("ap242/specify/spool_fits.step.gz");
    let text = "'\\⌀é\n😀".repeat(1800);
    let material = "a".repeat(10000);
    let model = PartPmi {
        attributes: vec![AttributeSet {
            name: "payload".into(),
            on: None,
            items: vec![(
                "JSON or provenance".into(),
                AttributeValue::Text(text.clone()),
            )],
            description: None,
            general_property: None,
        }],
        material: Some(Material {
            id: "metal".into(),
            name: Some(material.clone()),
            density: None,
        }),
        ..PartPmi::default()
    };
    let after = write(&doc, &model);
    let chunks = chunk_ids(&after);
    assert!(chunks.len() > 5);
    let read = pmi::read(&after, &read_part_definitions(after.bytes()).unwrap()).unwrap();
    assert_eq!(
        read.parts[0].attributes[0].items[0].1,
        AttributeValue::Text(text)
    );
    assert_eq!(
        read.parts[0].material.as_ref().unwrap().name,
        Some(material)
    );
    let consumed = read.provenance.parts[0].all();
    assert!(chunks.iter().all(|id| consumed.contains(id)));
    let again = write(&after, &model);
    assert_eq!(
        chunk_ids(&again).len(),
        chunks.len(),
        "replace must remove old chunks"
    );
}

#[test]
fn generated_long_note_label_keeps_its_full_semantic_wording() {
    let doc = fixture("ap242/specify/spool_fits.step.gz");
    let text = "Break edges. ".repeat(700);
    let model = PartPmi {
        attributes: vec![AttributeSet {
            name: "semantic text".into(),
            on: None,
            items: vec![("edges".into(), AttributeValue::Text(text.clone()))],
            description: None,
            general_property: None,
        }],
        ..PartPmi::default()
    };
    let after = write(&doc, &model);
    let read = pmi::read(&after, &read_part_definitions(after.bytes()).unwrap()).unwrap();
    assert_eq!(
        read.parts[0].attributes[0].items[0].1,
        AttributeValue::Text(text)
    );
    assert!(!chunk_ids(&after).is_empty());
}

#[test]
fn indivisible_names_and_unchanged_source_strings_are_refused_explicitly() {
    let doc = fixture("ap242/specify/spool_fits.step.gz");
    let mut model = PartPmi {
        attributes: vec![AttributeSet {
            name: "x".repeat(pmi::write::MAX_STRING_LITERAL_BYTES),
            on: None,
            items: vec![],
            description: None,
            general_property: None,
        }],
        ..PartPmi::default()
    };
    let parts = read_part_definitions(doc.bytes()).unwrap();
    let error = pmi::write(
        &doc,
        &parts,
        &[(PartId(0), model.clone())],
        Mode::Replace,
        pmi::write::PresentationPolicy::RemovePresentation,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("indivisible string"), "{error}");
    model.attributes.clear();
    let mut edit = Edit::new();
    edit.add(p21::Record::new(p21::simple(
        "DESCRIPTIVE_REPRESENTATION_ITEM",
        vec![A::String("unattached".into()), A::String("y".repeat(10000))],
    )));
    let doc = Document::parse(doc.apply(&edit).unwrap().bytes).unwrap();
    let parts = read_part_definitions(doc.bytes()).unwrap();
    let error = pmi::write(
        &doc,
        &parts,
        &[(PartId(0), model)],
        Mode::Replace,
        pmi::write::PresentationPolicy::RemovePresentation,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("unchanged source string"), "{error}");
}

#[cfg(feature = "json-import")]
#[test]
fn stc10_ordering_map_is_split_and_legacy_long_map_still_reads() {
    use haecceity::json;
    let doc = fixture("ap242/nist/nist_stc_10_asme1_ap242-e2.stp.gz");
    let parts = read_part_definitions(doc.bytes()).unwrap();
    let read = pmi::read(&doc, &parts).unwrap();
    let requested: Vec<_> = read
        .parts
        .iter()
        .enumerate()
        .map(|(i, p)| {
            (
                PartId(i),
                json::identify(p.clone(), Some(&read.identifiers.parts[i])).unwrap(),
            )
        })
        .collect();
    let expected: Vec<_> = requested
        .iter()
        .map(|(_, s)| json::to_value(s).unwrap())
        .collect();
    let (bytes, _) = json::write(
        &doc,
        &parts,
        &requested,
        pmi::write::PresentationPolicy::RemovePresentation,
    )
    .unwrap();
    let longest = longest_literal(&bytes);
    assert!(longest <= pmi::write::MAX_STRING_LITERAL_BYTES);
    let after = Document::parse(bytes).unwrap();
    let (ordering, children) = after
        .ids()
        .find_map(|id| {
            let RawEntity::Simple {
                name, attributes, ..
            } = after.get(id)?
            else {
                return None;
            };
            if name != "COMPOUND_REPRESENTATION_ITEM"
                || attributes.first() != Some(&A::String("ordering".into()))
            {
                return None;
            }
            let A::Typed { value, .. } = attributes.get(1)? else {
                return None;
            };
            let A::List(children) = value.as_ref() else {
                return None;
            };
            Some((
                id,
                children
                    .iter()
                    .map(|a| match a {
                        A::EntityRef(id) => *id,
                        _ => panic!("chunk reference"),
                    })
                    .collect::<Vec<_>>(),
            ))
        })
        .expect("STC-10 must exercise the long map");
    assert!(children.len() > 2);
    // A SET is not ordered: corrupt transport must not silently lose aliases.
    let mut unordered = after.get(ordering).unwrap().clone();
    if let RawEntity::Simple { attributes, .. } = &mut unordered
        && let A::Typed { type_name, .. } = &mut attributes[1]
    {
        *type_name = "SET_REPRESENTATION_ITEM".into();
    }
    let mut bad = Edit::new();
    bad.replace(ordering, p21::Record::new(unordered));
    let bad = Document::parse(after.apply(&bad).unwrap().bytes).unwrap();
    let error = pmi::read(&bad, &read_part_definitions(bad.bytes()).unwrap())
        .unwrap_err()
        .to_string();
    assert!(error.contains("invalid PMI ordering text"), "{error}");
    let mut legacy_text = String::new();
    let mut edit = Edit::new();
    for id in children {
        let RawEntity::Simple { attributes, .. } = after.get(id).unwrap() else {
            panic!("chunk")
        };
        let A::String(text) = &attributes[1] else {
            panic!("chunk text")
        };
        legacy_text.push_str(text);
        edit.remove(id);
    }
    assert!(legacy_text.len() > 40000);
    edit.replace(
        ordering,
        p21::Record::new(p21::simple(
            "DESCRIPTIVE_REPRESENTATION_ITEM",
            vec![A::String("ordering".into()), A::String(legacy_text)],
        )),
    );
    let legacy = Document::parse(after.apply(&edit).unwrap().bytes).unwrap();
    assert!(longest_literal(legacy.bytes()) > 40000);
    for (name, candidate) in [("split", &after), ("legacy", &legacy)] {
        let definitions = read_part_definitions(candidate.bytes()).unwrap();
        let read = pmi::read(candidate, &definitions).unwrap();
        for (i, p) in read.parts.iter().enumerate() {
            assert_eq!(
                json::to_value(
                    &json::identify(p.clone(), Some(&read.identifiers.parts[i])).unwrap()
                )
                .unwrap(),
                expected[i],
                "{name}: whole JSON semantic identity"
            );
        }
        let (rewritten, _) = json::write(
            candidate,
            &definitions,
            &requested,
            pmi::write::PresentationPolicy::RemovePresentation,
        )
        .unwrap();
        assert!(longest_literal(&rewritten) <= pmi::write::MAX_STRING_LITERAL_BYTES);
    }
    eprintln!("STC-10 longest written literal: {longest} bytes");
}

#[test]
fn geometry_writer_refuses_an_oversized_source_product_name() {
    let doc = fixture("ap242/specify/spool_fits.step.gz");
    let (id, mut entity) = doc
        .ids()
        .find_map(|id| match doc.get(id) {
            Some(e @ RawEntity::Simple { name, .. }) if name == "PRODUCT" => Some((id, e.clone())),
            _ => None,
        })
        .unwrap();
    if let RawEntity::Simple { attributes, .. } = &mut entity {
        attributes[1] = A::String("p".repeat(pmi::write::MAX_STRING_LITERAL_BYTES));
    }
    let mut edit = Edit::new();
    edit.replace(id, p21::Record::new(entity));
    let doc = Document::parse(doc.apply(&edit).unwrap().bytes).unwrap();
    let parts = read_part_definitions(doc.bytes()).unwrap();
    let part = haecceity::step::read_part(doc.bytes(), &parts[0]).unwrap();
    let error = haecceity::brep_write::write_step(
        &part,
        &haecceity::brep_write::WriteOptions {
            name: "fallback",
            pmi: None,
        },
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains(&format!(
            "encoded bytes (limit {})",
            pmi::write::MAX_STRING_LITERAL_BYTES
        )),
        "{error}"
    );
}
