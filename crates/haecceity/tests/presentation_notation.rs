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
fn scope_rings_follow_draftwright_at_a_bend_clear_of_the_frame() {
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
        assert_eq!(leader.len(), 3);
        let elbow = leader[1];
        assert!(geom::dist(elbow, leader[2]) > 2.3 * (0.5 + 0.018 * 20.));
        let actual = &draw.lines[draw.lines.len() - 1 - rings as usize..draw.lines.len() - 1];
        let up = geom::cross(draw.normal, draw.along);
        let frame_near = draw
            .lines
            .iter()
            .take(4)
            .flatten()
            .map(|q| geom::dot(*q, up))
            .fold(f64::INFINITY, f64::min);
        for (i, ring) in actual.iter().enumerate() {
            assert_eq!(ring.len(), 33);
            assert_eq!(ring[0], ring[32]);
            let r = 0.7 * 2.7 * ((0.5 + 0.018 * 20.) / 3.) * if i == 0 { 1. } else { 1.7 };
            for q in ring {
                assert!((geom::dist(*q, elbow) - r).abs() < 2e-6);
                assert!(
                    geom::dot(*q, up) < frame_near - 1e-6,
                    "scope ring must clear the frame"
                );
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

#[test]
fn physical_text_height_is_legible_across_small_and_large_parts_and_units() {
    use haecceity::{
        geom::Bounds,
        presentation::placement::{Layout, text_height},
    };
    for (extent, expected) in [(10., 0.68), (37.6, 1.1768), (100., 2.3), (300., 5.9)] {
        assert!((text_height(extent) - expected).abs() < 1e-12);
        for mm_per_unit in [1., 25.4, 0.001] {
            let bounds = Bounds {
                min: [0.; 3],
                max: [extent / mm_per_unit; 3],
            };
            assert!(
                (Layout::with_unit(bounds, mm_per_unit).text_height() * mm_per_unit - expected)
                    .abs()
                    < 1e-12
            );
        }
    }
}

#[test]
fn surface_texture_uses_draftwrights_iso_1302_legs_bar_and_removal_marks() {
    use presentation::notes;
    let mut t = SurfaceTexture {
        on: Some(FeatureId(0)),
        material_removal: MaterialRemoval::AnyProcessAllowed,
        parameters: vec![
            SurfaceTextureParameter {
                characteristic: "Ra".into(),
                value: Length {
                    value: decimal("3.2"),
                    unit: LengthUnit::Micrometre,
                },
            },
            SurfaceTextureParameter {
                characteristic: "Rz".into(),
                value: Length {
                    value: decimal("0.004"),
                    unit: LengthUnit::Millimetre,
                },
            },
        ],
    };
    assert_eq!(notes::texture_parameters(&t), ["Ra 3.2", "Rz 4"]);
    let basic = notes::surface_texture(&t);
    assert_eq!(basic.strokes.len(), 3);
    assert!(basic.arcs.is_empty());
    // Draftwright's short and long legs are 1.4 and 3 text heights, both at 60 degrees.
    let short = basic.strokes[0];
    let long = basic.strokes[1];
    assert_eq!(short[1], long[0]);
    for leg in [short, long] {
        assert!(
            ((leg[1][1] - leg[0][1]).abs() / (leg[1][0] - leg[0][0]).abs() - 3f64.sqrt()).abs()
                < 1e-12
        );
    }
    assert_eq!(short[0][1], 1.4 * 3.);
    assert_eq!(long[1][1], 3. * 3.);
    assert_eq!(
        basic
            .runs
            .iter()
            .map(|r| r.text.as_str())
            .collect::<Vec<_>>(),
        ["Ra 3.2", "Rz 4"]
    );
    t.material_removal = MaterialRemoval::Required;
    let required = notes::surface_texture(&t);
    assert_eq!(required.strokes.len(), 4);
    assert_eq!(required.strokes[3][0][1], required.strokes[3][1][1]);
    t.material_removal = MaterialRemoval::NotAllowed;
    let prohibited = notes::surface_texture(&t);
    assert_eq!(prohibited.strokes.len(), 3);
    assert_eq!(prohibited.arcs.len(), 1);
    assert!((prohibited.arcs[0][2] - 1.4).abs() < 1e-12);
    for c in [basic, required, prohibited] {
        assert!(c.runs.iter().all(|t| {
            presentation::font()
                .text(&t.text, t.size)
                .missing
                .is_empty()
        }));
    }
}

#[test]
fn part_settings_have_shared_wording_and_indexed_general_notes() {
    use presentation::notes::{self, Source};
    let attribute = |kind: &str, text: &str, index| AttributeSet {
        name: "semantic text".into(),
        on: None,
        description: None,
        general_property: None,
        items: vec![
            (kind.into(), AttributeValue::Text(text.into())),
            ("note index".into(), AttributeValue::Integer(index)),
        ],
    };
    let p = PartPmi {
        material: Some(Material {
            id: "6082-T6".into(),
            name: None,
            density: None,
        }),
        general: vec![GeneralTolerance::Class {
            text: "ISO 2768-m".into(),
            standard: None,
        }],
        surface_textures: vec![SurfaceTexture {
            on: None,
            material_removal: MaterialRemoval::AnyProcessAllowed,
            parameters: vec![SurfaceTextureParameter {
                characteristic: "Ra".into(),
                value: Length {
                    value: decimal("3.2"),
                    unit: LengthUnit::Micrometre,
                },
            }],
        }],
        attributes: vec![
            attribute("general note", "Second note", 2),
            attribute("heat treatment", "T6", 0),
            attribute("edge condition", "Break sharp edges 0.2 max", 0),
            attribute("surface treatment", "Black", 0),
            attribute("general note", "First note", 1),
        ],
        ..PartPmi::default()
    };
    let rows = notes::general_notes(&p);
    assert_eq!(
        rows.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(),
        [
            "MATERIAL: 6082-T6",
            "GENERAL TOLERANCES ISO 2768-m",
            "SURFACE FINISH Ra 3.2 UNLESS OTHERWISE STATED",
            "Break sharp edges 0.2 max",
            "Coating: Black",
            "Heat treatment: T6",
            "First note",
            "Second note"
        ]
    );
    assert_eq!(rows[6].source, Source::Attribute(4));
    assert_eq!(rows[7].source, Source::Attribute(0));
    let block = notes::notes_block(&rows);
    assert_eq!(block.runs[0].text, "GENERAL NOTES");
    assert_eq!(block.runs.len(), rows.len() + 1);
    assert_eq!(block.runs[4].text, "1  Break sharp edges 0.2 max");
    assert_eq!(block.runs[7].text, "4  First note");
    assert_eq!(block.runs[8].text, "5  Second note");
    for (kind, text, expected) in [
        ("surface treatment", "Anodise black", "Anodise black"),
        ("surface treatment", "Zinc plated", "Zinc plated"),
        (
            "heat treatment",
            "Solution treat and age to T6",
            "Solution treat and age to T6",
        ),
        ("heat treatment", "EN8", "Heat treatment: EN8"),
    ] {
        assert_eq!(notes::part_note_text(kind, text), expected);
    }
    assert!(notes::general_notes(&PartPmi::default()).is_empty());
    let mut unindexed = attribute("general note", "Unindexed", 0);
    unindexed.items.retain(|(key, _)| key != "note index");
    let mixed = PartPmi {
        attributes: vec![unindexed, attribute("general note", "Indexed", 1)],
        ..PartPmi::default()
    };
    assert_eq!(
        notes::general_notes(&mixed)
            .iter()
            .map(|r| r.text.as_str())
            .collect::<Vec<_>>(),
        ["Indexed", "Unindexed"]
    );
    let aliases = PartPmi {
        attributes: vec![
            attribute("coating", "Black", 0),
            attribute("edges", "Deburr", 0),
        ],
        ..PartPmi::default()
    };
    assert_eq!(
        notes::general_notes(&aliases)
            .iter()
            .map(|r| r.text.as_str())
            .collect::<Vec<_>>(),
        ["Deburr", "Coating: Black"]
    );
    let mut p = p;
    for a in &mut p.attributes {
        a.on = Some(NoteOwner::Feature(FeatureId(0)));
    }
    p.material = None;
    p.general.clear();
    p.surface_textures[0].on = Some(FeatureId(0));
    assert!(
        notes::general_notes(&p).is_empty(),
        "face settings are not part-level defaults"
    );
}

#[test]
fn editable_note_accents_and_micrometre_units_are_drawn() {
    let ink = presentation::font().text("café Ra 3.2 µm", 3.);
    assert!(ink.missing.is_empty());
    assert!(
        presentation::text::glyph('é').unwrap().1.len()
            > presentation::text::glyph('e').unwrap().1.len()
    );
}

#[test]
fn multiline_general_notes_keep_their_lines_and_clear_neighboring_rows() {
    use presentation::notes::{self, Row, Source};
    let rows = [
        Row {
            source: Source::Attribute(0),
            text: "First line\nSecond line".into(),
        },
        Row {
            source: Source::Attribute(1),
            text: "café".into(),
        },
    ];
    let block = notes::notes_block(&rows);
    assert_eq!(block.runs[1].text, "1  First line\n   Second line");
    for pair in block.letters.windows(2) {
        let a = presentation::font().text(&pair[0].text, pair[0].size);
        let b = presentation::font().text(&pair[1].text, pair[1].size);
        assert!(pair[0].at[1] + a.min[1] > pair[1].at[1] + b.max[1]);
        assert!(
            (pair[0].at[0] + a.min[0] - (pair[1].at[0] + b.min[0])).abs() < 1e-12,
            "left alignment"
        );
    }
}
