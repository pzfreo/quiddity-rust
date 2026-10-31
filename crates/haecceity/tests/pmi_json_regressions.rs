//! Minimal xcaf-pmi producer regressions for #142.
#![cfg(feature = "json")]
use haecceity::{
    json::{self, Anchors},
    p21::Document,
    pmi::{self, PartId},
    step::read_part_definitions,
};
use serde_json::{Value, json};
use std::io::Read;

fn fixture(name: &str) -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/q142")
        .join(name);
    json::parse(&std::fs::read_to_string(path).unwrap()).unwrap()
}
fn geometry() -> Document {
    let mut bytes = vec![];
    flate2::read::GzDecoder::new(&include_bytes!("fixtures/q142/geometry.step.gz")[..])
        .read_to_end(&mut bytes)
        .unwrap();
    Document::parse(bytes).unwrap()
}
fn identity(value: &Value) -> (Document, pmi::write::WriteReport) {
    json::validate(value).unwrap();
    let doc = geometry();
    let parts = read_part_definitions(doc.bytes()).unwrap();
    let semantic = json::from_value(value, &Anchors::read(&doc, &parts[0])).unwrap();
    let (bytes, report) = json::write(
        &doc,
        &parts,
        &[(PartId(0), semantic)],
        pmi::write::PresentationPolicy::RemovePresentation,
    )
    .unwrap();
    let after = Document::parse(bytes).unwrap();
    let parts = read_part_definitions(after.bytes()).unwrap();
    let read = pmi::read(&after, &parts).unwrap();
    let actual = json::identify(read.parts[0].clone(), Some(&read.identifiers.parts[0])).unwrap();
    assert_eq!(json::to_value(&actual).unwrap(), *value);
    assert!(haecceity::express::validate_document(after.graph()).is_empty());
    for violation in haecceity::express_rules::check_all(&after) {
        assert!(
            report.practice_schema_conflicts.contains(&violation),
            "{violation:?}"
        );
    }
    (after, report)
}

#[test]
fn producer_datum_target_decimals_are_stated_verbatim() {
    let original = fixture("datum-target-decimals.json");
    identity(&original);
    for (zero, one, size, coordinate) in [
        ("0.0", "1.0", "8.0", "-30.0"),
        ("0.", "1.", "8.", "-30."),
        ("0.5", "1.5", "8.5", "-30.5"),
        ("0.E+0", "1.00E0", "8.E+0", "-3.0E+1"),
    ] {
        for shape in [
            json!("point"),
            json!({"line":{"length":{"unit":"unit/mm","value":size}}}),
            json!({"rectangle":{"length":{"unit":"unit/mm","value":size},"width":{"unit":"unit/mm","value":size}}}),
            json!({"circle":{"diameter":{"unit":"unit/mm","value":size}}}),
            json!({"circular curve":{"diameter":{"unit":"unit/mm","value":size}}}),
        ] {
            let mut value = original.clone();
            for target in value["pmi"]["datum_targets"].as_array_mut().unwrap() {
                target["value"]["shape"] = shape.clone();
                let p = &mut target["value"]["placement"];
                p["origin"] = json!([
                    {"unit":"unit/mm","value":coordinate},
                    {"unit":"unit/mm","value":zero},
                    {"unit":"unit/mm","value":one}
                ]);
                // Equal REAL values with distinct tokens in the same record must survive.
                p["axis"] = json!([zero, "0.00", one]);
                p["ref_direction"] = json!([one, "0.E-1", "-0.0"]);
                if shape.get("line").is_some() {
                    target["value"]["movable"] = json!([one, "0.00", "0.E0"]);
                }
            }
            identity(&value);
        }
    }
    let mut value = original;
    value["pmi"]["geometry"] = json!([{"id":"supplemental point", "value":{
        "length_unit":"unit/mm",
        "item":{"leaves":[["CARTESIAN_POINT", [
            {"text":""}, {"list":[{"real":"0.0"},{"real":"0.E+0"},{"real":"1.00"}]}
        ]]]}
    }}]);
    identity(&value);
    value["pmi"]["geometry"][0]["value"]["item"]["leaves"][0][1][1]["list"][0]["real"] =
        json!("0e0");
    assert!(
        json::validate(&value)
            .unwrap_err()
            .to_string()
            .contains("Part 21 REAL")
    );
}

#[test]
fn producer_requirement_notes_use_characterized_items_in_a_supported_edition() {
    let mut value = fixture("requirement-note.json");
    for owner in [
        json!({"tolerance":"0:1:4:10"}),
        json!({"dimension":"0:1:4:7"}),
    ] {
        value["pmi"]["notes"][0]["value"]["on"] = owner;
        let (after, report) = identity(&value);
        assert_eq!(
            after.file_schema().unwrap(),
            [haecceity::express::TARGET_SCHEMA]
        );
        assert!(matches!(
            report.schema,
            pmi::write::SchemaChange::Upgraded { .. }
        ));
        // The native reader resolves the requirement owner without transport aliases.
        let parts = read_part_definitions(after.bytes()).unwrap();
        let native = pmi::read_native(&after, &parts).unwrap();
        assert!(matches!(
            native.parts[0].notes[0].on,
            Some(pmi::NoteOwner::Dimension(_) | pmi::NoteOwner::Tolerance(_))
        ));
    }
}

#[test]
fn every_note_owner_is_json_step_json_identity_even_on_shared_features() {
    let mut value = fixture("requirement-note.json");
    let targets = fixture("datum-target-decimals.json");
    for collection in ["features", "datum_targets", "datums"] {
        value["pmi"][collection].as_array_mut().unwrap().extend(
            targets["pmi"][collection]
                .as_array()
                .unwrap()
                .iter()
                .cloned(),
        );
    }
    let mut dimension = value["pmi"]["dimensions"][0].clone();
    dimension["id"] = json!("size twin");
    value["pmi"]["dimensions"]
        .as_array_mut()
        .unwrap()
        .push(dimension.clone());
    dimension["id"] = json!("location");
    dimension["value"]["kind"] = json!({"location":{
        "from":"feature/7","to":"feature/12","kind":"linear distance",
        "angle":null,"directed":false,"path":null
    }});
    dimension["value"]["tolerance"] = json!("none");
    value["pmi"]["dimensions"]
        .as_array_mut()
        .unwrap()
        .push(dimension);
    let mut tolerance = value["pmi"]["tolerances"][0].clone();
    tolerance["id"] = json!("tolerance twin");
    value["pmi"]["tolerances"]
        .as_array_mut()
        .unwrap()
        .push(tolerance);
    value["pmi"]["notes"] = json!([
        null,
        {"feature":"feature/7"},
        {"dimension":"0:1:4:7"},
        {"dimension":"size twin"},
        {"dimension":"location"},
        {"tolerance":"0:1:4:10"},
        {"tolerance":"tolerance twin"},
        {"datum_target":"datum_target/D1"}
    ])
    .as_array()
    .unwrap()
    .iter()
    .enumerate()
    .map(|(i, owner)| {
        json!({"id":format!("note/{i}"),"value":{
            "kind":"semantic text","text":"Check before assembly","on":owner,"callouts":[]
        }})
    })
    .collect::<Vec<_>>()
    .into();
    identity(&value);
}

#[test]
fn validate_refuses_geometry_independent_import_failures() {
    let original = fixture("requirement-note.json");
    for (path, replacement, reason) in [
        (
            "/pmi/dimensions/0/value/nominal",
            Value::Null,
            "without a nominal",
        ),
        (
            "/pmi/dimensions/0/value/nominal/quantity/length/value",
            json!("10"),
            "Part 21 REAL",
        ),
        (
            "/pmi/dimensions/0/value/nominal/quantity/length/value",
            json!("1.0e1"),
            "Part 21 REAL",
        ),
        (
            "/pmi/notes/0/value/on",
            json!({"tolerance":"missing"}),
            "unknown tolerances id",
        ),
        (
            "/pmi/tolerances/0/value/zone",
            json!({"form":"cylindrical or circular","projected":null,"non_uniform":null,
                "runout_angle":null,"affected_plane":null,"affected_plane_kind":"intersection_plane"}),
            "affected plane kind has no affected plane feature",
        ),
        (
            "/pmi/tolerances/0/value/zone",
            json!({"form":"cylindrical or circular","projected":null,"non_uniform":null,
                "runout_angle":null,"affected_plane":"feature/12","affected_plane_kind":"intersection_plane"}),
            "affected plane requires an explicit",
        ),
    ] {
        let mut value = original.clone();
        *value.pointer_mut(path).unwrap() = replacement;
        assert!(json::validate_schema(&value).is_ok());
        let error = json::validate(&value).unwrap_err().to_string();
        assert!(error.contains(reason), "{error}");
        let doc = geometry();
        let parts = read_part_definitions(doc.bytes()).unwrap();
        let imported =
            json::from_value(&value, &Anchors::read(&doc, &parts[0])).and_then(|semantic| {
                json::write(
                    &doc,
                    &parts,
                    &[(PartId(0), semantic)],
                    pmi::write::PresentationPolicy::RemovePresentation,
                )
            });
        assert!(imported.is_err());
    }
    let mut target = fixture("datum-target-decimals.json");
    target["pmi"]["datum_targets"][0]["value"]["shape"] = json!({"curve":"feature/3"});
    assert!(json::validate_schema(&target).is_ok());
    assert!(
        json::validate(&target)
            .unwrap_err()
            .to_string()
            .contains("explicit area/curve target requires stated face/curve geometry")
    );
}
