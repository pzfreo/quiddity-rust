//! Reader repairs are structural: repaired values, instance evidence, semantic references,
//! and a writer/read-back check. The model's constructors remain strict.
mod common;
use haecceity::{
    p21::Document,
    pmi::{
        self, Anchor, Bounds, DimTolerance, DimensionKind, Feature, FeatureId, FindingKind, PartId,
        PartPmi, PmiRead, ToleranceTarget,
    },
    step::{PartDefinition, read_part_definitions},
};
use std::{io::Read, path::Path};

fn load(path: &Path) -> Vec<u8> {
    let bytes = std::fs::read(path).unwrap();
    if path.extension().is_some_and(|s| s == "gz") {
        let mut out = Vec::new();
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
    let r = pmi::read(&doc, &parts).unwrap();
    for p in &r.parts {
        p.validate().unwrap();
    }
    (r, doc, parts)
}
fn fixture(extra: &str) -> (PmiRead, Document, Vec<PartDefinition>) {
    // Existing bolt: shape #4, representation #10, millimetre #400, faces #17/#105/#197.
    let base = String::from_utf8(load(
        &common::fixtures().join("ap242/specify/bolt_thread_knurl.step.gz"),
    ))
    .unwrap();
    let i = base.rfind("ENDSEC;").unwrap();
    read(format!("{}{extra}\n{}", &base[..i], &base[i..]).into_bytes())
}
fn measure(id: u64, name: &str, value: &str) -> String {
    format!(
        "#{id}=(LENGTH_MEASURE_WITH_UNIT() MEASURE_REPRESENTATION_ITEM() MEASURE_WITH_UNIT(LENGTH_MEASURE({value}),#400) REPRESENTATION_ITEM('{name}'));\n"
    )
}
fn dimension(
    nominal: &str,
    upper: &str,
    lower: &str,
    deviations: bool,
) -> (PmiRead, Document, Vec<PartDefinition>) {
    let mut extra = String::from(
        "#1000=DIMENSIONAL_SIZE(#1001,'diameter');\n#1001=SHAPE_ASPECT('test size','',#4,.T.);\n#1002=GEOMETRIC_ITEM_SPECIFIC_USAGE('','',#1001,#10,#17);\n#1003=DIMENSIONAL_CHARACTERISTIC_REPRESENTATION(#1000,#1008);\n",
    );
    extra += &measure(1004, "nominal value", nominal);
    extra += &measure(1005, if deviations { "" } else { "upper limit" }, upper);
    extra += &measure(1006, if deviations { "" } else { "lower limit" }, lower);
    if deviations {
        extra += "#1008=SHAPE_DIMENSION_REPRESENTATION('',(#1004),#399);\n#1009=PLUS_MINUS_TOLERANCE(#1010,#1000);\n#1010=TOLERANCE_VALUE(#1006,#1005);\n";
    } else {
        extra += "#1008=SHAPE_DIMENSION_REPRESENTATION('',(#1004,#1005,#1006),#399);\n";
    }
    fixture(&extra)
}
fn own_dimension(r: &PmiRead) -> Option<&pmi::Dimension> {
    r.provenance.parts[0]
        .dimensions
        .iter()
        .position(|ids| ids.contains(&1000))
        .map(|i| &r.parts[0].dimensions[i])
}
fn limits(d: &pmi::Dimension) -> &Bounds {
    match &d.tolerance {
        DimTolerance::Limits(b) | DimTolerance::Deviations(b) => b,
        other => panic!("{other:?}"),
    }
}
fn repaired(r: &PmiRead) -> Vec<&pmi::Finding> {
    r.findings
        .iter()
        .filter(|f| {
            f.detail.contains("stated reversed")
                && f.detail.contains("swapped")
                && !f.detail.contains("not swapped")
        })
        .collect()
}
#[test]
fn reversed_limits_are_read_with_nonfatal_instance_evidence_and_write_ascending() {
    let (r, doc, parts) = dimension("35.", "34.8", "35.2", false);
    let d = own_dimension(&r).unwrap();
    let b = limits(d);
    assert_eq!(
        (b.lower().decimal().as_str(), b.upper().decimal().as_str()),
        ("34.8", "35.2")
    );
    let repairs = repaired(&r);
    assert_eq!(repairs.len(), 1);
    let f = repairs[0];
    assert_eq!(f.kind, FindingKind::Nonconformance);
    assert_eq!(f.part, Some(PartId(0)));
    assert_eq!(f.ids[0], 1000);
    for id in [1005, 1006, 1008] {
        assert!(f.ids.contains(&id), "{f:?}");
    }
    // Strict construction is unchanged: repair belongs to import, not the model API.
    assert!(Bounds::new(b.lower().clone(), b.upper().clone()).is_err());
    let mut d = d.clone();
    let DimensionKind::Size { feature, .. } = &mut d.kind else {
        panic!("size");
    };
    let source = *feature;
    *feature = FeatureId(0);
    let requested = PartPmi {
        features: vec![r.parts[0].features[source.0].clone()],
        dimensions: vec![d],
        ..PartPmi::default()
    };
    let (edit, _) = pmi::write(
        &doc,
        &parts,
        &[(PartId(0), requested.clone())],
        pmi::Mode::Replace,
        pmi::write::PresentationPolicy::RemovePresentation,
    )
    .unwrap();
    let (after, _, _) = read(doc.apply(&edit).unwrap().bytes);
    assert_eq!(after.parts[0].dimensions, requested.dimensions);
    assert!(
        repaired(&after).is_empty(),
        "writer must state ascending limits, not require a second repair"
    );
}
#[test]
fn clean_limits_stay_stated_and_equal_or_outside_reversals_stay_refused() {
    let (r, _, _) = dimension("35.", "35.2", "34.8", false);
    let b = limits(own_dimension(&r).unwrap());
    assert_eq!(
        (b.upper().decimal().as_str(), b.lower().decimal().as_str()),
        ("35.2", "34.8")
    );
    assert!(repaired(&r).is_empty());
    for (n, u, l, reason) in [
        ("35.", "35.", "35.", "not greater"),
        ("36.", "34.8", "35.2", "outside"),
    ] {
        let (r, _, _) = dimension(n, u, l, false);
        assert!(own_dimension(&r).is_none());
        assert!(repaired(&r).is_empty());
        assert!(
            r.findings
                .iter()
                .any(|f| f.kind == FindingKind::Nonconformance
                    && f.ids.contains(&1000)
                    && f.detail.contains(reason)),
            "{:?}",
            r.findings
        );
    }
}
#[test]
fn a_conflicting_tolerance_form_is_refused_without_claiming_a_successful_repair() {
    let (_, doc, _) = dimension("35.", "34.8", "35.2", false);
    let mut text = String::from_utf8(doc.bytes().to_vec()).unwrap().replace(
        "#1008=SHAPE_DIMENSION_REPRESENTATION('',(#1004,#1005,#1006),#399);",
        "#1008=SHAPE_DIMENSION_REPRESENTATION('',(#1004,#1005,#1006,#1007),#399);",
    );
    let at = text.rfind("ENDSEC;").unwrap();
    text.insert_str(
        at,
        "#1007=DESCRIPTIVE_REPRESENTATION_ITEM('dimensional note','theoretical');\n",
    );
    let (r, _, _) = read(text.into_bytes());
    assert!(own_dimension(&r).is_none());
    assert!(repaired(&r).is_empty());
    assert!(r.findings.iter().any(|f| f.kind == FindingKind::Conflict
        && f.ids.contains(&1000)
        && f.detail.contains("more than one")));
}

#[test]
fn reversed_deviations_are_signed_offsets_including_same_sign_intervals() {
    for (u, l, expected) in [
        ("-0.2", "0.2", ("-0.2", "0.2")),
        ("0.1", "0.2", ("0.1", "0.2")),
        ("-0.2", "-0.1", ("-0.2", "-0.1")),
    ] {
        let (r, _, _) = dimension("35.", u, l, true);
        let b = limits(own_dimension(&r).unwrap());
        assert_eq!(
            (b.lower().decimal().as_str(), b.upper().decimal().as_str()),
            expected
        );
        let f = repaired(&r);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].kind, FindingKind::Nonconformance);
        for id in [1000, 1005, 1006, 1009, 1010] {
            assert!(f[0].ids.contains(&id));
        }
    }
    let (r, _, _) = dimension("35.", "0.2", "0.1", true);
    assert!(own_dimension(&r).is_some());
    assert!(repaired(&r).is_empty());
    let (r, _, _) = dimension("35.", "0.1", "0.1", true);
    assert!(own_dimension(&r).is_none());
    assert!(repaired(&r).is_empty());
}
#[test]
fn datum_feature_constituent_is_the_same_feature_when_named_by_a_tolerance() {
    // The label is conventional, not a discriminator: both named and anonymous plain
    // composition relationships are allowed by §6.5.2. #1200 is an unrelated all-around.
    for description in ["'datum feature'", "$", "''"] {
        let (r, _, parts) = fixture(&format!(
            "#1100=DATUM_FEATURE('compound','',#4,.T.);
#1101=SHAPE_ASPECT('child face','',#4,.T.);
#1102=GEOMETRIC_ITEM_SPECIFIC_USAGE('','',#1101,#10,#17);
#1103=GEOMETRIC_ITEM_SPECIFIC_USAGE('','',#1100,#10,#105);
#1104=SHAPE_ASPECT_RELATIONSHIP('',{description},#1100,#1101);
#1105=DATUM('','',#4,.F.,'Z');
#1106=SHAPE_ASPECT_RELATIONSHIP('','',#1100,#1105);
#1107=FLATNESS_TOLERANCE('on child','',#1108,#1101);
#1108=LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(0.1),#400);
#1200=ALL_AROUND_SHAPE_ASPECT('unrelated','',#4,.T.);
#1201=SHAPE_ASPECT('all-around member','',#4,.T.);
#1202=SHAPE_ASPECT_RELATIONSHIP('','',#1200,#1201);
#1203=SHAPE_ASPECT_RELATIONSHIP('','',#1200,$);
#1204=GEOMETRIC_ITEM_SPECIFIC_USAGE('','',#1201,#10,#197);"
        ));
        let p = &r.parts[0];
        let datum = p.datums.iter().find(|d| d.label().as_str() == "Z").unwrap();
        let feature = datum.feature().unwrap();
        let tol_index = r.provenance.parts[0]
            .tolerances
            .iter()
            .position(|ids| ids.contains(&1107))
            .unwrap();
        assert_eq!(
            p.tolerances[tol_index].target,
            ToleranceTarget::Feature(feature)
        );
        let Feature::Items(items) = &p.features[feature.0] else {
            panic!("union");
        };
        let mut faces: Vec<_> = items
            .iter()
            .map(|a| match a {
                Anchor::Face(i) => parts[0].faces[i.0],
                other => panic!("{other:?}"),
            })
            .collect();
        faces.sort();
        assert_eq!(faces, [17, 105]);
        let prov = &r.provenance.parts[0].features[feature.0];
        for id in [1100, 1101, 1102, 1103, 1104] {
            assert!(prov.contains(&id));
        }
        assert_eq!(
            r.provenance.parts[0]
                .features
                .iter()
                .filter(|ids| ids.contains(&1101))
                .count(),
            1
        );
        let all = r.provenance.parts[0]
            .features
            .iter()
            .position(|ids| ids.contains(&1200))
            .unwrap();
        assert!(matches!(
            p.features[all],
            Feature::Group {
                kind: pmi::GroupKind::AllAround,
                ..
            }
        ));
        assert_ne!(feature, FeatureId(all));
    }
}
#[test]
#[ignore = "requires the maintainer's git-excluded samples"]
fn se_and_control_samples() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples");
    for (name, counts) in [
        ("09-NIST-reexported-by-SE.stp", (7, 3, 2)),
        ("01-NIST-control-ctc01.stp", (12, 6, 3)),
    ] {
        let (r, _, parts) = read(load(&root.join(name)));
        let p = &r.parts[0];
        assert_eq!(
            (p.dimensions.len(), p.tolerances.len(), p.datums.len()),
            counts
        );
        println!("{name}: {counts:?}");
        if name.starts_with("09-") {
            for raw in [129, 130] {
                let i = r.provenance.parts[0]
                    .dimensions
                    .iter()
                    .position(|ids| ids.contains(&raw))
                    .unwrap();
                let d = &p.dimensions[i];
                assert_eq!(d.nominal.as_ref().unwrap().decimal().as_str(), "35.");
                let b = limits(d);
                assert_eq!(
                    (b.lower().decimal().as_str(), b.upper().decimal().as_str()),
                    if raw == 129 {
                        ("34.8", "35.2")
                    } else {
                        // #41/#42 carry these tokens: preserve their stated precision.
                        ("34.8000000000002", "35.2000000000002")
                    }
                );
                assert!(repaired(&r).iter().any(|f| f.ids[0] == raw));
            }
            let feature = p
                .datums
                .iter()
                .find(|d| d.label().as_str() == "C")
                .unwrap()
                .feature()
                .unwrap();
            assert_eq!(feature, FeatureId(1));
            assert!(r.provenance.parts[0].features[feature.0].contains(&222));
            let Feature::Items(items) = &p.features[feature.0] else {
                panic!("faces");
            };
            assert!(
                items
                    .iter()
                    .any(|a| matches!(a,Anchor::Face(i) if parts[0].faces[i.0]==2502))
            );
        }
    }
}
