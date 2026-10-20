//! Standard writer forms exercised by both write and clean-copy round-trip tests.
use haecceity::pmi::*;

fn d(s: &str) -> Decimal {
    Decimal::parse(s).unwrap()
}
fn mm(s: &str) -> Value {
    Value::length(d(s), LengthUnit::Millimetre)
}
fn face(n: usize) -> Feature {
    Feature::Items(vec![Anchor::Face(FaceIndex(n))])
}
fn item(name: &str, attrs: Vec<GeometryValue>) -> GeometryItem {
    GeometryItem {
        leaves: vec![(name.to_ascii_uppercase(), attrs)],
    }
}

pub fn every_kind() -> PartPmi {
    let gv = |s: &str| GeometryValue::Real(d(s));
    let point = item(
        "cartesian_point",
        vec![
            GeometryValue::Text("".into()),
            GeometryValue::List(vec![gv("0."), gv("0."), gv("0.")]),
        ],
    );
    let plane = item(
        "axis2_placement_3d",
        vec![
            GeometryValue::Text("".into()),
            GeometryValue::Item(Box::new(point)),
            GeometryValue::Unset,
            GeometryValue::Unset,
        ],
    );
    let tolerance = |target, value| GeometricTolerance {
        kind: ToleranceKind::Position,
        target,
        magnitude: Some(mm(value)),
        zone: None,
        modifiers: Vec::new(),
        datums: None,
        unit_basis: None,
        maximum: None,
        unequal: None,
        auxiliary: Vec::new(),
        description: None,
    };
    let mut p = PartPmi {
        features: vec![
            face(0),
            face(1),
            Feature::Items(vec![Anchor::Geometry(GeometryId(0))]),
        ],
        geometry: vec![Geometry {
            item: plane,
            length_unit: Some(LengthUnit::Millimetre),
        }],
        tolerances: vec![
            tolerance(ToleranceTarget::Feature(FeatureId(0)), "0.2"),
            tolerance(ToleranceTarget::Feature(FeatureId(0)), "0.1"),
            tolerance(
                ToleranceTarget::Relation {
                    relating: FeatureId(0),
                    related: FeatureId(1),
                    name: "controlled pair".into(),
                },
                "0.3",
            ),
        ],
        tolerance_relations: [
            RelationKind::Composite,
            RelationKind::Precedence,
            RelationKind::Simultaneity,
        ]
        .into_iter()
        .map(|kind| ToleranceRelation {
            kind,
            relating: ToleranceId(0),
            related: ToleranceId(1),
        })
        .collect(),
        decimal_places: Some(3),
        general: vec![GeneralTolerance::Table {
            name: "stated linear defaults".into(),
            cells: vec![ToleranceCell {
                items: vec![
                    ("lower limit".into(), CellValue::Value(mm("0."))),
                    ("upper limit".into(), CellValue::Value(mm("100."))),
                    (
                        "plus minus tolerance value".into(),
                        CellValue::Value(mm("0.1")),
                    ),
                    ("number of decimal places".into(), CellValue::Count(d("3."))),
                    (
                        "cell description".into(),
                        CellValue::Text("linear dimensions".into()),
                    ),
                ],
            }],
        }],
        material: Some(Material {
            id: "steel".into(),
            name: Some("test steel".into()),
            density: Some(Density {
                value: d("7850."),
                unit: vec![("kilogram".into(), d("1.")), ("metre".into(), d("-3."))],
            }),
        }),
        notes: (1..=5)
            .map(|i| Note {
                kind: format!("general note {i}"),
                text: format!("{i}. Preserve this requirement."),
                on: None,
                callouts: Vec::new(),
            })
            .collect(),
        ..PartPmi::default()
    };
    p.notes.extend([
        Note {
            kind: "surface finish".into(),
            text: "Ra 1.6 µm".into(),
            on: Some(NoteOwner::Feature(FeatureId(1))),
            callouts: Vec::new(),
        },
        Note {
            kind: "internal thread".into(),
            text: "M8 x 1.25-6H RH".into(),
            on: Some(NoteOwner::Feature(FeatureId(1))),
            callouts: Vec::new(),
        },
    ]);
    p.tolerances[0].zone = Some(Zone {
        form: ZoneForm::CylindricalOrCircular,
        projected: Some(ProjectedZone {
            end: Some(FeatureId(1)),
            length: mm("10."),
        }),
        non_uniform: None,
        runout_angle: None,
        affected_plane: Some(FeatureId(2)),
    });
    p
}
