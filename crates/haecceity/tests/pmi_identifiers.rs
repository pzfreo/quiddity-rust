//! Requirement identifiers are source metadata, independent of the semantic model.

mod common;

use std::io::Read;

use haecceity::p21::Document;
use haecceity::pmi::{self, FindingKind, PartId, PartPmi, PmiRead};
use haecceity::step::read_part_definitions;

fn plate() -> String {
    let bytes =
        std::fs::read(common::fixtures().join("ap242/identifiers/specify-0.1.4-plate.step.gz"))
            .unwrap();
    let mut text = String::new();
    flate2::read::GzDecoder::new(bytes.as_slice())
        .read_to_string(&mut text)
        .unwrap();
    text
}

fn read(text: &str) -> (Document, PmiRead) {
    let parts = read_part_definitions(text.as_bytes()).unwrap();
    let doc = Document::parse(text.as_bytes().to_vec()).unwrap();
    let result = pmi::read(&doc, &parts).unwrap();
    (doc, result)
}

fn appended(text: &str, records: &str) -> String {
    let at = text.rfind("ENDSEC;").unwrap();
    format!("{}{records}\n{}", &text[..at], &text[at..])
}

// Pinned from the Specify 0.1.4 file's ID_ATTRIBUTE records, in semantic model order.
const DIMENSIONS: &[(u64, &str)] = &[
    (2167, "15"),
    (2173, "17"),
    (2178, "18"),
    (2183, "16"),
    (2189, "9"),
    (2194, "10"),
    (2199, "25"),
    (2204, "26"),
    (2209, "27"),
    (2214, "28"),
    (2219, "11"),
    (2224, "12"),
    (2229, "13"),
    (2234, "14"),
    (2239, "23"),
    (2244, "24"),
    (2249, "8"),
    (2253, "6"),
    (2257, "5"),
    (2261, "33"),
    (2265, "30"),
    (2269, "29"),
    (2273, "32"),
    (2277, "31"),
    (2281, "7"),
    (2285, "4"),
];

fn check(values: &[Option<String>], provenance: &[Vec<u64>], expected: &[(u64, &str)]) {
    assert_eq!(values.len(), expected.len());
    assert_eq!(provenance.len(), expected.len());
    for ((value, prov), &(entity, number)) in values.iter().zip(provenance).zip(expected) {
        assert_eq!(value.as_deref(), Some(number), "#{entity}");
        assert!(prov.contains(&entity), "#{entity}: {prov:?}");
    }
}

#[test]
fn specify_plate_reads_every_numbered_requirement() {
    let (doc, r) = read(&plate());
    assert_eq!(r.parts.len(), 1);
    let p = &r.parts[0];
    let i = &r.identifiers.parts[0];
    let prov = &r.provenance.parts[0];
    assert_eq!(p.dimensions.len(), 26);
    assert_eq!(p.datums.len(), 3);
    assert_eq!(p.tolerances.len(), 3);
    assert_eq!(p.surface_textures.len(), 1);
    assert_eq!(p.threads.len(), 1);
    assert!(p.knurls.is_empty());
    check(&i.dimensions, &prov.dimensions, DIMENSIONS);
    check(
        &i.datums,
        &prov.datums,
        &[(2116, "1"), (2118, "2"), (2120, "3")],
    );
    check(
        &i.tolerances,
        &prov.tolerances,
        &[(2294, "19"), (2298, "20"), (2302, "21")],
    );
    check(&i.surface_textures, &prov.surface_textures, &[(2363, "34")]);
    check(&i.threads, &prov.threads, &[(2310, "22")]);
    assert!(p.surface_textures[0].on.is_none(), "part default");
    assert_eq!(p.datums[0].label().as_str(), "A");
    for (index, attributes) in [
        (0, &prov.datums),
        (1, &prov.dimensions),
        (2, &prov.tolerances),
        (3, &prov.surface_textures),
        (4, &prov.threads),
    ] {
        for item in attributes {
            assert!(item.iter().any(|&id| matches!(doc.get(id), Some(haecceity::p21::RawEntity::Simple {name, ..}) if name.eq_ignore_ascii_case("id_attribute"))), "kind {index}: {item:?}");
        }
    }
    assert!(r.findings.is_empty(), "{:?}", r.findings);
    assert_eq!(i.features.len(), p.features.len());
    assert_eq!(i.datum_targets.len(), p.datum_targets.len());
    assert_eq!(i.tolerance_relations.len(), p.tolerance_relations.len());
    assert_eq!(i.general.len(), p.general.len());
    assert_eq!(i.knurls.len(), p.knurls.len());
    assert_eq!(i.notes.len(), p.notes.len());
    assert_eq!(i.attributes.len(), p.attributes.len());
}

#[test]
fn identifiers_are_optional_text_and_do_not_change_the_model() {
    let text = plate();
    let (_, numbered) = read(&text);
    let unnumbered = text
        .lines()
        .filter(|line| !line.contains("=ID_ATTRIBUTE("))
        .collect::<Vec<_>>()
        .join("\n");
    let (_, plain) = read(&unnumbered);
    assert_eq!(numbered.parts, plain.parts);
    let i = &plain.identifiers.parts[0];
    for values in [
        &i.features,
        &i.datums,
        &i.dimensions,
        &i.tolerances,
        &i.surface_textures,
        &i.threads,
    ] {
        assert!(values.iter().all(Option::is_none));
    }
    let (_, textual) =
        read(&text.replace("ID_ATTRIBUTE('15',#2167)", "ID_ATTRIBUTE('00-A''B',#2167)"));
    assert_eq!(
        textual.identifiers.parts[0].dimensions[0].as_deref(),
        Some("00-A'B")
    );
    assert_ne!(textual, numbered);
    assert_eq!(textual.parts, numbered.parts);
}

#[test]
fn simple_tolerances_and_empty_identifiers_are_read() {
    let text = plate();
    let line = text
        .lines()
        .find(|line| line.starts_with("#2294="))
        .unwrap();
    let text = text
        .replace(line, "#2294=POSITION_TOLERANCE('','',#2293,#2069);")
        .replace("ID_ATTRIBUTE('19',#2294)", "ID_ATTRIBUTE('',#2294)");
    let (_, r) = read(&text);
    assert_eq!(r.parts[0].tolerances.len(), 3);
    assert_eq!(r.identifiers.parts[0].tolerances[0].as_deref(), Some(""));
}

#[test]
fn parameter_and_part_ids_do_not_supply_missing_manufacturing_ids() {
    let text = plate()
        .replace("#2440=ID_ATTRIBUTE('34',#2363);", "")
        .replace("#2441=ID_ATTRIBUTE('22',#2310);", "");
    let (_, r) = read(&appended(
        &text,
        "#3000=ID_ATTRIBUTE('parameter',#2369);\n#3001=ID_ATTRIBUTE('part',#4);",
    ));
    assert_eq!(r.identifiers.parts[0].surface_textures, [None]);
    assert_eq!(r.identifiers.parts[0].threads, [None]);
}

#[test]
fn duplicates_are_findings_even_when_the_values_agree() {
    let text = plate();
    for value in ["15", "different"] {
        let (_, r) = read(&appended(
            &text,
            &format!("#3000=ID_ATTRIBUTE('{value}',#2167);"),
        ));
        assert_eq!(r.identifiers.parts[0].dimensions[0], None);
        let f = r
            .findings
            .iter()
            .find(|f| f.ids == [2167, 2411, 3000])
            .unwrap();
        assert_eq!(f.kind, FindingKind::Nonconformance);
        assert!(f.detail.contains("more than one id_attribute"));
        assert!(r.provenance.parts[0].dimensions[0].contains(&3000));
        assert_eq!(r.identifiers.parts[0].dimensions[1].as_deref(), Some("17"));
    }
    let (_, r) = read(&text.replace("ID_ATTRIBUTE('15',#2167)", "ID_ATTRIBUTE($,#2167)"));
    assert_eq!(r.identifiers.parts[0].dimensions[0], None);
    assert!(
        r.findings
            .iter()
            .any(|f| f.ids == [2167, 2411] && f.kind == FindingKind::Nonconformance)
    );
}

#[test]
fn shared_support_ids_are_not_requirement_ids() {
    let text = plate();
    // #2069 is the shared feature of the first size/location and position requirements.
    // #2063 is datum A's DATUM_FEATURE, distinct from its DATUM #2116.
    let text = text
        .replace("#2411=ID_ATTRIBUTE('15',#2167);", "")
        .replace("#2408=ID_ATTRIBUTE('1',#2116);", "");
    let (_, r) = read(&appended(
        &text,
        "#3000=ID_ATTRIBUTE('support',#2069);\n#3001=ID_ATTRIBUTE('datum-feature',#2063);\n#3002=ID_ATTRIBUTE('part',#4);",
    ));
    assert_eq!(r.identifiers.parts[0].dimensions[0], None);
    assert_eq!(r.identifiers.parts[0].datums[0], None);
    assert_eq!(r.identifiers.parts[0].dimensions[1].as_deref(), Some("17"));
    assert_eq!(r.identifiers.parts[0].tolerances[0].as_deref(), Some("19"));
    assert_eq!(
        r.identifiers.parts[0].surface_textures[0].as_deref(),
        Some("34")
    );
    assert!(
        r.identifiers.parts[0]
            .features
            .iter()
            .any(|id| id.as_deref() == Some("support"))
    );
}

#[test]
fn property_and_relationship_items_use_their_own_carriers() {
    let (_, r) = read(&appended(
        &plate(),
        "#3000=PROPERTY_DEFINITION('manufacturing requirement','note',#4);
#3001=DESCRIPTIVE_REPRESENTATION_ITEM('note','inspection note');
#3002=REPRESENTATION('',(#3001),#2057);
#3003=PROPERTY_DEFINITION_REPRESENTATION(#3000,#3002);
#3004=ID_ATTRIBUTE('note-id',#3000);
#3005=ID_ATTRIBUTE('uda-id',#2346);
#3006=GEOMETRIC_TOLERANCE_RELATIONSHIP('simultaneity','',#2294,#2298);
#3007=ID_ATTRIBUTE('relationship-id',#3006);",
    ));
    let i = &r.identifiers.parts[0];
    assert_eq!(i.notes, [Some("note-id".into())]);
    assert_eq!(i.tolerance_relations, [Some("relationship-id".into())]);
    let index = r.parts[0]
        .attributes
        .iter()
        .position(|a| a.name == "semantic text")
        .unwrap();
    assert_eq!(i.attributes[index].as_deref(), Some("uda-id"));
}

#[test]
fn datum_targets_use_their_own_shape_aspect() {
    let (_, r) = read(&appended(
        &plate(),
        "#3000=DATUM_TARGET('','area',#4,.T.,'1');
#3001=GEOMETRIC_ITEM_SPECIFIC_USAGE('','',#3000,#10,#213);
#3002=DATUM('','',#4,.F.,'D');
#3003=SHAPE_ASPECT_RELATIONSHIP('','',#3000,#3002);
#3004=ID_ATTRIBUTE('target-id',#3000);",
    ));
    assert_eq!(r.parts[0].datum_targets.len(), 1, "{:?}", r.findings);
    assert_eq!(
        r.identifiers.parts[0].datum_targets,
        [Some("target-id".into())]
    );
    assert_eq!(r.identifiers.parts[0].datums[3], None);
}

#[test]
fn merged_datums_do_not_select_one_requirement_identifier() {
    let (_, r) = read(&appended(
        &plate(),
        "#3000=DATUM('','',#4,.F.,'A');
#3001=SHAPE_ASPECT_RELATIONSHIP('','',#2063,#3000);
#3002=ID_ATTRIBUTE('other-A',#3000);",
    ));
    assert_eq!(r.parts[0].datums.len(), 3);
    assert_eq!(r.identifiers.parts[0].datums[0], None);
    assert!(
        r.findings
            .iter()
            .any(|f| f.detail.contains("merged datum") && f.ids == [3000, 2116])
    );
}

#[test]
fn writer_append_preserves_identifiers_and_their_original_records() {
    let text = plate();
    let (doc, before) = read(&text);
    let parts = read_part_definitions(doc.bytes()).unwrap();
    let extra = PartPmi {
        attributes: vec![pmi::AttributeSet {
            name: "append test".into(),
            on: None,
            items: vec![(
                "value".into(),
                pmi::AttributeValue::Text("new metadata".into()),
            )],
            description: None,
            general_property: None,
        }],
        ..PartPmi::default()
    };
    let (edit, _) = pmi::write(
        &doc,
        &parts,
        &[(PartId(0), extra)],
        pmi::Mode::Add,
        pmi::write::PresentationPolicy::Refuse,
    )
    .unwrap();
    let bytes = doc.apply(&edit).unwrap().bytes;
    let (after_doc, after) = read(std::str::from_utf8(&bytes).unwrap());
    let mut expected = before.identifiers;
    expected.parts[0].attributes.push(None);
    assert_eq!(after.identifiers, expected);
    for id in doc.ids() {
        assert_eq!(
            &after_doc.bytes()[after_doc.span(id).unwrap()],
            &doc.bytes()[doc.span(id).unwrap()],
            "original #{id}"
        );
    }
}
