//! Derived axes and median planes, with real-file evidence and conservative refusals.
mod common;

use std::io::Read;

use haecceity::p21::Document;
use haecceity::pmi::{self, *};
use haecceity::step::read_part_definitions;

fn grm() -> String {
    let b = std::fs::read(
        common::fixtures().join("ap242/derived/GRM-03_thumbwheel_drive_screw_AP242_PMI.step.gz"),
    )
    .unwrap();
    let mut out = String::new();
    flate2::read::GzDecoder::new(b.as_slice())
        .read_to_string(&mut out)
        .unwrap();
    out
}

fn read(text: &str) -> PmiRead {
    let parts = read_part_definitions(text.as_bytes()).unwrap();
    let doc = Document::parse(text.as_bytes().to_vec()).unwrap();
    let r = pmi::read(&doc, &parts).unwrap();
    for p in &r.parts {
        p.validate().unwrap();
    }
    assert_eq!(
        r.accounting.semantic,
        r.accounting.consumed + r.accounting.reported
    );
    r
}

fn replace(text: &str, id: u64, record: &str) -> String {
    let doc = Document::parse(text.as_bytes().to_vec()).unwrap();
    let mut text = text.to_string();
    text.replace_range(doc.span(id).unwrap(), record);
    text
}

fn append(text: &str, extra: &str) -> String {
    let at = text.rfind("ENDSEC;").unwrap();
    format!("{}{extra}\n{}", &text[..at], &text[at..])
}

fn feature(r: &PmiRead, aspect: u64) -> FeatureId {
    FeatureId(
        r.provenance.parts[0]
            .features
            .iter()
            .position(|p| p.contains(&aspect))
            .unwrap(),
    )
}

fn geometry(r: &PmiRead, aspect: u64) -> Option<&DerivedGeometry> {
    match &r.parts[0].features[feature(r, aspect).0] {
        Feature::Derived { geometry, .. } => geometry.map(|g| &r.parts[0].derived_geometry[g.0]),
        f => panic!("#{aspect}: expected derived feature, got {f:?}"),
    }
}

fn dimension(r: &PmiRead, id: u64) -> &Dimension {
    let i = r.provenance.parts[0]
        .dimensions
        .iter()
        .position(|p| p.contains(&id))
        .unwrap();
    &r.parts[0].dimensions[i]
}

#[test]
fn grm_all_ten_dimensions_resolve_and_match_unchanged_nominals_and_qualifiers() {
    let r = read(&grm());
    assert_eq!(r.parts[0].dimensions.len(), 10);
    assert_eq!(r.parts[0].derived_geometry.len(), 15);
    let mut refused = 0;
    for (id, aspects, nominal) in [
        (841, vec![835], "4."),
        (877, vec![871], "6."),
        (914, vec![907], "10."),
        (958, vec![952], "5."),
        (994, vec![988], "1.6"),
        (1040, vec![1030, 1034], "3.2"),
        (1086, vec![1076, 1080], "0.5"),
        (1132, vec![1122, 1126], "2."),
        (1174, vec![1164, 1168], "3."),
        (1216, vec![1206, 1210], "20."),
    ] {
        let d = dimension(&r, id);
        let expected = Length {
            value: Decimal::parse(nominal).unwrap(),
            unit: LengthUnit::Millimetre,
        };
        assert_eq!(
            d.nominal.as_ref().unwrap().quantity,
            Quantity::Length(expected.clone()),
            "#{id}"
        );
        assert_eq!(
            d.qualifier,
            if id == 914 {
                Some(Qualifier::Maximum)
            } else {
                None
            },
            "#{id}"
        );
        if aspects.iter().any(|&a| geometry(&r, a).is_none()) {
            refused += 1;
            continue;
        }
        let a = geometry(&r, aspects[0]).unwrap();
        let measured = if aspects.len() == 1 {
            assert_eq!(a.kind, DerivedGeometryKind::Axis);
            assert_eq!(a.supplemental.len(), 1);
            assert!(
                matches!(d.kind, DimensionKind::Size { feature: f, kind: SizeKind::Diameter, .. } if f == feature(&r, aspects[0]))
            );
            a.radius.as_ref().unwrap().mm() * 2.
        } else {
            let b = geometry(&r, aspects[1]).unwrap();
            assert!(
                matches!(d.kind, DimensionKind::Location { from, to, kind: LocationKind::LinearDistance, .. } if from == feature(&r, aspects[0]) && to == feature(&r, aspects[1]))
            );
            assert_eq!(a.kind, DerivedGeometryKind::MedianPlane);
            assert_eq!(b.kind, DerivedGeometryKind::MedianPlane);
            assert_eq!(a.deriving_faces.len(), 1);
            assert_eq!(b.deriving_faces.len(), 1);
            let n = a.direction.0.each_ref().map(Decimal::to_f64);
            let m = b.direction.0.each_ref().map(Decimal::to_f64);
            let dot: f64 = n.iter().zip(m).map(|(x, y)| x * y).sum();
            assert!((dot.abs() - 1.).abs() < 1e-12);
            a.point
                .iter()
                .zip(&b.point)
                .zip(n)
                .map(|((p, q), n)| (q.mm() - p.mm()) * n)
                .sum::<f64>()
                .abs()
        };
        assert!(
            (measured - expected.mm()).abs() < 1e-7,
            "#{id}: geometry {measured}, nominal {nominal}"
        );
    }
    assert_eq!(refused, 0, "{:?}", r.findings);
    assert!(
        !r.findings
            .iter()
            .any(|f| f.detail.contains("derived geometry refused")
                || f.detail.contains("plane-plane distance refused"))
    );
}

fn agreeing() -> String {
    replace(&grm(), 836, "#836=CARTESIAN_POINT('',(-1.6,0.,0.));")
}

#[test]
fn cylinder_axis_uses_faces_not_supplemental_point_and_keeps_qualifiers() {
    let text = agreeing();
    let r = read(&text);
    let g = geometry(&r, 835).unwrap();
    assert_eq!(g.kind, DerivedGeometryKind::Axis);
    assert_eq!(g.radius.as_ref().unwrap().mm(), 2.);
    assert_eq!(
        g.point[0].mm(),
        -3.2,
        "source face's axis, not the explicit point"
    );
    assert_eq!(g.supplemental.len(), 1);
    let parts = read_part_definitions(text.as_bytes()).unwrap();
    assert_eq!(
        g.deriving_faces,
        vec![FaceIndex(parts[0].face_index(573).unwrap())]
    );
    assert!(!r.findings.iter().any(|f| f.ids[0] == 835));
    assert_eq!(dimension(&r, 914).qualifier, Some(Qualifier::Maximum));
    let qualified = read(&replace(&text, 908, "#908=CARTESIAN_POINT('',(1.,0.,0.));"));
    assert!(geometry(&qualified, 907).is_some());
    assert_eq!(
        dimension(&qualified, 914).qualifier,
        Some(Qualifier::Maximum)
    );
    // Removing the explicit usage leaves the same computed geometry.
    let no_explicit = replace(
        &text,
        834,
        "#834=DESCRIPTIVE_REPRESENTATION_ITEM('unused','');",
    );
    let r2 = read(&no_explicit);
    let g2 = geometry(&r2, 835).unwrap();
    assert_eq!(
        (&g.point, &g.direction, &g.radius),
        (&g2.point, &g2.direction, &g2.radius)
    );
}

#[test]
fn chains_and_distance_targets_resolve_axis_axis_axis_plane_and_plane_plane() {
    let text = append(
        &agreeing(),
        "
#3000=DERIVED_SHAPE_ASPECT('median A','',#4,.F.);
#3001=SHAPE_ASPECT_DERIVING_RELATIONSHIP('',$,#3000,#1025);
#3002=SHAPE_ASPECT_DERIVING_RELATIONSHIP('',$,#3000,#1027);
#3003=DERIVED_SHAPE_ASPECT('median B','',#4,.F.);
#3004=SHAPE_ASPECT_DERIVING_RELATIONSHIP('',$,#3003,#1071);
#3005=SHAPE_ASPECT_DERIVING_RELATIONSHIP('',$,#3003,#1073);
#3006=DERIVED_SHAPE_ASPECT('chained axis','',#4,.F.);
#3007=SHAPE_ASPECT_DERIVING_RELATIONSHIP('',$,#3006,#835);
#3010=DIMENSIONAL_LOCATION('linear distance','',#835,#3006);
#3011=DIMENSIONAL_LOCATION('linear distance','',#835,#3000);
#3012=DIMENSIONAL_LOCATION('linear distance','',#3000,#3003);
#3020=DIMENSIONAL_CHARACTERISTIC_REPRESENTATION(#3010,#839);
#3021=DIMENSIONAL_CHARACTERISTIC_REPRESENTATION(#3011,#839);
#3022=DIMENSIONAL_CHARACTERISTIC_REPRESENTATION(#3012,#839);
#3030=LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(0.01),#770);
#3031=STRAIGHTNESS_TOLERANCE('','',#3030,#3006);
",
    );
    let r = read(&text);
    assert_eq!(
        geometry(&r, 3000).unwrap().kind,
        DerivedGeometryKind::MedianPlane
    );
    assert!((geometry(&r, 3000).unwrap().point[0].mm() + 1.6).abs() < 1e-12);
    assert_eq!(geometry(&r, 3000).unwrap().deriving_faces.len(), 2);
    assert!((geometry(&r, 3003).unwrap().point[0].mm() - 0.25).abs() < 1e-12);
    assert_eq!(
        geometry(&r, 3006).unwrap().deriving_faces,
        geometry(&r, 835).unwrap().deriving_faces
    );
    for id in [3010, 3011, 3012] {
        let DimensionKind::Location { from, to, .. } = dimension(&r, id).kind else {
            panic!()
        };
        for f in [from, to] {
            assert!(matches!(
                r.parts[0].features[f.0],
                Feature::Derived {
                    geometry: Some(_),
                    ..
                }
            ));
        }
    }
    let i = r.provenance.parts[0]
        .tolerances
        .iter()
        .position(|p| p.contains(&3031))
        .unwrap();
    assert_eq!(
        r.parts[0].tolerances[i].target,
        ToleranceTarget::Feature(feature(&r, 3006))
    );
}

#[test]
fn explicit_axis_placements_lines_and_planes_are_read_and_cross_checked() {
    for record in [
        "#836=AXIS1_PLACEMENT('',#564,#565);",
        "#836=AXIS2_PLACEMENT_3D('',#564,#565,#566);",
        "#836=LINE('',#564,#3000);",
    ] {
        let text = append(
            &replace(&grm(), 836, record),
            "
#3000=VECTOR('',#565,1.);
#3001=DIRECTION('',(0.,1.,0.));
#3002=CARTESIAN_POINT('',(0.,0.,0.));
#3003=AXIS2_PLACEMENT_3D('',#3002,#3001,$);
",
        );
        let r = read(&text);
        assert!(geometry(&r, 835).is_some(), "{record}: {:?}", r.findings);
    }
    let text = replace(&grm(), 836, "#836=AXIS2_PLACEMENT_3D('',#564,#566,$);");
    let r = read(&text);
    assert!(geometry(&r, 835).is_none());
    assert!(
        r.findings
            .iter()
            .any(|f| f.ids[0] == 835 && f.detail.contains("direction/normal disagreement"))
    );
}

#[test]
fn explicit_only_cycles_and_disagreeing_chains_are_refused() {
    let only = replace(
        &agreeing(),
        837,
        "#837=DESCRIPTIVE_REPRESENTATION_ITEM('unused','');",
    );
    let r = read(&only);
    assert!(
        r.findings
            .iter()
            .any(|f| f.ids[0] == 835 && f.detail.contains("explicit geometry alone"))
    );
    let chained = append(
        &replace(&grm(), 836, "#836=CARTESIAN_POINT('',(-1.6,0.,1.));"),
        "
#3000=DERIVED_SHAPE_ASPECT('','',#4,.F.);
#3001=SHAPE_ASPECT_DERIVING_RELATIONSHIP('',$,#3000,#835);
",
    );
    let r = read(&chained);
    assert!(geometry(&r, 3000).is_none());
    let cycle = replace(
        &agreeing(),
        837,
        "#837=SHAPE_ASPECT_DERIVING_RELATIONSHIP('',$,#835,#835);",
    );
    let r = read(&cycle);
    assert!(
        r.findings
            .iter()
            .any(|f| f.ids[0] == 835 && f.detail.contains("base does not resolve"))
    );
}

#[test]
fn cone_and_torus_give_axes_without_cylinder_radius() {
    for record in [
        "#562=CONICAL_SURFACE('',#563,2.,0.4);",
        "#562=TOROIDAL_SURFACE('',#563,2.,0.5);",
    ] {
        let text = append(
            &replace(&agreeing(), 562, record),
            "
#3000=AXIS1_PLACEMENT('',#564,#565);
#3001=LINE('',#564,#3002);
#3002=VECTOR('',#566,1.);
",
        );
        let r = read(&text);
        let g = geometry(&r, 835).unwrap_or_else(|| panic!("{record}: {:?}", r.findings));
        assert_eq!(g.kind, DerivedGeometryKind::Axis);
        assert!(g.radius.is_none());
    }
}

#[test]
fn supplemental_median_planes_and_placements_must_match_the_median() {
    for item in [
        "#3010=PLANE('',#3011);",
        "#3010=AXIS2_PLACEMENT_3D('',#3012,#565,$);",
    ] {
        for (x, agrees) in [(-1.6, true), (0., false)] {
            let text = append(
                &grm(),
                &format!(
                    "
#3000=DERIVED_SHAPE_ASPECT('median','',#4,.F.);
#3001=SHAPE_ASPECT_DERIVING_RELATIONSHIP('',$,#3000,#1025);
#3002=SHAPE_ASPECT_DERIVING_RELATIONSHIP('',$,#3000,#1027);
#3003=CONSTRUCTIVE_GEOMETRY_REPRESENTATION('',(#3010),#769);
#3004=GEOMETRIC_ITEM_SPECIFIC_USAGE('','',#3000,#3003,#3010);
{item}
#3011=AXIS2_PLACEMENT_3D('',#3012,#565,$);
#3012=CARTESIAN_POINT('',({x},0.,0.));
"
                ),
            );
            let r = read(&text);
            assert_eq!(
                geometry(&r, 3000).is_some(),
                agrees,
                "{item}: {:?}",
                r.findings
            );
        }
    }
}

#[test]
fn supplemental_units_are_converted_and_uncertainty_is_observed() {
    let text = replace(&grm(), 564, "#564=CARTESIAN_POINT('',(-3.2,25.4,0.));");
    let text = replace(&text, 836, "#836=CARTESIAN_POINT('',(-1.6,1.,0.));");
    let text = replace(
        &text,
        1992,
        "#1992=CONSTRUCTIVE_GEOMETRY_REPRESENTATION('',(#836),#3003);",
    );
    let text = append(&text, "
#3000=(LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT($,.METRE.));
#3001=LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(0.0254),#3000);
#3002=(CONVERSION_BASED_UNIT('INCH',#3001) LENGTH_UNIT() NAMED_UNIT(#3004));
#3003=(GEOMETRIC_REPRESENTATION_CONTEXT(3) GLOBAL_UNIT_ASSIGNED_CONTEXT((#3002,#771,#772)) REPRESENTATION_CONTEXT('inch',''));
#3004=DIMENSIONAL_EXPONENTS(1.,0.,0.,0.,0.,0.,0.);
");
    let r = read(&text);
    assert_eq!(geometry(&r, 835).unwrap().point[1].mm(), 25.4);
    let r = read(&replace(
        &text,
        836,
        "#836=CARTESIAN_POINT('',(-1.6,1.001,0.));",
    ));
    assert!(geometry(&r, 835).is_none());
    // The file's length uncertainty is 1e-7 mm. A larger stated uncertainty is honoured.
    let text = replace(
        &agreeing(),
        836,
        "#836=CARTESIAN_POINT('',(-1.6,0.00001,0.));",
    );
    assert!(geometry(&read(&text), 835).is_none());
    let text = replace(
        &text,
        773,
        "#773=UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(0.0001),#770,'distance_accuracy_value','confusion accuracy');",
    );
    assert!(geometry(&read(&text), 835).is_some());
}

#[test]
fn writer_preserves_cross_checked_supplemental_geometry_and_verifies_computed_records() {
    use pmi::write::{Mode, PresentationPolicy, differences_as_stated};
    let text = agreeing();
    let before = read(&text);
    let source = &before.parts[0];
    let g = geometry(&before, 835).unwrap();
    let mut g = g.clone();
    g.supplemental = vec![GeometryId(0)];
    let mut dim = dimension(&before, 841).clone();
    let DimensionKind::Size { feature, .. } = &mut dim.kind else {
        panic!()
    };
    *feature = FeatureId(1);
    let p = PartPmi {
        features: vec![
            Feature::Items(vec![Anchor::Face(g.deriving_faces[0])]),
            Feature::Derived {
                kind: DerivedKind::Derived,
                from: vec![FeatureId(0)],
                geometry: Some(DerivedGeometryId(0)),
            },
        ],
        derived_geometry: vec![g],
        geometry: vec![source.geometry[geometry(&before, 835).unwrap().supplemental[0].0].clone()],
        dimensions: vec![dim],
        ..PartPmi::default()
    };
    let parts = read_part_definitions(text.as_bytes()).unwrap();
    let doc = Document::parse(text.into_bytes()).unwrap();
    let (edit, _) = pmi::write(
        &doc,
        &parts,
        &[(PartId(0), p.clone())],
        Mode::Add,
        PresentationPolicy::RemovePresentation,
    )
    .unwrap();
    let bytes = doc.apply(&edit).unwrap().bytes;
    let after = read(std::str::from_utf8(&bytes).unwrap());
    let mut expected = p.derived_geometry[0].clone();
    expected.supplemental.clear();
    let matching: Vec<_> = after.parts[0]
        .derived_geometry
        .iter()
        .filter(|g| {
            let mut computed = (*g).clone();
            computed.supplemental.clear();
            computed == expected
        })
        .collect();
    assert_eq!(matching.len(), 2);
    for g in matching {
        assert_eq!(g.supplemental.len(), 1);
        assert_eq!(after.parts[0].geometry[g.supplemental[0].0], p.geometry[0]);
    }
    let mut changed = p.clone();
    changed.derived_geometry[0].point[1].value = Decimal::parse("1").unwrap();
    assert!(
        !differences_as_stated(&p, &changed).is_empty(),
        "verification must include computed geometry"
    );
}

#[test]
fn arbitrary_surfaces_of_revolution_remain_refused() {
    let text = append(
        &replace(
            &agreeing(),
            562,
            "#562=SURFACE_OF_REVOLUTION('',#3001,#3000);",
        ),
        "
#3000=AXIS1_PLACEMENT('',#564,#565);
#3001=LINE('',#564,#3002);
#3002=VECTOR('',#566,1.);
",
    );
    let r = read(&text);
    assert!(geometry(&r, 835).is_none());
    assert!(r.findings.iter().any(|f| {
        f.ids[0] == 835
            && f.detail
                .contains("unsupported surface #562 (surface_of_revolution)")
    }));
}

#[test]
fn supplemental_membership_includes_nested_geometry_and_rejects_unrelated_items() {
    let text = append(
        &replace(
            &agreeing(),
            1992,
            "#1992=CONSTRUCTIVE_GEOMETRY_REPRESENTATION('',(#3000),#769);",
        ),
        "#3000=GEOMETRIC_SET('',(#836));",
    );
    assert!(geometry(&read(&text), 835).is_some());
    let text = replace(&text, 3000, "#3000=CARTESIAN_POINT('',(0.,0.,0.));");
    let r = read(&text);
    assert!(geometry(&r, 835).is_none());
    assert!(
        r.findings
            .iter()
            .any(|f| f.ids[0] == 835 && f.detail.contains("not a member"))
    );
}

#[test]
fn a_looser_explicit_context_cannot_relax_a_different_items_cross_check() {
    // Both points have the same 0.001 mm distance from the axis. Only the first
    // belongs to the context with 0.01 mm uncertainty; the second must be refused.
    let text = append(&replace(&agreeing(), 836, "#836=CARTESIAN_POINT('',(-1.6,0.001,0.));"), "
#3000=(GEOMETRIC_REPRESENTATION_CONTEXT(3) GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#3001)) GLOBAL_UNIT_ASSIGNED_CONTEXT((#770,#771,#772)) REPRESENTATION_CONTEXT('loose',''));
#3001=UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(0.01),#770,'distance_accuracy_value','');
#3002=CONSTRUCTIVE_GEOMETRY_REPRESENTATION('',(#836),#3000);
#3003=CARTESIAN_POINT('',(-1.6,0.001,0.));
#3004=CONSTRUCTIVE_GEOMETRY_REPRESENTATION('',(#3003),#769);
#3005=GEOMETRIC_ITEM_SPECIFIC_USAGE('','',#835,#3004,#3003);
");
    let text = replace(
        &text,
        834,
        "#834=GEOMETRIC_ITEM_SPECIFIC_USAGE('','',#835,#3002,#836);",
    );
    let r = read(&text);
    assert!(geometry(&r, 835).is_none());
    assert!(r.findings.iter().any(|f| f.ids[0] == 835
        && f.detail.contains("#3003: explicit geometry disagrees")
        && f.detail.contains("tolerance 1.000000000000e-7 mm")));
}

#[test]
fn a_point_off_both_axis_and_deriving_surface_keeps_the_conflict_finding() {
    let r = read(&replace(
        &grm(),
        836,
        "#836=CARTESIAN_POINT('',(-1.6,0.,1.));",
    ));
    assert!(geometry(&r, 835).is_none());
    assert!(dimension(&r, 841).nominal.is_some());
    assert!(r.findings.iter().any(|f| {
        f.kind == FindingKind::Conflict
            && f.ids[0] == 835
            && f.ids.contains(&836)
            && f.ids.contains(&573)
            && f.detail
                .contains("explicit geometry disagrees with derived axis")
    }));
}

#[test]
fn non_parallel_single_plane_aspects_refuse_the_distance_and_keep_the_planes() {
    // #1030 and #1034 each derive exactly one plane. Rotate only the second
    // normal; its explicit point is still on that plane, so both aspects resolve.
    let r = read(&replace(&grm(), 525, "#525=DIRECTION('',(0.,1.,0.));"));
    for aspect in [1030, 1034] {
        let g = geometry(&r, aspect).unwrap();
        assert_eq!(g.kind, DerivedGeometryKind::MedianPlane);
        assert_eq!(g.deriving_faces.len(), 1);
    }
    assert_eq!(
        dimension(&r, 1040).nominal.as_ref().unwrap().quantity.si(),
        3.2
    );
    assert!(r.findings.iter().any(|f| {
        f.kind == FindingKind::NotModelled
            && f.ids[0] == 1040
            && f.ids.contains(&1030)
            && f.ids.contains(&1034)
            && f.detail
                .contains("plane-plane distance refused: non-parallel planes")
    }));
}

#[test]
fn surface_points_use_cone_station_and_torus_tube_not_a_constant_radius() {
    for (surface, points) in [
        (
            "#562=CONICAL_SURFACE('',#563,2.,0.7853981633974483);",
            vec![("(-2.2,0.,3.)", true), ("(-2.2,0.,2.)", false)],
        ),
        (
            "#562=TOROIDAL_SURFACE('',#563,2.,0.5);",
            vec![
                ("(-3.2,0.,2.5)", true),
                ("(-3.2,0.,1.5)", true),
                ("(-2.9,0.,2.4)", true),
                ("(-2.9,0.,1.6)", true),
                ("(-2.7,0.,2.)", true),
                ("(-2.2,0.,2.5)", false),
                ("(-2.9,0.,2.5)", false),
            ],
        ),
        (
            "#562=TOROIDAL_SURFACE('',#563,0.5,1.);",
            vec![("(-3.2,0.,0.5)", true), ("(-3.2,0.,0.75)", false)],
        ),
    ] {
        let text = replace(&grm(), 562, surface);
        for (point, agrees) in points {
            let r = read(&replace(
                &text,
                836,
                &format!("#836=CARTESIAN_POINT('',{point});"),
            ));
            assert_eq!(
                geometry(&r, 835).is_some(),
                agrees,
                "{surface}: {point}: {:?}",
                r.findings
            );
        }
    }
    // The cone angle must be converted from its context, just like length units.
    let text = replace(&grm(), 562, "#562=CONICAL_SURFACE('',#563,2.,45.);");
    let text = replace(
        &text,
        771,
        "#771=(CONVERSION_BASED_UNIT('degree',#3000) NAMED_UNIT(#3002) PLANE_ANGLE_UNIT());",
    );
    let text = append(
        &text,
        "
#3000=PLANE_ANGLE_MEASURE_WITH_UNIT(PLANE_ANGLE_MEASURE(0.017453292519943295),#3001);
#3001=(NAMED_UNIT(*) PLANE_ANGLE_UNIT() SI_UNIT($,.RADIAN.));
#3002=DIMENSIONAL_EXPONENTS(0.,0.,0.,0.,0.,0.,0.);
",
    );
    let r = read(&replace(
        &text,
        836,
        "#836=CARTESIAN_POINT('',(-2.2,0.,3.));",
    ));
    assert!(geometry(&r, 835).is_some(), "{:?}", r.findings);
}

#[test]
fn surface_point_tolerance_tracks_model_uncertainty_without_accepting_arbitrary_offsets() {
    for (offset, agrees) in [(0.00000005, true), (0.0000002, false)] {
        let text = replace(
            &grm(),
            836,
            &format!("#836=CARTESIAN_POINT('',(-1.6,0.,{}));", 2. + offset),
        );
        assert_eq!(geometry(&read(&text), 835).is_some(), agrees);
        let text = replace(
            &text,
            773,
            "#773=UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(0.000001),#770,'distance_accuracy_value','confusion accuracy');",
        );
        assert!(geometry(&read(&text), 835).is_some());
    }
}
