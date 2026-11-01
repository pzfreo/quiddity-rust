//! Native source statements and JSON identity regressions reported by xcaf-pmi.
mod common;
use haecceity::{
    p21::Document,
    pmi::{
        self, AuxiliaryClassification, Feature, FindingKind, PmiRead, ToleranceKind,
        ToleranceTarget, ZoneForm,
    },
    step::{PartDefinition, read_part_definitions},
};
use std::{io::Read, path::Path};

fn load(path: &Path) -> Vec<u8> {
    let bytes = std::fs::read(path).unwrap();
    if path.extension().is_some_and(|s| s == "gz") {
        let mut out = vec![];
        flate2::read::GzDecoder::new(bytes.as_slice())
            .read_to_end(&mut out)
            .unwrap();
        out
    } else {
        bytes
    }
}
fn read(bytes: Vec<u8>) -> (PmiRead, Document, Vec<PartDefinition>) {
    let parts = read_part_definitions(&bytes).unwrap();
    let doc = Document::parse(bytes).unwrap();
    let read = pmi::read(&doc, &parts).unwrap();
    for p in &read.parts {
        p.validate().unwrap();
    }
    (read, doc, parts)
}
fn fixture(extra: &str) -> (PmiRead, Document, Vec<PartDefinition>) {
    // Existing bolt: shape #4, representation #10, millimetre #400, face #17.
    let base = String::from_utf8(load(
        &common::fixtures().join("ap242/specify/bolt_thread_knurl.step.gz"),
    ))
    .unwrap();
    let at = base.rfind("ENDSEC;").unwrap();
    read(format!("{}{extra}\n{}", &base[..at], &base[at..]).into_bytes())
}
const SUPPORT: &str = "
#1000=SHAPE_ASPECT('test feature','',#4,.T.);
#1001=GEOMETRIC_ITEM_SPECIFIC_USAGE('','',#1000,#10,#17);
#1002=LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(0.1),#400);
";
fn own_tolerance(r: &PmiRead, source: u64) -> Option<&pmi::GeometricTolerance> {
    r.provenance.parts[0]
        .tolerances
        .iter()
        .position(|p| p.contains(&source))
        .map(|i| &r.parts[0].tolerances[i])
}
#[test]
fn native_maximum_is_one_tolerance_with_its_own_magnitude_and_cap() {
    let (r, _, _) = fixture(&format!(
        "{SUPPORT}
#1003=(GEOMETRIC_TOLERANCE('with maximum','',#1002,#1000)
GEOMETRIC_TOLERANCE_WITH_MODIFIERS((.MAXIMUM_MATERIAL_REQUIREMENT.))
GEOMETRIC_TOLERANCE_WITH_MAXIMUM_TOLERANCE(#1004) POSITION_TOLERANCE());
#1004=LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(0.25),#400);"
    ));
    let t = own_tolerance(&r, 1003).unwrap();
    assert_eq!(t.kind, ToleranceKind::Position);
    assert_eq!(t.magnitude.as_ref().unwrap().decimal().as_str(), "0.1");
    assert_eq!(t.maximum.as_ref().unwrap().decimal().as_str(), "0.25");
    assert_eq!(
        r.provenance.parts[0]
            .tolerances
            .iter()
            .filter(|p| p.contains(&1004))
            .count(),
        1
    );
    assert!(
        !r.findings.iter().any(|f| f.ids.contains(&1003)),
        "{:?}",
        r.findings
    );
}
#[test]
fn all_over_classification_keeps_its_feature_scope_and_part_targets_stay_literal() {
    let (r, _, _) = fixture(&format!(
        "{SUPPORT}
#1003=SURFACE_PROFILE_TOLERANCE('feature scoped','',#1002,#1000);
#1004=GEOMETRIC_TOLERANCE_AUXILIARY_CLASSIFICATION(.ALL_OVER.,#1003);
#1005=SURFACE_PROFILE_TOLERANCE('part scoped','',#1002,#4);"
    ));
    let t = own_tolerance(&r, 1003).unwrap();
    assert_eq!(t.auxiliary, [AuxiliaryClassification::AllOver]);
    let ToleranceTarget::Feature(feature) = t.target else {
        panic!("all over must not widen a feature target")
    };
    assert!(matches!(&r.parts[0].features[feature.0], Feature::Items(items) if items.len() == 1));
    // PMI practice §6.3 also explicitly permits a product_definition_shape target.
    assert_eq!(
        own_tolerance(&r, 1005).unwrap().target,
        ToleranceTarget::WholePart
    );
}
#[test]
fn incomplete_non_runout_zone_retains_the_explicit_diameter_form() {
    let (r, _, _) = fixture(&format!(
        "{SUPPORT}
#1003=POSITION_TOLERANCE('position','',#1002,#1000);
#1004=TOLERANCE_ZONE_FORM('cylindrical or circular');
#1005=TOLERANCE_ZONE('','',#4,.F.,(#1003),#1004);
#1006=RUNOUT_ZONE_DEFINITION(#1005,());"
    ));
    let t = own_tolerance(&r, 1003).unwrap();
    let z = t.zone.as_ref().unwrap();
    assert_eq!(z.form, ZoneForm::CylindricalOrCircular);
    assert!(z.runout_angle.is_none());
    assert!(
        r.findings
            .iter()
            .any(|f| f.kind == FindingKind::Nonconformance
                && [1003, 1006].iter().all(|id| f.ids.contains(id))
                && f.detail.contains("explicitly stated zone form")),
        "{:?}",
        r.findings
    );
    // The same omission remains ambiguous without a stated zone form or with boundaries.
    for (form, boundaries) in [
        ("unknown", "()"),
        ("within a cone", "()"),
        ("cylindrical or circular", "(#1000)"),
    ] {
        let (refused, _, _) = fixture(&format!(
            "{SUPPORT}
#1003=POSITION_TOLERANCE('position','',#1002,#1000);
#1004=TOLERANCE_ZONE_FORM('{form}');
#1005=TOLERANCE_ZONE('','',#4,.F.,(#1003),#1004);
#1006=RUNOUT_ZONE_DEFINITION(#1005,{boundaries});"
        ));
        assert!(own_tolerance(&refused, 1003).is_none());
        assert!(
            refused
                .findings
                .iter()
                .any(|f| f.ids.contains(&1006) && f.detail.contains("tolerance not read"))
        );
    }
}
#[test]
fn a_real_runout_tolerance_still_requires_its_missing_orientation() {
    let (r, _, _) = fixture(&format!(
        "{SUPPORT}
#1200=DATUM_FEATURE('datum feature','',#4,.T.);
#1201=GEOMETRIC_ITEM_SPECIFIC_USAGE('','',#1200,#10,#105);
#1202=DATUM('','',#4,.F.,'Z');
#1203=SHAPE_ASPECT_RELATIONSHIP('','',#1200,#1202);
#1204=DATUM_REFERENCE_COMPARTMENT('','',#4,.F.,#1202,$);
#1205=DATUM_SYSTEM('','',#4,.F.,(#1204));
#1003=(CIRCULAR_RUNOUT_TOLERANCE() GEOMETRIC_TOLERANCE('runout','',#1002,#1000)
GEOMETRIC_TOLERANCE_WITH_DATUM_REFERENCE((#1205)));
#1004=TOLERANCE_ZONE_FORM('between two parallel circles on a conical surface');
#1005=TOLERANCE_ZONE('','',#4,.F.,(#1003),#1004);
#1006=RUNOUT_ZONE_DEFINITION(#1005,());"
    ));
    assert!(own_tolerance(&r, 1003).is_none());
    assert!(r.findings.iter().any(|f| f.ids.contains(&1006)
        && f.detail.contains("runout tolerance")
        && f.detail.contains("tolerance not read")));
}

#[test]
fn nist_maximum_looking_frames_are_separate_source_controls() {
    let (r, doc, _) = read(load(
        &common::fixtures().join("ap242/nist/nist_ftc_07_asme1_ap242-e2.stp.gz"),
    ));
    // NIST expected rows 24/25 draw .040 M | .045 MAX. The actual STEP states four
    // independent position controls, not the §6.9.5 subtype or a maximum relationship.
    for (base, cap) in [(14544, 14579), (14781, 14816)] {
        let a = own_tolerance(&r, base).unwrap();
        let b = own_tolerance(&r, cap).unwrap();
        assert_eq!(a.kind, ToleranceKind::Position);
        assert_eq!(a.target, b.target);
        assert_eq!(a.datums, b.datums);
        assert_eq!(a.magnitude.as_ref().unwrap().decimal().as_str(), "0.04");
        assert_eq!(b.magnitude.as_ref().unwrap().decimal().as_str(), "0.045");
        assert!(a.maximum.is_none() && b.maximum.is_none());
        assert!(doc.get(base).is_some() && doc.get(cap).is_some());
    }
}

#[cfg(feature = "json-import")]
#[test]
fn repeated_and_empty_source_identifiers_do_not_become_ambiguous_wire_ids() {
    use haecceity::json::{self, Anchors};
    let (r, doc, parts) = fixture(&format!("{SUPPORT}
#1100=DIMENSIONAL_SIZE(#1000,'diameter');
#1101=DIMENSIONAL_SIZE(#1000,'diameter');
#1102=DIMENSIONAL_SIZE(#1000,'diameter');
#1103=ID_ATTRIBUTE('42',#1100);
#1104=ID_ATTRIBUTE('42',#1101);
#1105=ID_ATTRIBUTE('',#1102);
#1110=(LENGTH_MEASURE_WITH_UNIT() MEASURE_REPRESENTATION_ITEM() MEASURE_WITH_UNIT(LENGTH_MEASURE(10.),#400) REPRESENTATION_ITEM('nominal value'));
#1111=SHAPE_DIMENSION_REPRESENTATION('',(#1110),#399);
#1112=DIMENSIONAL_CHARACTERISTIC_REPRESENTATION(#1100,#1111);
#1113=DIMENSIONAL_CHARACTERISTIC_REPRESENTATION(#1101,#1111);
#1114=DIMENSIONAL_CHARACTERISTIC_REPRESENTATION(#1102,#1111);"));
    // Trim unrelated bolt attributes/notes: this fixture exercises dimension identity,
    // including native STEP transport, without extending the writer's attribute support.
    let model = pmi::PartPmi {
        features: r.parts[0].features.clone(),
        dimensions: r.parts[0].dimensions.clone(),
        geometry: r.parts[0].geometry.clone(),
        ..pmi::PartPmi::default()
    };
    let semantic = json::identify(model.clone(), Some(&r.identifiers.parts[0])).unwrap();
    let wire = json::to_value(&semantic).unwrap();
    let again = json::identify(model.clone(), Some(&r.identifiers.parts[0])).unwrap();
    assert_eq!(semantic.ids, again.ids);
    let dimensions = wire["pmi"]["dimensions"].as_array().unwrap();
    let own: Vec<_> = dimensions
        .iter()
        .filter(|d| d.get("identifier").is_some())
        .collect();
    assert_eq!(own.len(), 3);
    assert_eq!(own[0]["id"], "42");
    assert!(own[1]["id"].as_str().unwrap().starts_with("dimensions:"));
    assert!(own[2]["id"].as_str().unwrap().starts_with("dimensions:"));
    assert_ne!(own[1]["id"], own[2]["id"]);
    assert_eq!(
        (
            &own[0]["identifier"],
            &own[1]["identifier"],
            &own[2]["identifier"]
        ),
        (
            &serde_json::json!("42"),
            &serde_json::json!("42"),
            &serde_json::json!("")
        )
    );
    assert!(
        r.findings
            .iter()
            .any(|f| f.kind == FindingKind::Nonconformance
                && [1100, 1101, 1103, 1104].iter().all(|i| f.ids.contains(i))
                && f.detail.contains("\"42\""))
    );
    assert!(r.findings.iter().any(|f| f.ids.contains(&1102)
        && f.ids.contains(&1105)
        && f.detail.contains("empty source identifier")));
    let anchors = Anchors {
        names: vec![None; 500],
        edge_count: 500,
        ..Anchors::default()
    };
    let decoded = json::from_value(&wire, &anchors).unwrap();
    assert_eq!(json::to_value(&decoded).unwrap(), wire);
    assert_eq!(decoded.pmi, model);
    // Source observations are separate from the writer's unique native aliases. Both
    // duplicates and an explicitly empty value must survive the STEP transport too.
    let (output, _) = json::write(
        &doc,
        &parts,
        &[(pmi::PartId(0), decoded)],
        pmi::write::PresentationPolicy::RemovePresentation,
    )
    .unwrap();
    let (back, _, _) = read(output);
    let back = json::identify(back.parts[0].clone(), Some(&back.identifiers.parts[0])).unwrap();
    assert_eq!(json::to_value(&back).unwrap(), wire);
}
#[cfg(feature = "json-import")]
#[test]
fn every_nist_file_exports_and_preserves_its_source_identifiers_and_references() {
    use haecceity::json::{self, Anchors};
    for file in std::fs::read_dir(common::fixtures().join("ap242/nist")).unwrap() {
        let path = file.unwrap().path();
        if !path.to_string_lossy().ends_with(".stp.gz") {
            continue;
        }
        let (r, _, parts) = read(load(&path));
        let p = &r.parts[0];
        let semantic = json::identify(p.clone(), Some(&r.identifiers.parts[0])).unwrap();
        let wire = json::to_value(&semantic).unwrap();
        let mut ids = std::collections::BTreeSet::new();
        for (collection, values) in &semantic.ids {
            for id in values {
                assert!(
                    !id.is_empty() && ids.insert(id),
                    "{} {collection}",
                    path.display()
                );
            }
            if let Some(original) = semantic.identifiers.get(collection) {
                for (i, value) in original.iter().enumerate() {
                    assert_eq!(
                        wire["pmi"][collection][i].get("identifier"),
                        value
                            .as_ref()
                            .map(|v| serde_json::Value::String(v.clone()))
                            .as_ref()
                    );
                }
            }
        }
        let anchors = Anchors {
            names: vec![None; parts[0].faces.len()],
            edge_count: parts[0].edges.len(),
            ..Anchors::default()
        };
        let decoded = json::from_value(&wire, &anchors).unwrap();
        assert_eq!(decoded.pmi, *p, "{}", path.display());
        assert_eq!(json::to_value(&decoded).unwrap(), wire);
        if path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("nist_stc_06")
            || path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("nist_stc_09")
        {
            assert!(
                r.findings
                    .iter()
                    .any(|f| f.kind == FindingKind::Nonconformance
                        && f.detail.contains("duplicate source identifier")),
                "{}",
                path.display()
            );
        }
    }
}
