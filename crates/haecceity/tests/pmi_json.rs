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
