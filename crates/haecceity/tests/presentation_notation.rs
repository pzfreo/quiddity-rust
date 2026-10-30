//! Requirement-level content checks from Draftwright's tolerance and control-frame cases.
#![cfg(feature = "presentation")]
use haecceity::{pmi::*, presentation};
fn decimal(s: &str) -> Decimal {
    Decimal::parse(s).unwrap()
}
fn mm(s: &str) -> Value {
    Value::length(decimal(s), LengthUnit::Millimetre)
}
fn dimension(tolerance: DimTolerance) -> Dimension {
    Dimension {
        kind: DimensionKind::Size {
            feature: FeatureId(0),
            kind: SizeKind::Diameter,
            path: None,
            angle: None,
        },
        nominal: Some(mm("10.0")),
        tolerance,
        qualifier: None,
        modifiers: vec![],
        principle: None,
    }
}
#[test]
fn dimension_bands_state_the_same_requirement_as_draftwright() {
    for (upper, lower, text) in [
        ("0.2", "-0.2", "⌀10.0 ±0.2"),
        ("0.0762", "-0.0254", "⌀10.0 +0.0762 -0.0254"),
        ("0.09", "0.0", "⌀10.0 +0.09 0"),
        ("0.0", "-0.2", "⌀10.0 0 -0.2"),
        ("-0.014", "-0.039", "⌀10.0 -0.014 -0.039"),
        ("0.039", "0.014", "⌀10.0 +0.039 +0.014"),
    ] {
        let d = dimension(DimTolerance::Deviations(
            Bounds::new(mm(upper), mm(lower)).unwrap(),
        ));
        assert_eq!(presentation::dimension_text(&d), text);
        assert_eq!(presentation::dimension(&d).runs[0].text, text);
    }
}
#[test]
fn stated_precision_fit_classes_and_limits_are_not_sheet_rounded() {
    let d = dimension(DimTolerance::Deviations(
        Bounds::new(mm("0.0000100"), mm("-0.0000100")).unwrap(),
    ));
    assert_eq!(presentation::dimension_text(&d), "⌀10.0 ±0.0000100");
    let class = Iso286Class {
        deviation: FundamentalDeviation::parse("H").unwrap(),
        grade: ToleranceGrade::It7,
    };
    assert_eq!(
        presentation::dimension_text(&dimension(DimTolerance::Fit {
            class,
            limits: None
        })),
        "⌀10.0 H7"
    );
    let limits = dimension(DimTolerance::Limits(
        Bounds::new(mm("10.015"), mm("10.000")).unwrap(),
    ));
    assert_eq!(presentation::dimension_text(&limits), "⌀10.000 - ⌀10.015");
}
#[test]
fn basic_and_reference_dimensions_have_their_graphic_conventions() {
    let d = dimension(DimTolerance::Basic);
    assert_eq!(presentation::dimension(&d).strokes.len(), 4);
    let mut d = dimension(DimTolerance::None);
    d.modifiers.push(DimensionModifier::Reference);
    assert_eq!(presentation::dimension_text(&d), "(⌀10.0)");
}
#[test]
fn dimensions_choose_the_cax_if_type_and_real_prefix() {
    for (kind, prefix, name) in [
        (SizeKind::Diameter, "⌀", "diameter dimension"),
        (SizeKind::SphericalDiameter, "S⌀", "diameter dimension"),
        (SizeKind::Radius, "R", "radial dimension"),
        (SizeKind::SphericalRadius, "SR", "radial dimension"),
        (SizeKind::Thickness, "", "linear dimension"),
    ] {
        let mut d = dimension(DimTolerance::None);
        if let DimensionKind::Size { kind: k, .. } = &mut d.kind {
            *k = kind;
        }
        assert_eq!(presentation::dimension_type(&d), name);
        assert_eq!(presentation::dimension_text(&d), format!("{prefix}10.0"));
        assert!(!presentation::dimension(&d).polylines().is_empty());
    }
    let mut d = dimension(DimTolerance::None);
    d.kind = DimensionKind::Size {
        feature: FeatureId(0),
        kind: SizeKind::Other("angle".into()),
        path: None,
        angle: Some(AngleSelection::Small),
    };
    d.nominal = Some(Value::angle(decimal("45.0"), AngleUnit::Degree));
    assert_eq!(presentation::dimension_type(&d), "angular dimension");
    assert_eq!(presentation::dimension_text(&d), "45.0°");
}
#[test]
fn deviations_in_another_unit_use_the_nominals_unit_even_at_zero() {
    let upper = Value::length(decimal("0.001"), LengthUnit::Metre);
    let lower = Value::length(decimal("0.0"), LengthUnit::Metre);
    let mut d = dimension(DimTolerance::Deviations(Bounds::new(upper, lower).unwrap()));
    d.nominal = Some(mm("0.0"));
    assert_eq!(presentation::dimension_text(&d), "⌀0.0 +1.0 0");
}

#[test]
fn names_use_full_notation_without_steps_terminal_decimal_point() {
    let mut d = dimension(DimTolerance::Deviations(
        Bounds::new(mm("0.1"), mm("-0.1")).unwrap(),
    ));
    d.kind = DimensionKind::Size {
        feature: FeatureId(0),
        kind: SizeKind::Thickness,
        path: None,
        angle: None,
    };
    d.nominal = Some(mm("3."));
    assert_eq!(presentation::dimension(&d).notation(), "3 ±0.1");
    d.nominal = Some(mm("5."));
    d.tolerance = DimTolerance::None;
    assert_eq!(presentation::dimension(&d).notation(), "5");
    d.kind = dimension(DimTolerance::None).kind;
    d.nominal = Some(mm("3.3"));
    d.tolerance = DimTolerance::Fit {
        class: Iso286Class {
            deviation: FundamentalDeviation::parse("H").unwrap(),
            grade: ToleranceGrade::It11,
        },
        limits: None,
    };
    assert_eq!(presentation::dimension(&d).notation(), "⌀3.3 H11");
    assert_eq!(presentation::tolerance::number(&decimal("3.E2")), "3E2");
}

#[test]
fn qualifiers_and_stated_fit_limits_follow_draftwright_pmi_labels() {
    // Draftwright aaa7a73 src/pmi.rs:1934–1941, dimension_label:3739–3775.
    for (qualifier, suffix) in [
        (Qualifier::Maximum, "MAX"),
        (Qualifier::Minimum, "MIN"),
        (Qualifier::Average, "AVG"),
    ] {
        let mut d = dimension(DimTolerance::None);
        d.qualifier = Some(qualifier);
        assert_eq!(
            presentation::dimension(&d).notation(),
            format!("⌀10.0 {suffix}")
        );
    }
    let d = dimension(DimTolerance::Fit {
        class: Iso286Class {
            deviation: FundamentalDeviation::parse("H").unwrap(),
            grade: ToleranceGrade::It7,
        },
        limits: Some(Bounds::new(mm("10.015"), mm("10.000")).unwrap()),
    });
    assert_eq!(presentation::dimension(&d).notation(), "⌀10.000 - ⌀10.015");
}
fn profile() -> GeometricTolerance {
    GeometricTolerance {
        kind: ToleranceKind::SurfaceProfile,
        target: ToleranceTarget::Feature(FeatureId(0)),
        magnitude: Some(mm("0.1")),
        zone: None,
        modifiers: vec![],
        unit_basis: None,
        maximum: None,
        unequal: None,
        datums: None,
        auxiliary: vec![],
        description: None,
    }
}
#[test]
fn scope_rings_follow_draftwright_at_the_short_leader_attachment() {
    use haecceity::{
        geom::{self, Bounds as Box3},
        presentation::placement::Layout,
    };
    let mut p = PartPmi {
        features: vec![
            Feature::Group {
                kind: GroupKind::AllAround,
                members: vec![FeatureId(1)],
            },
            Feature::Items(vec![]),
        ],
        ..PartPmi::default()
    };
    for (target, auxiliary, rings) in [
        (ToleranceTarget::Feature(FeatureId(0)), vec![], 1),
        (
            ToleranceTarget::Feature(FeatureId(1)),
            vec![AuxiliaryClassification::AllOver],
            2,
        ),
        (ToleranceTarget::WholePart, vec![], 2),
    ] {
        let mut t = profile();
        t.target = target;
        t.auxiliary = auxiliary;
        let c = presentation::tolerance(&t, &p);
        assert_eq!(c.notation(), "⌓ 0.1"); // Scope is graphical, as in Draftwright's runs.
        assert_eq!(c.leader_rings, rings);
        let mut layout = Layout::new(Box3 {
            min: [-10.; 3],
            max: [10.; 3],
        });
        let draw = layout.place([0.; 3], Some([1., 0., 0.]), &c);
        let leader = draw.lines.last().unwrap();
        let elbow = leader[1];
        let actual = &draw.lines[draw.lines.len() - 1 - rings as usize..draw.lines.len() - 1];
        for (i, ring) in actual.iter().enumerate() {
            assert_eq!(ring.len(), 33);
            assert_eq!(ring[0], ring[32]);
            let r = 0.7 * 2.7 * (20. / 125. / 3.) * if i == 0 { 1. } else { 1.7 };
            for q in ring {
                assert!((geom::dist(*q, elbow) - r).abs() < 2e-6);
            }
        }
    }
    // Traverse scope through a dimension target and nested derived/group membership too.
    p.dimensions.push(dimension(DimTolerance::None));
    let mut t = profile();
    t.target = ToleranceTarget::Dimension(DimensionId(0));
    assert_eq!(presentation::tolerance(&t, &p).leader_rings, 1);
}
#[test]
fn unprinted_draftwright_fields_remain_semantic_only() {
    // The read-only upstream rejects these in pmi.rs blockers and pmi_lowering.rs
    // SCOPE/QUALIFIERS. Do not invent notation it does not draw. The complete list is
    // documented in docs/step-ap242.md, rather than claiming these fields are rendered.
    let expected = "⌀10.0";
    for (_, m) in DimensionModifier::TABLE {
        let mut d = dimension(DimTolerance::None);
        d.modifiers.push(m);
        assert_eq!(presentation::dimension(&d).notation(), expected);
    }
    for principle in [Principle::Independency, Principle::EnvelopeRequirement] {
        let mut d = dimension(DimTolerance::None);
        d.principle = Some(principle);
        assert_eq!(presentation::dimension(&d).notation(), expected);
    }
    let mut d = dimension(DimTolerance::None);
    d.qualifier = Some(Qualifier::Other("arbitrary".into()));
    assert_eq!(presentation::dimension(&d).notation(), expected);
    let p = PartPmi {
        features: vec![Feature::Items(vec![])],
        ..PartPmi::default()
    };
    for (_, m) in ToleranceModifier::ALL {
        let mut t = profile();
        t.modifiers.push(m);
        let expected = match m {
            ToleranceModifier::MaximumMaterialRequirement => "⌓ 0.1Ⓜ",
            ToleranceModifier::LeastMaterialRequirement => "⌓ 0.1Ⓛ",
            _ => "⌓ 0.1",
        };
        assert_eq!(
            presentation::tolerance(&t, &p).notation(),
            expected,
            "{m:?}"
        );
    }
    for field in ["projected", "unequal", "unit_basis", "maximum"] {
        let mut t = profile();
        match field {
            "projected" => {
                t.zone = Some(Zone {
                    form: ZoneForm::BetweenTwoEquidistantComplexSurfacesOrTwoParallelPlanes,
                    projected: Some(ProjectedZone {
                        end: None,
                        length: mm("10.0"),
                    }),
                    non_uniform: None,
                    runout_angle: None,
                    affected_plane: None,
                    affected_plane_kind: None,
                })
            }
            "unequal" => t.unequal = Some(mm("0.02")),
            "unit_basis" => {
                t.unit_basis = Some(UnitBasis {
                    size: mm("100.0"),
                    area: None,
                })
            }
            _ => t.maximum = Some(mm("0.5")),
        }
        assert_eq!(
            presentation::tolerance(&t, &p).notation(),
            "⌓ 0.1",
            "{field}"
        );
    }
    let mut p = p;
    p.datums
        .push(Datum::new(DatumLabel::new("A").unwrap(), Some(FeatureId(0)), vec![]).unwrap());
    for at_reference in [true, false] {
        let mut t = profile();
        let modifier = DatumModifier::Simple(SimpleDatumModifier::MaximumMaterialRequirement);
        t.datums = Some(
            DatumSystem::new(vec![Compartment {
                references: vec![DatumReference {
                    datum: DatumId(0),
                    modifiers: if at_reference {
                        vec![modifier.clone()]
                    } else {
                        vec![]
                    },
                }],
                modifiers: if at_reference { vec![] } else { vec![modifier] },
            }])
            .unwrap(),
        );
        assert_eq!(presentation::tolerance(&t, &p).notation(), "⌓ 0.1 A");
    }
}
