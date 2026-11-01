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
fn a_reversal_without_a_nominal_keeps_the_existing_refusal() {
    for deviations in [false, true] {
        let (_, doc, _) = dimension("35.", "34.8", "35.2", deviations);
        let text = String::from_utf8(doc.bytes().to_vec()).unwrap().replace(
            if deviations {
                "#1008=SHAPE_DIMENSION_REPRESENTATION('',(#1004),#399);"
            } else {
                "#1008=SHAPE_DIMENSION_REPRESENTATION('',(#1004,#1005,#1006),#399);"
            },
            if deviations {
                "#1008=SHAPE_DIMENSION_REPRESENTATION('',(),#399);"
            } else {
                "#1008=SHAPE_DIMENSION_REPRESENTATION('',(#1005,#1006),#399);"
            },
        );
        let (r, _, _) = read(text.into_bytes());
        assert!(own_dimension(&r).is_none());
        assert!(repaired(&r).is_empty());
    }
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
#1103=GEOMETRIC_ITEM_SPECIFIC_USAGE('','',#1100,#10,#17);
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
        assert_eq!(faces, [17]);
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
// Synthetic PMI on the existing three-face bolt fixture: the supplied LM10UU is
// FreeCAD geometry only, so it cannot be trimmed to preserve Specify's missing PMI.
// This reproduces its relevant topology: datum A -> three plain land aspects,
// each with its own independent diameter size (all nominally 19 mm). The test
// calls the new datum Z because the base bolt already has datums A and B.
#[test]
fn multi_constituent_datum_keeps_three_size_features() {
    let mut extra = String::from(
        "#1100=DATUM_FEATURE('lands','',#4,.T.);
#1101=DATUM('','',#4,.F.,'Z');
#1102=SHAPE_ASPECT_RELATIONSHIP('','',#1100,#1101);\n",
    );
    for (child, face) in [(1110, 17), (1120, 105), (1130, 197)] {
        extra += &format!(
            "#{child}=SHAPE_ASPECT('land','',#4,.T.);
#{}=GEOMETRIC_ITEM_SPECIFIC_USAGE('','',#{child},#10,#{face});
#{}=SHAPE_ASPECT_RELATIONSHIP('','datum feature',#1100,#{child});
#{}=DIMENSIONAL_SIZE(#{child},'diameter');
#{}=DIMENSIONAL_CHARACTERISTIC_REPRESENTATION(#{},#{});
#{}=SHAPE_DIMENSION_REPRESENTATION('',(#{}),#399);\n",
            child + 1,
            child + 2,
            child + 3,
            child + 4,
            child + 3,
            child + 5,
            child + 5,
            child + 6,
        );
        extra += &measure(child + 6, "nominal value", "19.");
    }
    let (r, _, parts) = fixture(&extra);
    let p = &r.parts[0];
    let union = p
        .datums
        .iter()
        .find(|d| d.label().as_str() == "Z")
        .unwrap()
        .feature()
        .unwrap();
    let mut children = Vec::new();
    let mut faces = Vec::new();
    for (child, face) in [(1110, 17), (1120, 105), (1130, 197)] {
        let dim = r.provenance.parts[0]
            .dimensions
            .iter()
            .position(|ids| ids.contains(&(child + 3)))
            .unwrap();
        let DimensionKind::Size { feature, .. } = p.dimensions[dim].kind else {
            panic!("size");
        };
        assert_ne!(feature, union, "child #{child} is only one land");
        assert!(!children.contains(&feature));
        children.push(feature);
        let anchor = Anchor::Face(pmi::FaceIndex(parts[0].face_index(face).unwrap()));
        assert_eq!(p.features[feature.0], Feature::Items(vec![anchor]));
        assert!(r.provenance.parts[0].features[feature.0].contains(&child));
        faces.push(anchor);
        assert_eq!(
            p.dimensions[dim]
                .nominal
                .as_ref()
                .unwrap()
                .decimal()
                .as_str(),
            "19."
        );
    }
    faces.sort();
    assert_eq!(p.features[union.0], Feature::Items(faces));
    assert!(r.provenance.parts[0].features[union.0].contains(&1100));
}

#[test]
fn child_does_not_alias_a_datum_with_additional_direct_faces() {
    let (r, _, _) = fixture(
        "#1100=DATUM_FEATURE('union','',#4,.T.);
#1101=SHAPE_ASPECT('child','',#4,.T.);
#1102=GEOMETRIC_ITEM_SPECIFIC_USAGE('','',#1101,#10,#17);
#1103=GEOMETRIC_ITEM_SPECIFIC_USAGE('','',#1100,#10,#105);
#1104=SHAPE_ASPECT_RELATIONSHIP('','',#1100,#1101);
#1105=DATUM('','',#4,.F.,'Z');
#1106=SHAPE_ASPECT_RELATIONSHIP('','',#1100,#1105);
#1107=FLATNESS_TOLERANCE('on child','',#1108,#1101);
#1108=LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(0.1),#400);",
    );
    let p = &r.parts[0];
    let datum = p.datums.iter().find(|d| d.label().as_str() == "Z").unwrap();
    let i = r.provenance.parts[0]
        .tolerances
        .iter()
        .position(|ids| ids.contains(&1107))
        .unwrap();
    let ToleranceTarget::Feature(child) = p.tolerances[i].target else {
        panic!("feature");
    };
    assert_ne!(Some(child), datum.feature());
    assert!(matches!(&p.features[child.0], Feature::Items(items) if items.len() == 1));
    assert!(
        matches!(&p.features[datum.feature().unwrap().0], Feature::Items(items) if items.len() == 2)
    );
}

#[test]
fn unreferenced_datum_constituents_also_keep_their_own_features() {
    // Without an establishing datum, the aspect scan requests #1101 before #1200.
    // Resolving its parent must neither lose the child nor register it twice.
    for established in [true, false] {
        let mut extra = String::from(
            "#1200=DATUM_FEATURE('union','',#4,.T.);
#1101=SHAPE_ASPECT('first face','',#4,.T.);
#1102=SHAPE_ASPECT('second face','',#4,.T.);
#1103=GEOMETRIC_ITEM_SPECIFIC_USAGE('','',#1101,#10,#17);
#1104=GEOMETRIC_ITEM_SPECIFIC_USAGE('','',#1102,#10,#105);
#1105=SHAPE_ASPECT_RELATIONSHIP('','',#1200,#1101);
#1106=SHAPE_ASPECT_RELATIONSHIP('','',#1200,#1102);",
        );
        if established {
            extra += "#1107=DATUM('','',#4,.F.,'Z');
#1108=SHAPE_ASPECT_RELATIONSHIP('','',#1200,#1107);";
        }
        let (r, _, _) = fixture(&extra);
        for child in [1101, 1102] {
            let own: Vec<_> = r.provenance.parts[0]
                .features
                .iter()
                .enumerate()
                .filter(|(_, ids)| ids.contains(&child) && !ids.contains(&1200))
                .collect();
            assert_eq!(own.len(), 1, "child #{child}, established={established}");
            let i = own[0].0;
            assert!(matches!(&r.parts[0].features[i], Feature::Items(items) if items.len() == 1));
        }
    }
}

#[test]
fn shared_face_carriers_keep_group_members_distinct_from_a_datum_union() {
    // GISU UR1 makes the writer share face carriers between an indivisible datum
    // and a per-member group. Those carriers cannot alias the datum's whole union.
    let (r, _, _) = fixture(
        "#1100=DATUM_FEATURE('union','',#4,.T.);
#1101=SHAPE_ASPECT('first face','',#4,.T.);
#1102=SHAPE_ASPECT('second face','',#4,.T.);
#1103=GEOMETRIC_ITEM_SPECIFIC_USAGE('','',#1101,#10,#17);
#1104=GEOMETRIC_ITEM_SPECIFIC_USAGE('','',#1102,#10,#105);
#1105=SHAPE_ASPECT_RELATIONSHIP('','',#1100,#1101);
#1106=SHAPE_ASPECT_RELATIONSHIP('','',#1100,#1102);
#1107=DATUM('','',#4,.F.,'Z');
#1108=SHAPE_ASPECT_RELATIONSHIP('','',#1100,#1107);
#1200=COMPOSITE_GROUP_SHAPE_ASPECT('','multiple elements',#4,.T.);
#1201=SHAPE_ASPECT_RELATIONSHIP('','',#1200,#1101);
#1202=SHAPE_ASPECT_RELATIONSHIP('','',#1200,#1102);
#1203=FLATNESS_TOLERANCE('on group','',#1204,#1200);
#1204=LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(0.1),#400);",
    );
    let p = &r.parts[0];
    let datum = p.datums.iter().find(|d| d.label().as_str() == "Z").unwrap();
    let Feature::Items(union) = &p.features[datum.feature().unwrap().0] else {
        panic!("datum union");
    };
    assert_eq!(union.len(), 2);
    let i = r.provenance.parts[0]
        .features
        .iter()
        .position(|ids| ids.contains(&1200))
        .unwrap();
    let Feature::Group { members, .. } = &p.features[i] else {
        panic!("per-member group");
    };
    assert_eq!(members.len(), 2);
    assert_ne!(members[0], members[1]);
    for member in members {
        assert!(matches!(&p.features[member.0], Feature::Items(items) if items.len() == 1));
        assert_ne!(Some(*member), datum.feature());
    }
}

#[test]
fn stc10_named_datum_constituents_do_not_become_seven_duplicate_features() {
    let path = common::fixtures().join("ap242/nist/nist_stc_10_asme1_ap242-e2.stp.gz");
    let (r, _, parts) = read(load(&path));
    assert_eq!(r.parts[0].features.len(), 166);
    // Each SAR goes from a datum feature to one plain face aspect. Sizes and tolerances
    // refer to those aspects; they must retain the existing datum feature identity.
    for (parent, child, label, face) in [
        (113, 2857, "A", 6993),
        (115, 2858, "B", 7230),
        (114, 2856, "C", 7009),
        (119, 2905, "E", 7071),
        (118, 2903, "F", 7070),
        (120, 2915, "G", 7048),
        (121, 2923, "H", 7022),
    ] {
        let matches: Vec<_> = r.provenance.parts[0]
            .features
            .iter()
            .enumerate()
            .filter(|(_, ids)| ids.contains(&child))
            .collect();
        assert_eq!(matches.len(), 1, "child #{child}");
        let (i, ids) = matches[0];
        assert!(ids.contains(&parent));
        let datum = r.parts[0]
            .datums
            .iter()
            .find(|d| d.label().as_str() == label)
            .unwrap();
        assert_eq!(datum.feature(), Some(FeatureId(i)));
        assert_eq!(
            r.parts[0].features[i],
            Feature::Items(vec![Anchor::Face(pmi::FaceIndex(
                parts[0].face_index(face).unwrap()
            ))])
        );
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
        let path = if name.starts_with("09-") {
            root.join(name)
        } else {
            common::fixtures().join("ap242/nist/nist_ctc_01_asme1_ap242-e1.stp.gz")
        };
        let (r, _, parts) = read(load(&path));
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
            let callout = r.callouts.iter().find(|c| c.id == 258).unwrap();
            assert!(
                callout.links.iter().flat_map(|l| &l.targets).any(|t| {
                    t.part == PartId(0) && t.item == pmi::PresentedItem::Feature(feature)
                }),
                "{callout:?}"
            );
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
