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
