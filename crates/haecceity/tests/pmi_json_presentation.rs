#![cfg(feature = "json")]
use haecceity::{
    json::{
        self,
        presentation::{self, Source},
    },
    p21::{self, Attribute as A, Document},
    pmi::{self, PartId},
    step::read_part_definitions,
};
use std::io::Read;
fn ctc() -> Vec<u8> {
    let raw = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/ap242/nist/nist_ctc_01_asme1_ap242-e1.stp.gz"
    ))
    .unwrap();
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(raw.as_slice())
        .read_to_end(&mut bytes)
        .unwrap();
    bytes
}
#[test]
fn ctc_01_as_drawn_and_unlinked_graphics_survive() {
    let doc = Document::parse(ctc()).unwrap();
    // A callout with an explicit unattached polyline, carrying no semantic association.
    let mut edit = p21::Edit::new();
    let p = edit.add(p21::simple(
        "CARTESIAN_POINT",
        vec![
            A::String("".into()),
            A::List(vec![A::Real(1.), A::Real(2.), A::Real(3.)]),
        ],
    ));
    let q = edit.add(p21::simple(
        "CARTESIAN_POINT",
        vec![
            A::String("".into()),
            A::List(vec![A::Real(4.), A::Real(5.), A::Real(6.)]),
        ],
    ));
    let line = edit.add(p21::simple(
        "POLYLINE",
        vec![
            A::String("orphan".into()),
            A::List(vec![p.to_ref(), q.to_ref()]),
        ],
    ));
    let ann = edit.add(p21::simple(
        "ANNOTATION_CURVE_OCCURRENCE",
        vec![A::String("orphan".into()), A::List(vec![]), line.to_ref()],
    ));
    let orphan = edit.add(p21::simple(
        "DRAUGHTING_CALLOUT",
        vec![
            A::String("unlinked witness".into()),
            A::List(vec![ann.to_ref()]),
        ],
    ));
    let context = doc
        .ids()
        .find_map(|id| match doc.get(id) {
            Some(p21::RawEntity::Simple {
                name, attributes, ..
            }) if name.eq_ignore_ascii_case("draughting_model") => attributes.get(2).cloned(),
            _ => None,
        })
        .unwrap();
    edit.add(p21::simple(
        "DRAUGHTING_MODEL",
        vec![
            A::String("unlinked graphics".into()),
            A::List(vec![orphan.to_ref()]),
            context,
        ],
    ));
    let bytes = doc.apply(&edit).unwrap().bytes;
    let parts = read_part_definitions(&bytes).unwrap();
    let doc = Document::parse(bytes).unwrap();
    let read = pmi::read(&doc, &parts).unwrap();
    let semantic = json::identify(read.parts[0].clone(), Some(&read.identifiers.parts[0])).unwrap();
    let drawn = presentation::read(&doc, &read, PartId(0), &semantic).unwrap();
    assert!(!drawn.items.is_empty());
    assert!(
        drawn
            .items
            .keys()
            .all(|id| semantic.ids.values().flatten().any(|known| known == id))
    );
    let graphics: Vec<_> = drawn
        .items
        .values()
        .flatten()
        .chain(&drawn.unlinked)
        .collect();
    assert!(graphics.iter().all(|g| g.source == Source::File));
    assert!(graphics.iter().any(|g| !g.polylines.is_empty()));
    assert!(graphics.iter().any(|g| !g.triangles.is_empty()));
    let orphan = drawn
        .unlinked
        .iter()
        .find(|g| g.name == "unlinked witness")
        .unwrap();
    assert!(!orphan.step_ids.is_empty());
    assert_eq!(orphan.polylines, vec![vec![[1., 2., 3.], [4., 5., 6.]]]);
    let decoded: presentation::Presentation =
        serde_json::from_value(serde_json::to_value(&drawn).unwrap()).unwrap();
    assert_eq!(decoded, drawn);
    eprintln!(
        "CTC-01: {} linked graphics, {} unlinked; {} polylines, {} triangles",
        drawn.items.values().map(Vec::len).sum::<usize>(),
        drawn.unlinked.len(),
        graphics.iter().map(|g| g.polylines.len()).sum::<usize>(),
        graphics.iter().map(|g| g.triangles.len()).sum::<usize>()
    );
}
#[cfg(feature = "presentation")]
#[test]
fn generated_notation_has_semantic_ids_and_source() {
    let bytes = ctc();
    let doc = Document::parse(bytes.clone()).unwrap();
    let parts = read_part_definitions(&bytes).unwrap();
    let read = pmi::read(&doc, &parts).unwrap();
    let semantic = json::identify(read.parts[0].clone(), Some(&read.identifiers.parts[0])).unwrap();
    let generated = presentation::generated(
        &semantic,
        &haecceity::step::read_part(&bytes, &parts[0]).unwrap(),
    )
    .unwrap();
    assert!(!generated.items.is_empty());
    assert!(
        generated
            .items
            .values()
            .flatten()
            .all(|g| g.source == Source::Generated)
    );
    presentation::validate(&serde_json::to_value(generated).unwrap()).unwrap();
}

#[test]
fn malformed_coordinates_and_indices_never_renumber_drawn_points() {
    let doc = Document::parse(ctc()).unwrap();
    let mut edit = p21::Edit::new();
    let coords = edit.add(p21::simple(
        "COORDINATES_LIST",
        vec![
            A::String("bad unused row".into()),
            A::Integer(3),
            A::List(vec![
                A::List(vec![A::Real(1.), A::Real(2.)]),
                A::List(vec![A::Real(3.), A::Real(4.), A::Real(5.)]),
                A::List(vec![A::Real(6.), A::Real(7.), A::Real(8.)]),
            ]),
        ],
    ));
    let curve = edit.add(p21::simple(
        "TESSELLATED_CURVE_SET",
        vec![
            A::String("stable indices".into()),
            coords.to_ref(),
            A::List(vec![
                A::List(vec![A::Integer(2), A::Integer(3)]),
                A::List(vec![A::Integer(0), A::Integer(2), A::Integer(3)]),
            ]),
        ],
    ));
    let occurrence = edit.add(p21::simple(
        "TESSELLATED_ANNOTATION_OCCURRENCE",
        vec![
            A::String("stable indices".into()),
            A::List(vec![]),
            curve.to_ref(),
        ],
    ));
    let root = edit.add(p21::simple(
        "DRAUGHTING_CALLOUT",
        vec![
            A::String("bad-source-index witness".into()),
            A::List(vec![occurrence.to_ref()]),
        ],
    ));
    let context = doc
        .ids()
        .find_map(|id| match doc.get(id) {
            Some(p21::RawEntity::Simple {
                name, attributes, ..
            }) if name.eq_ignore_ascii_case("draughting_model") => attributes.get(2).cloned(),
            _ => None,
        })
        .unwrap();
    edit.add(p21::simple(
        "DRAUGHTING_MODEL",
        vec![
            A::String("source index witness".into()),
            A::List(vec![root.to_ref()]),
            context,
        ],
    ));
    let bytes = doc.apply(&edit).unwrap().bytes;
    let parts = read_part_definitions(&bytes).unwrap();
    let doc = Document::parse(bytes).unwrap();
    let read = pmi::read(&doc, &parts).unwrap();
    let semantic = json::identify(read.parts[0].clone(), Some(&read.identifiers.parts[0])).unwrap();
    let presentation = presentation::read(&doc, &read, PartId(0), &semantic).unwrap();
    let graphic = presentation
        .unlinked
        .iter()
        .find(|g| g.name == "bad-source-index witness")
        .unwrap();
    assert_eq!(graphic.polylines, vec![vec![[3., 4., 5.], [6., 7., 8.]]]);
    assert!(
        graphic
            .unsupported
            .iter()
            .any(|why| why.contains("index out of range"))
    );
}
