#![cfg(feature = "json")]
use haecceity::{
    json::{self, Anchors},
    p21::Document,
    pmi::{self, *},
    step::read_part_definitions,
};
use serde_json::json;
fn cube() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/ties/cube.step"
    ))
    .unwrap()
}
fn test_model() -> PartPmi {
    PartPmi {
        features: vec![Feature::Items(vec![Anchor::Face(FaceIndex(0))])],
        datums: vec![
            Datum::new(DatumLabel::new("A").unwrap(), Some(FeatureId(0)), vec![]).unwrap(),
        ],
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
        ..PartPmi::default()
    }
}
fn anchors() -> Anchors {
    Anchors {
        names: vec![Some("top".into())],
        fingerprints: vec![Some("support-key".into())],
        edge_count: 0,
    }
}
#[test]
fn all_anchor_forms_and_precise_units() {
    let p = test_model();
    let semantic = json::identify(p.clone(), None).unwrap();
    let v = json::to_value(&semantic).unwrap();
    assert_eq!(v["units"]["length"], json!(["millimetre"]));
    for face in [
        json!(0),
        json!({"name":"top"}),
        json!({"fingerprint":"support-key"}),
    ] {
        let mut v = v.clone();
        v["pmi"]["features"][0]["value"]["items"][0]["face"] = face;
        let decoded = json::from_value(&v, &anchors()).unwrap();
        assert_eq!(p, decoded.pmi);
        assert_eq!(
            v["pmi"]["dimensions"][0]["value"]["nominal"]["quantity"]["length"]["value"],
            "10.000"
        );
    }
}
#[test]
fn refusals_are_explained() {
    let base = json::to_value(&json::identify(test_model(), None).unwrap()).unwrap();
    for (path, value, reason) in [
        ("/schema", json!("haecceity.pmi/2"), "schema violation"),
        (
            "/pmi/features/0/value/items/0/face",
            json!({"name":"absent"}),
            "unknown face name",
        ),
        (
            "/pmi/features/0/value/items/0/face",
            json!(1),
            "unknown face index",
        ),
        ("/pmi/datums/0/id", json!("features/0"), "duplicate item id"),
        (
            "/pmi/dimensions/0/value/nominal/quantity/length/value",
            json!(1.25),
            "schema violation",
        ),
        (
            "/pmi/dimensions/0/value/nominal/quantity/length/unit",
            json!(7),
            "undefined length unit",
        ),
    ] {
        let mut v = base.clone();
        *v.pointer_mut(path).unwrap() = value;
        let e = json::from_value(&v, &anchors()).unwrap_err().to_string();
        assert!(e.contains(reason), "{e}");
    }
    let mut v = base.clone();
    v["pmi"]["bogus"] = json!(true);
    assert!(
        json::validate_schema(&v)
            .unwrap_err()
            .to_string()
            .contains("unknown property")
    );
}
#[test]
fn caller_ids_survive_step_write() {
    let bytes = cube();
    let doc = Document::parse(bytes.clone()).unwrap();
    let parts = read_part_definitions(&bytes).unwrap();
    let mut p = json::identify(test_model(), None).unwrap();
    p.ids.get_mut("dimensions").unwrap()[0] = "inspection-26".into();
    let (written, _) = json::write(
        &doc,
        &parts,
        &[(PartId(0), p)],
        pmi::write::PresentationPolicy::RemovePresentation,
    )
    .unwrap();
    let doc = Document::parse(written.clone()).unwrap();
    let parts = read_part_definitions(&written).unwrap();
    let read = pmi::read(&doc, &parts).unwrap();
    assert_eq!(
        read.identifiers.parts[0].dimensions,
        vec![Some("inspection-26".into())]
    );
    assert!(pmi::write::differences(&test_model(), &read.parts[0]).is_empty());
    let reread = pmi::read(&doc, &parts).unwrap();
    assert_eq!(read.identifiers, reread.identifiers);
}

#[test]
fn undefined_datums_and_disallowed_feature_roles() {
    let mut p = test_model();
    p.tolerances.push(GeometricTolerance {
        kind: ToleranceKind::Position,
        target: ToleranceTarget::Feature(FeatureId(0)),
        magnitude: None,
        zone: None,
        modifiers: vec![],
        unit_basis: None,
        maximum: None,
        unequal: None,
        datums: Some(
            DatumSystem::new(vec![Compartment {
                references: vec![DatumReference {
                    datum: DatumId(3),
                    modifiers: vec![],
                }],
                modifiers: vec![],
            }])
            .unwrap(),
        ),
        auxiliary: vec![],
        description: None,
    });
    let v = json::to_value(&json::identify(p, None).unwrap()).unwrap();
    assert!(
        json::from_value(&v, &anchors())
            .unwrap_err()
            .to_string()
            .contains("missing datum 3")
    );
    let mut p = test_model();
    p.features.push(Feature::Derived {
        kind: DerivedKind::Apex,
        from: vec![FeatureId(0)],
        geometry: None,
    });
    p.datums = vec![Datum::new(DatumLabel::new("A").unwrap(), Some(FeatureId(1)), vec![]).unwrap()];
    let v = json::to_value(&json::identify(p, None).unwrap()).unwrap();
    assert!(
        json::from_value(&v, &anchors())
            .unwrap_err()
            .to_string()
            .contains("kind not allowed")
    );
}

#[path = "common/pmi_gaps.rs"]
mod gaps;
#[test]
fn full_model_and_schema_reference_closure() {
    let p = gaps::every_kind();
    let a = Anchors {
        names: vec![None; 2],
        fingerprints: vec![],
        edge_count: 0,
    };
    let encoded = json::to_value(&json::identify(p.clone(), None).unwrap()).unwrap();
    assert_eq!(json::from_value(&encoded, &a).unwrap().pmi, p);
    let s: serde_json::Value = serde_json::from_str(json::PMI_SCHEMA).unwrap();
    fn refs(v: &serde_json::Value, root: &serde_json::Value) {
        if let Some(r) = v["$ref"].as_str() {
            assert!(root.pointer(r.strip_prefix('#').unwrap()).is_some(), "{r}");
        }
        match v {
            serde_json::Value::Object(m) => m.values().for_each(|v| refs(v, root)),
            serde_json::Value::Array(a) => a.iter().for_each(|v| refs(v, root)),
            _ => {}
        }
    }
    refs(&s, &s);
}

#[test]
fn specify_face_fingerprint_support_key() {
    let mut a = anchors();
    a.bind_support(2, "verified-face-hash").unwrap();
    let mut v = json::to_value(&json::identify(test_model(), None).unwrap()).unwrap();
    v["pmi"]["features"][0]["value"]["items"][0]["face"] =
        json!({"fingerprint":{"schema":2,"faces":"verified-face-hash","support":[0]}});
    assert_eq!(json::from_value(&v, &a).unwrap().pmi, test_model());
    v["pmi"]["features"][0]["value"]["items"][0]["face"]["fingerprint"]["faces"] =
        json!("another-part");
    assert!(
        json::from_value(&v, &a)
            .unwrap_err()
            .to_string()
            .contains("unknown face fingerprint")
    );
}

#[test]
fn text_parser_refuses_duplicates_at_every_depth() {
    for text in [
        r#"{"schema":"haecceity.pmi/1","schema":"haecceity.pmi/1"}"#,
        r#"{"pmi":{"features":[],"features":[]}}"#,
    ] {
        assert!(
            json::parse(text)
                .unwrap_err()
                .to_string()
                .contains("duplicate key")
        );
    }
    assert!(json::parse("{} trailing").is_err());
    let model = json::identify(test_model(), None).unwrap();
    assert_eq!(
        json::from_str(&json::to_string(&model).unwrap(), &anchors())
            .unwrap()
            .pmi,
        model.pmi
    );
}

#[test]
fn named_step_faces_use_the_mesh_face_order_and_write_identically() {
    use haecceity::p21::{self, Attribute, RawEntity};
    let doc = Document::parse(cube()).unwrap();
    let parts = read_part_definitions(doc.bytes()).unwrap();
    let mut face = doc.get(parts[0].faces[0]).unwrap().clone();
    if let RawEntity::Simple { attributes, .. } = &mut face {
        attributes[0] = Attribute::String("OCCT named top".into());
    } else {
        panic!("cube face is simple");
    }
    let mut edit = p21::Edit::new();
    edit.replace(parts[0].faces[0], face);
    let bytes = doc.apply(&edit).unwrap().bytes;
    let doc = Document::parse(bytes.clone()).unwrap();
    let parts = read_part_definitions(&bytes).unwrap();
    let mesh_part = haecceity::step::read_part(&bytes, &parts[0]).unwrap();
    assert_eq!(mesh_part.faces.len(), parts[0].faces.len());
    for (index, entity) in parts[0].faces.iter().enumerate() {
        assert_eq!(mesh_part.face_source(index).unwrap().entity, *entity);
    }
    let mut anchors = Anchors::read(&doc, &parts[0]);
    assert_eq!(anchors.names[0].as_deref(), Some("OCCT named top"));
    anchors.bind_support(2, "verified").unwrap();
    let v = json::to_value(&json::identify(test_model(), None).unwrap()).unwrap();
    let mut outputs = vec![];
    for face in [
        json!(0),
        json!({"name":"OCCT named top"}),
        json!({"fingerprint":{"schema":2,"faces":"verified","support":[0]}}),
    ] {
        let mut v = v.clone();
        v["pmi"]["features"][0]["value"]["items"][0]["face"] = face;
        let decoded = json::from_value(&v, &anchors).unwrap();
        outputs.push(
            json::write(
                &doc,
                &parts,
                &[(PartId(0), decoded)],
                pmi::write::PresentationPolicy::RemovePresentation,
            )
            .unwrap()
            .0,
        );
    }
    assert_eq!(outputs[0], outputs[1]);
    assert_eq!(outputs[0], outputs[2]);
}

#[test]
fn every_written_kind_keeps_its_caller_id_through_migration() {
    use std::collections::BTreeSet;
    let bytes = cube();
    let doc = Document::parse(bytes.clone()).unwrap();
    let parts = read_part_definitions(&bytes).unwrap();
    let input = json::identify(gaps::every_kind(), None).unwrap();
    let expected: BTreeSet<_> = input.ids.values().flatten().cloned().collect();
    let (written, _) = json::write(
        &doc,
        &parts,
        &[(PartId(0), input)],
        pmi::write::PresentationPolicy::RemovePresentation,
    )
    .unwrap();
    let defs = read_part_definitions(&written).unwrap();
    let read = pmi::read(&Document::parse(written).unwrap(), &defs).unwrap();
    let output = json::identify(read.parts[0].clone(), Some(&read.identifiers.parts[0])).unwrap();
    let actual: BTreeSet<_> = output.ids.values().flatten().cloned().collect();
    assert!(
        expected.is_subset(&actual),
        "caller ids lost: {:?}",
        expected.difference(&actual).collect::<Vec<_>>()
    );
    // Writer-created auxiliary features receive deterministic ids on read.
    let again = json::identify(read.parts[0].clone(), Some(&read.identifiers.parts[0])).unwrap();
    assert_eq!(output.ids, again.ids);
}

#[test]
fn read_only_extensions_mixed_units_and_iso_classes_are_lossless() {
    let mut p = test_model();
    let length = |text: &str| Length {
        value: Decimal::parse(text).unwrap(),
        unit: LengthUnit::Foot,
    };
    let placement = Placement {
        origin: [length("0.000"), length("1.25"), length("-2.5")],
        axis: None,
        ref_direction: None,
    };
    p.holes.push(Hole {
        feature: FeatureId(0),
        definition: RoundHole {
            name: "source hole".into(),
            diameter: length("0.5"),
            diameter_tolerance: None,
            depth: Some(length("1.0")),
            depth_tolerance: None,
            through: false,
            placement: placement.clone(),
        },
        placement: Some(placement),
    });
    p.general.push(GeneralTolerance::Class {
        text: "ISO 2768-mK".into(),
        standard: standards::Iso2768::recognise("ISO 2768-mK"),
    });
    p.attributes.push(AttributeSet {
        name: "custom units".into(),
        on: None,
        description: None,
        general_property: None,
        items: vec![
            (
                "length".into(),
                AttributeValue::Measure(Value::length(
                    Decimal::parse("0.2500").unwrap(),
                    LengthUnit::Other {
                        name: "custom length".into(),
                        metres: Decimal::parse("0.0025").unwrap(),
                    },
                )),
            ),
            (
                "angle".into(),
                AttributeValue::Measure(Value::angle(
                    Decimal::parse("12.3400").unwrap(),
                    AngleUnit::Other {
                        name: "custom angle".into(),
                        radians: Decimal::parse("0.01").unwrap(),
                    },
                )),
            ),
        ],
    });
    let v = json::to_value(&json::identify(p.clone(), None).unwrap()).unwrap();
    assert_eq!(json::from_value(&v, &anchors()).unwrap().pmi, p);
}

#[test]
fn independently_named_aliases_are_refused_if_the_writer_coalesces_them() {
    let input = cube();
    let doc = Document::parse(input.clone()).unwrap();
    let parts = read_part_definitions(&input).unwrap();
    let mut p = test_model();
    p.features.push(p.features[0].clone());
    let semantic = json::identify(p, None).unwrap();
    let error = json::write(
        &doc,
        &parts,
        &[(PartId(0), semantic)],
        pmi::write::PresentationPolicy::RemovePresentation,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("cannot be represented independently"),
        "{error}"
    );
}
