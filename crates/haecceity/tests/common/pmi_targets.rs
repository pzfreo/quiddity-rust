//! Practice §6.6 targets, and Figure 3 precision defaults.
use haecceity::pmi::*;
use std::collections::{BTreeMap, BTreeSet};
fn d(s: &str) -> Decimal {
    Decimal::parse(s).unwrap()
}
fn length(s: &str) -> Length {
    Length {
        value: d(s),
        unit: LengthUnit::Millimetre,
    }
}
fn point(x: &str) -> GeometryItem {
    GeometryItem {
        leaves: vec![(
            "CARTESIAN_POINT".into(),
            vec![
                GeometryValue::Text("".into()),
                GeometryValue::List(vec![
                    GeometryValue::Real(d(x)),
                    GeometryValue::Real(d("0.")),
                    GeometryValue::Real(d("0.")),
                ]),
            ],
        )],
    }
}
pub fn decimal_cell() -> ToleranceCell {
    ToleranceCell {
        items: vec![("number of decimal places".into(), CellValue::Count(d("3")))],
    }
}
pub fn every_kind() -> PartPmi {
    let placement = Placement {
        origin: [length("1."), length("2."), length("3.")],
        axis: Some(Direction([d("0."), d("0."), d("1.")])),
        ref_direction: Some(Direction([d("1."), d("0."), d("0.")])),
    };
    let shapes = [
        TargetShape::Point,
        TargetShape::Line {
            length: length("5."),
        },
        TargetShape::Rectangle {
            length: length("5."),
            width: length("3."),
        },
        TargetShape::Circle {
            diameter: length("4."),
        },
        TargetShape::CircularCurve {
            diameter: length("6."),
        },
        TargetShape::Area(FeatureId(1)),
        TargetShape::Curve(FeatureId(2)),
    ];
    let targets = shapes
        .into_iter()
        .enumerate()
        .map(|(i, shape)| {
            let explicit = matches!(shape, TargetShape::Area(_) | TargetShape::Curve(_));
            DatumTarget::new(
                (i + 1) as u32,
                shape,
                (!explicit).then(|| placement.clone()),
                None,
                (!explicit).then_some(FeatureId(0)),
            )
            .unwrap()
        })
        .collect();
    let datums = vec![
        Datum::new(
            DatumLabel::new("A").unwrap(),
            None,
            (0..7).map(DatumTargetId).collect(),
        )
        .unwrap(),
    ];
    let system = DatumSystem::new(vec![Compartment {
        references: vec![DatumReference {
            datum: DatumId(0),
            modifiers: vec![],
        }],
        modifiers: vec![],
    }])
    .unwrap();
    PartPmi {
        features: vec![
            Feature::Items(vec![Anchor::Face(FaceIndex(0))]),
            Feature::Items(vec![Anchor::Face(FaceIndex(1))]),
            Feature::Items(vec![Anchor::Geometry(GeometryId(0))]),
        ],
        geometry: vec![Geometry {
            item: GeometryItem {
                leaves: vec![(
                    "POLYLINE".into(),
                    vec![
                        GeometryValue::Text("".into()),
                        GeometryValue::List(vec![
                            GeometryValue::Item(Box::new(point("0."))),
                            GeometryValue::Item(Box::new(point("10."))),
                        ]),
                    ],
                )],
            },
            length_unit: Some(LengthUnit::Millimetre),
        }],
        datum_targets: targets,
        datums,
        tolerances: vec![GeometricTolerance {
            kind: ToleranceKind::Position,
            target: ToleranceTarget::Feature(FeatureId(0)),
            magnitude: Some(Value::length(d("0.05"), LengthUnit::Millimetre)),
            zone: None,
            modifiers: vec![],
            datums: Some(system),
            unit_basis: None,
            maximum: None,
            unequal: None,
            auxiliary: vec![],
            description: None,
        }],
        decimal_places: Some(3),
        general: vec![GeneralTolerance::Table {
            name: "precision".into(),
            cells: vec![decimal_cell()],
        }],
        ..PartPmi::default()
    }
}

/// Only the original's target requirements and their feature dependency graph, reindexed.
pub fn from_read(p: &PartPmi) -> PartPmi {
    let datums: Vec<_> = p
        .datums
        .iter()
        .filter(|d| !d.targets().is_empty())
        .collect();
    let mut needed = BTreeSet::new();
    let mut pending: Vec<_> = p
        .datum_targets
        .iter()
        .flat_map(|t| {
            t.on().into_iter().chain(match t.shape() {
                TargetShape::Area(f) | TargetShape::Curve(f) => Some(*f),
                _ => None,
            })
        })
        .chain(datums.iter().filter_map(|d| d.feature()))
        .collect();
    while let Some(f) = pending.pop() {
        if needed.insert(f.0) {
            match &p.features[f.0] {
                Feature::Group { members, .. } | Feature::Derived { from: members, .. } => {
                    pending.extend(members)
                }
                _ => {}
            }
        }
    }
    let map: BTreeMap<_, _> = needed
        .iter()
        .enumerate()
        .map(|(i, &old)| (old, FeatureId(i)))
        .collect();
    let features = needed
        .iter()
        .map(|&i| match &p.features[i] {
            Feature::Items(v) => Feature::Items(v.clone()),
            Feature::Group { members, kind } => Feature::Group {
                members: members.iter().map(|f| map[&f.0]).collect(),
                kind: kind.clone(),
            },
            Feature::Derived {
                kind,
                from,
                geometry,
            } => Feature::Derived {
                kind: *kind,
                from: from.iter().map(|f| map[&f.0]).collect(),
                geometry: *geometry,
            },
        })
        .collect();
    let datum_targets = p
        .datum_targets
        .iter()
        .map(|t| {
            DatumTarget::new(
                t.number(),
                match t.shape() {
                    TargetShape::Area(f) => TargetShape::Area(map[&f.0]),
                    TargetShape::Curve(f) => TargetShape::Curve(map[&f.0]),
                    s => s.clone(),
                },
                t.placement().cloned(),
                t.movable().cloned(),
                t.on().map(|f| map[&f.0]),
            )
            .unwrap()
        })
        .collect();
    let datums = datums
        .iter()
        .map(|d| {
            Datum::new(
                d.label().clone(),
                d.feature().map(|f| map[&f.0]),
                d.targets().to_vec(),
            )
            .unwrap()
        })
        .collect();
    PartPmi {
        features,
        geometry: p.geometry.clone(),
        derived_geometry: p.derived_geometry.clone(),
        datum_targets,
        datums,
        ..PartPmi::default()
    }
}

/// Cross-check probes only: XCAF exposes target datums when a tolerance references them.
/// These explicitly stated synthetic FCFs are never added by the production writer.
pub fn with_occt_probes(mut p: PartPmi) -> PartPmi {
    let f = FeatureId(p.features.len());
    p.features
        .push(Feature::Items(vec![Anchor::Face(FaceIndex(0))]));
    p.tolerances = (0..p.datums.len())
        .map(|i| GeometricTolerance {
            kind: ToleranceKind::Position,
            target: ToleranceTarget::Feature(f),
            magnitude: Some(Value::length(d("0.1"), LengthUnit::Millimetre)),
            datums: Some(
                DatumSystem::new(vec![Compartment {
                    references: vec![DatumReference {
                        datum: DatumId(i),
                        modifiers: vec![],
                    }],
                    modifiers: vec![],
                }])
                .unwrap(),
            ),
            zone: None,
            modifiers: vec![],
            unit_basis: None,
            maximum: None,
            unequal: None,
            auxiliary: vec![],
            description: None,
        })
        .collect();
    p
}

/// Unshared support aspects exercise Figure 45's direct-usage variant in XCAF.
pub fn separate_support(mut p: PartPmi) -> PartPmi {
    for t in &mut p.datum_targets {
        if t.on().is_some() {
            *t = DatumTarget::new(
                t.number(),
                t.shape().clone(),
                t.placement().cloned(),
                t.movable().cloned(),
                Some(FeatureId(1)),
            )
            .unwrap();
        }
    }
    p
}
