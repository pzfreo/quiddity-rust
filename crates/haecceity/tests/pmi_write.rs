//! The AP242 PMI writer: written PMI reads back as written, the original file's bytes are
//! kept, and every row of the design's Anti-requirements table has its test, named after it.

mod common;

use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};

use haecceity::express;
use haecceity::p21::{Attribute, Document, RawEntity};
use haecceity::pmi::write::{
    ItemRef, Mode, PresentationPolicy, SchemaChange, WriteError, WriteReport, differences,
    differences_as_stated,
};
use haecceity::pmi::{self, *};
use haecceity::step::{PartDefinition, read_part_definitions};

fn file_bytes(path: &Path) -> Vec<u8> {
    let raw = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    if path.extension().is_some_and(|e| e == "gz") {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(raw.as_slice())
            .read_to_end(&mut out)
            .unwrap();
        out
    } else {
        raw
    }
}

struct File {
    parts: Vec<PartDefinition>,
    doc: Document,
}

fn open(path: &Path) -> File {
    let bytes = file_bytes(path);
    let parts = read_part_definitions(&bytes).expect("part definitions");
    let doc = Document::parse(bytes).expect("document");
    File { parts, doc }
}

fn fixture(rel: &str) -> File {
    open(&common::fixtures().join(rel))
}

/// A specify-core-written part, whose PMI each test replaces with its own.
fn spool() -> File {
    fixture("ap242/specify/spool_fits.step.gz")
}

fn dec(t: &str) -> Decimal {
    Decimal::parse(t).unwrap()
}

fn mm(t: &str) -> Value {
    Value::length(dec(t), LengthUnit::Millimetre)
}

fn length(t: &str, unit: LengthUnit) -> Length {
    Length {
        value: dec(t),
        unit,
    }
}

fn faces(f: &[usize]) -> Feature {
    Feature::Items(f.iter().map(|&i| Anchor::Face(FaceIndex(i))).collect())
}

fn label(l: &str) -> DatumLabel {
    DatumLabel::new(l).unwrap()
}

fn datum(l: &str, f: usize) -> Datum {
    Datum::new(label(l), Some(FeatureId(f)), Vec::new()).unwrap()
}

fn system(datums: &[usize]) -> DatumSystem {
    DatumSystem::new(
        datums
            .iter()
            .map(|&d| Compartment {
                references: vec![DatumReference {
                    datum: DatumId(d),
                    modifiers: Vec::new(),
                }],
                modifiers: Vec::new(),
            })
            .collect(),
    )
    .unwrap()
}

fn tolerance(kind: ToleranceKind, target: ToleranceTarget, magnitude: &str) -> GeometricTolerance {
    GeometricTolerance {
        kind,
        target,
        magnitude: Some(mm(magnitude)),
        zone: None,
        modifiers: Vec::new(),
        unit_basis: None,
        maximum: None,
        unequal: None,
        datums: None,
        auxiliary: Vec::new(),
        description: None,
    }
}

fn size(feature: usize, nominal: &str, tolerance: DimTolerance) -> Dimension {
    Dimension {
        kind: DimensionKind::Size {
            feature: FeatureId(feature),
            kind: SizeKind::Diameter,
            path: None,
            angle: None,
        },
        nominal: Some(mm(nominal)),
        tolerance,
        qualifier: None,
        modifiers: Vec::new(),
        principle: None,
    }
}

fn bounds(upper: &str, lower: &str) -> Bounds {
    Bounds::new(mm(upper), mm(lower)).unwrap()
}

fn fit(class: &str) -> Iso286Class {
    let split = class.find(|c: char| c.is_ascii_digit()).unwrap();
    Iso286Class {
        deviation: FundamentalDeviation::parse(&class[..split]).unwrap(),
        grade: ToleranceGrade::parse(&class[split..]).unwrap(),
    }
}

struct Written {
    doc: Document,
    read: PmiRead,
    report: WriteReport,
    /// The original's largest id: added instances are above it.
    max: u64,
}

impl Written {
    /// The added instances.
    fn added(&self) -> Vec<u64> {
        self.doc.ids().filter(|&id| id > self.max).collect()
    }

    fn text(&self, id: u64) -> String {
        String::from_utf8_lossy(&self.doc.bytes()[self.doc.span(id).unwrap()]).into_owned()
    }

    fn names(&self, id: u64) -> Vec<String> {
        match self.doc.get(id).unwrap() {
            RawEntity::Simple { name, .. } => vec![name.to_ascii_lowercase()],
            RawEntity::Complex { parts, .. } => {
                parts.iter().map(|p| p.name.to_ascii_lowercase()).collect()
            }
        }
    }
}

fn write(f: &File, pmi: &[(PartId, PartPmi)], mode: Mode) -> Result<Written, WriteError> {
    let (edit, report) = pmi::write(
        &f.doc,
        &f.parts,
        pmi,
        mode,
        PresentationPolicy::RemovePresentation,
    )?;
    let bytes = f.doc.apply(&edit).unwrap().bytes;
    let parts = read_part_definitions(&bytes).unwrap();
    let doc = Document::parse(bytes).unwrap();
    let read = pmi::read(&doc, &parts).unwrap();
    Ok(Written {
        doc,
        read,
        report,
        max: f.doc.max_id(),
    })
}

/// Writes `p` as part 0's PMI (replacing what it had) and checks it reads back as written,
/// with no finding about what was written.
fn written(f: &File, p: &PartPmi) -> Written {
    let w = write(f, &[(PartId(0), p.clone())], Mode::Replace)
        .unwrap_or_else(|e| panic!("not written: {e}"));
    assert_reads_back(&w, PartId(0), p);
    w
}

/// `pmi::verify` of a replace of part 0's PMI with `p` (written into `f`, read back as `w`).
fn assert_verified(f: &File, w: &Written, p: &PartPmi) {
    assert_verified_as(f, w, p, Mode::Replace);
}

/// `pmi::verify` of a write of part 0's PMI `p` with `mode`.
fn assert_verified_as(f: &File, w: &Written, p: &PartPmi, mode: Mode) {
    let before = pmi::read(&f.doc, &f.parts).unwrap();
    let parts = read_part_definitions(w.doc.bytes()).unwrap();
    let v = pmi::verify(
        pmi::Snapshot {
            doc: &f.doc,
            defs: &f.parts,
            read: &before,
        },
        &f.doc,
        &[(PartId(0), p.clone())],
        mode,
        pmi::Snapshot {
            doc: &w.doc,
            defs: &parts,
            read: &w.read,
        },
    );
    assert!(v.is_ok(), "{:?}", v.into_result());
}

fn assert_reads_back(w: &Written, part: PartId, p: &PartPmi) {
    let d = differences_as_stated(p, &w.read.parts[part.0]);
    assert!(
        d.is_empty(),
        "read back differs:\n{}\nfindings: {:#?}",
        d.join("\n"),
        w.read
            .findings
            .iter()
            .filter(|f| f.ids.iter().any(|&id| id > w.max))
            .collect::<Vec<_>>()
    );
    let on_written: Vec<&Finding> = w
        .read
        .findings
        .iter()
        .filter(|f| f.ids.iter().any(|&id| id > w.max))
        .collect();
    assert!(
        on_written.is_empty(),
        "findings on written instances: {on_written:#?}"
    );
}

/// Four planar features, datums A–D on them.
fn four_datums() -> PartPmi {
    PartPmi {
        features: vec![faces(&[0]), faces(&[1]), faces(&[2]), faces(&[3])],
        datums: vec![datum("A", 0), datum("B", 1), datum("C", 2), datum("D", 3)],
        ..PartPmi::default()
    }
}

// ---------------------------------------------------------------------------------------------
// Anti-requirements, one test per row (design: Anti-requirements)
// ---------------------------------------------------------------------------------------------

/// Datum precedence on the datum (one label per letter per tolerance): A|B, B|C|A and D|B|C on
/// one part read back exactly, with one DATUM per letter in the file.
#[test]
fn datum_precedence_is_the_systems_not_the_datums() {
    let mut p = four_datums();
    p.features.push(faces(&[4]));
    for (sys, kind) in [
        (vec![0, 1], ToleranceKind::Perpendicularity),
        (vec![1, 2, 0], ToleranceKind::Parallelism),
        (vec![3, 1, 2], ToleranceKind::Position),
    ] {
        let mut t = tolerance(kind, ToleranceTarget::Feature(FeatureId(4)), "0.1");
        t.datums = Some(system(&sys));
        p.tolerances.push(t);
    }
    let w = written(&spool(), &p);
    for l in ["A", "B", "C", "D"] {
        let n = w
            .added()
            .into_iter()
            .filter(|&id| w.names(id) == ["datum"] && w.text(id).contains(&format!("'{l}')")))
            .count();
        assert_eq!(n, 1, "datum {l} written {n} times");
    }
    let systems: Vec<String> = w.read.parts[0]
        .tolerances
        .iter()
        .map(|t| {
            t.datums
                .as_ref()
                .unwrap()
                .compartments()
                .iter()
                .map(|c| {
                    w.read.parts[0].datums[c.references[0].datum.0]
                        .label()
                        .to_string()
                })
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect();
    let mut systems = systems;
    systems.sort();
    assert_eq!(systems, ["A|B", "B|C|A", "D|B|C"]);
}

/// Lower deviation negated: +0.1/−0.05 and g6 (−0.009/−0.025) round trip, written as signed
/// offsets.
#[test]
fn lower_deviations_keep_their_sign() {
    let p = PartPmi {
        features: vec![faces(&[0]), faces(&[1])],
        dimensions: vec![
            size(0, "20.", DimTolerance::Deviations(bounds("0.1", "-0.05"))),
            size(
                1,
                "70.",
                DimTolerance::Deviations(bounds("-0.009", "-0.025")),
            ),
        ],
        ..PartPmi::default()
    };
    let w = written(&spool(), &p);
    let values: Vec<String> = w
        .added()
        .into_iter()
        .filter(|&id| w.names(id) == ["tolerance_value"])
        .map(|id| {
            let RawEntity::Simple { attributes, .. } = w.doc.get(id).unwrap() else {
                unreachable!()
            };
            attributes
                .iter()
                .map(|a| match a {
                    Attribute::EntityRef(m) => w.text(*m),
                    _ => panic!("tolerance_value bound is not a measure"),
                })
                .collect::<Vec<_>>()
                .join(" / ")
        })
        .collect();
    assert_eq!(values.len(), 2);
    assert!(
        values
            .iter()
            .any(|v| v.contains("LENGTH_MEASURE(-0.05)") && v.contains("LENGTH_MEASURE(0.1)")),
        "{values:?}"
    );
    assert!(
        values
            .iter()
            .any(|v| v.contains("LENGTH_MEASURE(-0.025)") && v.contains("LENGTH_MEASURE(-0.009)")),
        "{values:?}"
    );
}

/// Both deviations below nominal unreadable: g6 and f7 (−0.020/−0.041) are written and read.
#[test]
fn deviations_both_below_nominal_are_written_and_read() {
    let p = PartPmi {
        features: vec![faces(&[0]), faces(&[1])],
        dimensions: vec![
            size(
                0,
                "70.",
                DimTolerance::Deviations(bounds("-0.010", "-0.029")),
            ),
            size(
                1,
                "20.",
                DimTolerance::Deviations(bounds("-0.020", "-0.041")),
            ),
        ],
        ..PartPmi::default()
    };
    written(&spool(), &p);
}

/// ISO 286 grade written wrongly (fits as bare limits): Ø20 H7, Ø70 g6 and Ø20 H7
/// (20.000/20.021) read back as fits, `LIMITS_AND_FITS('H','hole','7','')`.
#[test]
fn iso286_fits_are_written_as_fits() {
    let p = PartPmi {
        features: vec![faces(&[0]), faces(&[1]), faces(&[2])],
        dimensions: vec![
            size(
                0,
                "20.",
                DimTolerance::Fit {
                    class: fit("H7"),
                    limits: None,
                },
            ),
            size(
                1,
                "70.",
                DimTolerance::Fit {
                    class: fit("g6"),
                    limits: None,
                },
            ),
            size(
                2,
                "20.",
                DimTolerance::Fit {
                    class: fit("H7"),
                    limits: Some(bounds("20.021", "20.000")),
                },
            ),
        ],
        ..PartPmi::default()
    };
    let w = written(&spool(), &p);
    let fits: Vec<String> = w
        .added()
        .into_iter()
        .filter(|&id| w.names(id) == ["limits_and_fits"])
        .map(|id| w.text(id))
        .collect();
    assert_eq!(fits.len(), 3);
    assert_eq!(
        fits.iter()
            .filter(|t| t.contains("('H','hole','7','')"))
            .count(),
        2,
        "{fits:?}"
    );
    assert_eq!(
        fits.iter()
            .filter(|t| t.contains("('g','shaft','6','')"))
            .count(),
        1,
        "{fits:?}"
    );
    assert!(
        w.read.parts[0]
            .dimensions
            .iter()
            .all(|d| matches!(d.tolerance, DimTolerance::Fit { .. }))
    );
}

/// Tolerance unit taken from the last dimension written (1000×): tolerances on a part with no
/// dimension carry their own unit; millimetre values written into an inch-context part (NIST
/// STC-06) keep their unit and their text.
#[test]
fn tolerance_units_are_each_values_own() {
    // No dimension at all: a flatness and a position in millimetres, a profile in inches.
    let mut p = four_datums();
    p.tolerances.push(tolerance(
        ToleranceKind::Flatness,
        ToleranceTarget::Feature(FeatureId(0)),
        "0.05",
    ));
    let mut inch = tolerance(
        ToleranceKind::SurfaceProfile,
        ToleranceTarget::Feature(FeatureId(1)),
        "0.002",
    );
    inch.magnitude = Some(Value::length(dec("0.002"), LengthUnit::Inch));
    p.tolerances.push(inch);
    written(&spool(), &p);

    // Millimetre values onto an inch part, beside its own PMI.
    let f = fixture("ap242/nist/nist_stc_06_asme1_ap242-e3.stp.gz");
    let free = free_faces(&f, 0, 2);
    let new = PartPmi {
        features: vec![faces(&[free[0]]), faces(&[free[1]])],
        dimensions: vec![size(
            1,
            "12.5",
            DimTolerance::Deviations(bounds("0.1", "-0.05")),
        )],
        tolerances: vec![tolerance(
            ToleranceKind::Flatness,
            ToleranceTarget::Feature(FeatureId(0)),
            "0.0030",
        )],
        ..PartPmi::default()
    };
    let w = write(&f, &[(PartId(0), new.clone())], Mode::Add).unwrap_or_else(|e| panic!("{e}"));
    let after = &w.read.parts[0];
    let flat = after
        .tolerances
        .iter()
        .find(|t| {
            t.kind == ToleranceKind::Flatness
                && t.magnitude
                    .as_ref()
                    .is_some_and(|m| m.decimal().as_str() == "0.0030")
        })
        .expect("the flatness reads back");
    assert_eq!(
        flat.magnitude.as_ref().unwrap().as_length().unwrap().unit,
        LengthUnit::Millimetre
    );
    let dim = after
        .dimensions
        .iter()
        .find(|d| {
            d.nominal
                .as_ref()
                .is_some_and(|n| n.decimal().as_str() == "12.5")
        })
        .expect("the dimension reads back");
    assert_eq!(
        dim.nominal.as_ref().unwrap().as_length().unwrap().unit,
        LengthUnit::Millimetre
    );
    assert_eq!(
        dim.tolerance,
        DimTolerance::Deviations(bounds("0.1", "-0.05"))
    );
}

/// Values changed by unit conversion: written in their own unit with their own digits (every
/// inch NIST file's values keep their text: `pmi_roundtrip.rs`); here stated texts that a
/// conversion or shortest-form printing would change.
#[test]
fn values_keep_their_stated_text() {
    let p = PartPmi {
        features: vec![faces(&[0]), faces(&[1])],
        dimensions: vec![Dimension {
            nominal: Some(Value::length(dec("0.3750"), LengthUnit::Inch)),
            tolerance: DimTolerance::Deviations(
                Bounds::new(
                    Value::length(dec("0.0030"), LengthUnit::Inch),
                    Value::length(dec("-0.0010"), LengthUnit::Inch),
                )
                .unwrap(),
            ),
            ..size(0, "1.", DimTolerance::None)
        }],
        tolerances: vec![tolerance(
            ToleranceKind::Flatness,
            ToleranceTarget::Feature(FeatureId(1)),
            "2.50E-02",
        )],
        ..PartPmi::default()
    };
    let w = written(&spool(), &p);
    let text: String = w.added().into_iter().map(|id| w.text(id)).collect();
    for t in ["(0.3750)", "(0.0030)", "(-0.0010)", "(2.50E-02)", "'INCH'"] {
        assert!(text.contains(t), "{t} not written");
    }
}

/// PMI that has been read cannot be written back: read → replace → read of a specify-core file
/// is the same PMI (every file: `pmi_roundtrip.rs`).
#[test]
fn pmi_read_is_written_back() {
    let f = spool();
    let r = pmi::read(&f.doc, &f.parts).unwrap();
    let mut p = r.parts[0].clone();
    p.notes.clear(); // decision 5: the text route is not written
    written(&f, &p);
}

/// Malformed RUNOUT_ZONE_DEFINITION: the validator rejects a two-attribute zone; the writer
/// writes three (zone, boundaries, orientation) only for a stated orientation.
#[test]
fn runout_zone_definitions_have_their_three_attributes() {
    let bad = haecceity::p21::simple(
        "RUNOUT_ZONE_DEFINITION",
        vec![Attribute::EntityRef(1), Attribute::List(Vec::new())],
    );
    let types = |id: u64| match id {
        1 => Some(vec!["TOLERANCE_ZONE".to_string()]),
        _ => Some(vec!["RUNOUT_ZONE_ORIENTATION".to_string()]),
    };
    let v = express::validate(&bad, &types);
    assert!(
        v.iter().any(|v| v.kind == express::Kind::AttributeCount),
        "{v:?}"
    );

    let mut p = four_datums();
    let mut t = tolerance(
        ToleranceKind::CircularRunout,
        ToleranceTarget::Feature(FeatureId(1)),
        "0.02",
    );
    t.datums = Some(system(&[0]));
    t.zone = Some(Zone {
        form: ZoneForm::Other(String::new()),
        projected: None,
        non_uniform: None,
        runout_angle: Some(Value::angle(dec("30."), AngleUnit::Degree)),
        affected_plane: None,
    });
    p.tolerances.push(t);
    let mut plain = tolerance(
        ToleranceKind::TotalRunout,
        ToleranceTarget::Feature(FeatureId(2)),
        "0.03",
    );
    plain.datums = Some(system(&[0]));
    plain.zone = Some(Zone {
        form: ZoneForm::Other(String::new()),
        projected: None,
        non_uniform: None,
        runout_angle: None,
        affected_plane: None,
    });
    p.tolerances.push(plain);
    let w = written(&spool(), &p);
    let zones: Vec<u64> = w
        .added()
        .into_iter()
        .filter(|&id| w.names(id) == ["runout_zone_definition"])
        .collect();
    assert_eq!(zones.len(), 1, "one stated orientation, one definition");
}

/// Untyped MEASURE_WITH_UNIT: the validator rejects an untyped value; nothing the writer makes
/// has one.
#[test]
fn untyped_measures_cannot_be_written() {
    let bad = haecceity::p21::simple(
        "LENGTH_MEASURE_WITH_UNIT",
        vec![Attribute::Real(0.5), Attribute::EntityRef(1)],
    );
    let types = |_| {
        Some(vec![
            "LENGTH_UNIT".into(),
            "NAMED_UNIT".into(),
            "SI_UNIT".into(),
        ])
    };
    let v = express::validate(&bad, &types);
    assert!(v.iter().any(|v| v.kind == express::Kind::Untyped), "{v:?}");

    let mut p = four_datums();
    p.dimensions.push(size(
        0,
        "20.",
        DimTolerance::Deviations(bounds("0.1", "-0.05")),
    ));
    let mut t = tolerance(
        ToleranceKind::Position,
        ToleranceTarget::Dimension(DimensionId(0)),
        "0.2",
    );
    t.datums = Some(system(&[1, 2]));
    t.modifiers = vec![ToleranceModifier::MaximumMaterialRequirement];
    p.tolerances.push(t);
    let w = written(&spool(), &p);
    let mut measures = 0;
    for id in w.added() {
        if let RawEntity::Simple {
            name, attributes, ..
        } = w.doc.get(id).unwrap()
            && name.ends_with("MEASURE_WITH_UNIT")
        {
            measures += 1;
            assert!(
                matches!(attributes[0], Attribute::Typed { .. }),
                "#{id} untyped"
            );
        }
        if let RawEntity::Complex { parts, .. } = w.doc.get(id).unwrap() {
            for leaf in parts.iter().filter(|l| l.name == "MEASURE_WITH_UNIT") {
                measures += 1;
                assert!(
                    matches!(leaf.attributes[0], Attribute::Typed { .. }),
                    "#{id} untyped"
                );
            }
        }
    }
    assert!(measures >= 4);
}

/// Complex entities where the simple form is expected: a perpendicularity with datums is a
/// simple PERPENDICULARITY_TOLERANCE (the validator rejects it as a complex instance); a
/// position with datums and a modifier is the complex instance it must be.
#[test]
fn complex_instances_only_where_required() {
    let bad = haecceity::p21::Complex::new([
        haecceity::p21::leaf(
            "GEOMETRIC_TOLERANCE",
            vec![
                Attribute::String(String::new()),
                Attribute::String(String::new()),
                Attribute::EntityRef(1),
                Attribute::EntityRef(2),
            ],
        ),
        haecceity::p21::leaf(
            "GEOMETRIC_TOLERANCE_WITH_DATUM_REFERENCE",
            vec![Attribute::List(vec![Attribute::EntityRef(3)])],
        ),
        haecceity::p21::leaf("PERPENDICULARITY_TOLERANCE", vec![]),
    ])
    .unwrap()
    .into_record();
    let types = |id: u64| {
        Some(vec![
            match id {
                1 => "LENGTH_MEASURE_WITH_UNIT",
                2 => "SHAPE_ASPECT",
                _ => "DATUM_SYSTEM",
            }
            .to_string(),
        ])
    };
    let v = express::validate(&bad, &types);
    assert!(
        v.iter().any(|v| v.kind == express::Kind::InternalMapping),
        "{v:?}"
    );

    let mut p = four_datums();
    p.features.push(faces(&[4]));
    let mut perp = tolerance(
        ToleranceKind::Perpendicularity,
        ToleranceTarget::Feature(FeatureId(4)),
        "0.05",
    );
    perp.datums = Some(system(&[0]));
    let mut pos = tolerance(
        ToleranceKind::Position,
        ToleranceTarget::Feature(FeatureId(4)),
        "0.1",
    );
    pos.datums = Some(system(&[0, 1]));
    pos.modifiers = vec![ToleranceModifier::MaximumMaterialRequirement];
    p.tolerances = vec![perp, pos];
    let w = written(&spool(), &p);
    let kinds: BTreeSet<Vec<String>> = w
        .added()
        .into_iter()
        .map(|id| w.names(id))
        .filter(|n| {
            n.iter()
                .any(|x| x.ends_with("_tolerance") && x != "plus_minus_tolerance")
        })
        .collect();
    assert!(
        kinds.contains(&vec!["perpendicularity_tolerance".to_string()]),
        "{kinds:?}"
    );
    assert!(
        kinds.contains(&vec![
            "geometric_tolerance".to_string(),
            "geometric_tolerance_with_datum_reference".to_string(),
            "geometric_tolerance_with_modifiers".to_string(),
            "position_tolerance".to_string(),
        ]),
        "{kinds:?}"
    );
}

/// Datums imported only when referenced: a datum no tolerance references reads back.
#[test]
fn unreferenced_datums_are_written_and_read() {
    let p = PartPmi {
        features: vec![faces(&[0]), faces(&[5, 6])],
        datums: vec![datum("E", 0), datum("F", 1)],
        ..PartPmi::default()
    };
    written(&spool(), &p);
}

/// The references of instance `id`, in attribute order.
fn refs_of(doc: &Document, id: u64) -> Vec<u64> {
    let mut out = Vec::new();
    haecceity::p21::visit_attributes(doc.get(id).unwrap(), &mut |a| {
        if let Attribute::EntityRef(n) = a {
            out.push(*n);
        }
    });
    out
}

/// The datum feature symbols of the written file (decision 6): per
/// `draughting_model_item_association`, the label of the datum its datum feature establishes,
/// with its callout checked to be a tessellated 'datum' set in an annotation plane of a
/// draughting model that no `mechanical_design_and_draughting_relationship` relates to the
/// part's shape representation (left out, docs/step-ap242.md decisions of 2026-10-09).
fn datum_symbols(w: &Written) -> Vec<String> {
    let mut out = Vec::new();
    for id in w.doc.ids() {
        if w.names(id) != ["draughting_model_item_association"] {
            continue;
        }
        let [feature, model, callout] = refs_of(&w.doc, id)[..] else {
            panic!("#{id}: {}", w.text(id));
        };
        assert!(
            w.text(id)
                .contains("'PMI representation to presentation link'")
        );
        assert!(
            w.names(feature).iter().any(|n| n.contains("datum_feature")),
            "#{id} links {:?}",
            w.names(feature)
        );
        assert_eq!(w.names(model), ["draughting_model"]);
        assert_eq!(w.names(callout), ["draughting_callout"]);
        let [occurrence] = refs_of(&w.doc, callout)[..] else {
            panic!("{}", w.text(callout));
        };
        assert_eq!(w.names(occurrence), ["tessellated_annotation_occurrence"]);
        let set = *refs_of(&w.doc, occurrence).last().unwrap();
        assert!(
            w.text(set).contains("TESSELLATED_GEOMETRIC_SET('datum'"),
            "{}",
            w.text(set)
        );
        let plane = refs_of(&w.doc, model)
            .into_iter()
            .find(|&p| w.names(p) == ["annotation_plane"] && refs_of(&w.doc, p).contains(&callout))
            .expect("the callout's annotation plane is in the model");
        assert!(refs_of(&w.doc, plane).len() >= 3);
        assert!(
            !w.doc
                .referrers(model)
                .iter()
                .any(|&r| w.names(r) == ["mechanical_design_and_draughting_relationship"]),
            "the model is not related to the shape (maintainer decision 2026-10-09: \
             OpenCascade 7.9 crashes on that relationship)"
        );
        let datum = w
            .doc
            .referrers(feature)
            .iter()
            .filter(|&&r| w.names(r) == ["shape_aspect_relationship"])
            .map(|&r| refs_of(&w.doc, r)[1])
            .find(|&d| w.names(d) == ["datum"])
            .expect("the feature establishes a datum");
        let text = w.text(datum);
        let label = text.rsplit('\'').nth(1).unwrap().to_string();
        out.push(label);
    }
    out.sort();
    out
}

/// Datum feature symbols (decision 6): each added datum gets one, linked to its datum feature,
/// valid against the schema (the writer's own check) and not read as PMI; replace and remove
/// take the symbols with their datums, leaving none of what the first write added; a datum
/// resolved to the file's own (add) gets none.
#[test]
fn datum_feature_symbols_go_with_their_datums() {
    let f = spool();
    let w = written(&f, &four_datums());
    assert_eq!(datum_symbols(&w), ["A", "B", "C", "D"]);
    assert_eq!(
        w.report.datum_symbols,
        ["A", "B", "C", "D"].map(|l| (PartId(0), l.to_string()))
    );
    let again = File {
        parts: read_part_definitions(w.doc.bytes()).unwrap(),
        doc: Document::parse(w.doc.bytes().to_vec()).unwrap(),
    };
    // Replace with datum A only: the other three symbols go with their datums.
    let only_a = PartPmi {
        features: vec![faces(&[0])],
        datums: vec![datum("A", 0)],
        ..PartPmi::default()
    };
    let w2 = written(&again, &only_a);
    assert_eq!(datum_symbols(&w2), ["A"]);
    assert!(
        w2.doc
            .ids()
            .all(|id| id <= f.doc.max_id() || id > w.doc.max_id()),
        "an instance of the first write survives its replacement"
    );
    // Remove: nothing of the first write is left.
    let w3 = write(&again, &[(PartId(0), PartPmi::default())], Mode::Remove).unwrap();
    assert!(datum_symbols(&w3).is_empty());
    assert!(w3.doc.ids().all(|id| id <= f.doc.max_id()));
    // Under the Refuse policy the symbols are presentation like any other: a replace that
    // would orphan them is refused, naming them.
    match pmi::write(
        &again.doc,
        &again.parts,
        &[(PartId(0), PartPmi::default())],
        Mode::Remove,
        PresentationPolicy::Refuse,
    ) {
        Err(WriteError::Removal(r)) => assert!(
            r.blockers
                .iter()
                .any(|b| b.entity == "draughting_model_item_association"),
            "{r:?}"
        ),
        other => panic!("refused: {:?}", other.err()),
    }
    // Add with the file's datum A: reused, no symbol.
    let w4 = write(&again, &[(PartId(0), only_a)], Mode::Add).unwrap();
    assert!(w4.report.datum_symbols.is_empty());
    assert_eq!(datum_symbols(&w4).len(), 4);
}

/// A size and a tolerance on a feature of several faces (maintainer decision 2026-10-09): one
/// `geometric_item_specific_usage` per face, which OpenCascade reads, never an
/// `item_identified_representation_usage` of a `set_representation_item`. A shape aspect has
/// one usage per representation (UR2), so each face is a member shape aspect of its own,
/// shared by every feature on that face (UR1, §5.1), and the feature is composed of its
/// members; it reads back as the feature of those faces.
#[test]
fn a_feature_of_several_faces_has_a_usage_per_face() {
    let f = spool();
    let p = PartPmi {
        // Feature 1 shares face 10 with feature 2, which is a datum feature of its own.
        features: vec![faces(&[0]), faces(&[9, 10]), faces(&[10]), faces(&[5, 6])],
        datums: vec![datum("A", 0), datum("B", 2)],
        dimensions: vec![size(
            3,
            "20.",
            DimTolerance::Deviations(bounds("0.1", "-0.1")),
        )],
        tolerances: vec![{
            let mut t = tolerance(
                ToleranceKind::SurfaceProfile,
                ToleranceTarget::Feature(FeatureId(1)),
                "0.1",
            );
            t.datums = Some(system(&[0, 1]));
            t
        }],
        ..PartPmi::default()
    };
    let w = written(&f, &p);
    assert_verified(&f, &w, &p);
    let added = w.added();
    for &id in &added {
        assert!(
            !w.text(id).contains("SET_REPRESENTATION_ITEM"),
            "{}",
            w.text(id)
        );
        assert!(
            w.names(id) != ["item_identified_representation_usage"],
            "{}",
            w.text(id)
        );
    }
    // Every usage identifies one face; each face is identified once.
    let usages: Vec<u64> = added
        .iter()
        .copied()
        .filter(|&id| w.names(id) == ["geometric_item_specific_usage"])
        .collect();
    let mut identified: Vec<u64> = usages.iter().map(|&u| refs_of(&w.doc, u)[2]).collect();
    let n = identified.len();
    identified.sort_unstable();
    identified.dedup();
    assert_eq!(identified.len(), n, "a face is identified twice");
    assert_eq!(n, 5, "faces 0, 5, 6, 9, 10: one usage each");
    let violations: Vec<_> = haecceity::express_rules::check_all(&w.doc)
        .into_iter()
        .filter(|v| v.id > w.max)
        .collect();
    assert!(violations.is_empty(), "{violations:#?}");
}

/// Add onto a file whose kept plain shape aspect identifies a feature's faces by one usage of
/// a `set_representation_item` (§6.5.1, as earlier writes did): the new feature is composed of
/// that aspect, and no face gains a second usage (item_identified_representation_usage UR1).
#[test]
fn add_composes_a_feature_of_an_aspect_of_its_faces() {
    let f = spool();
    let def = &f.parts[0];
    let (face9, face10) = (def.faces[9], def.faces[10]);
    let gisu = f
        .doc
        .ids()
        .find(|&id| {
            matches!(f.doc.get(id).unwrap(), RawEntity::Simple { name, .. }
                if name.eq_ignore_ascii_case("geometric_item_specific_usage"))
        })
        .unwrap();
    let rep = refs_of(&f.doc, gisu)[1];
    let (aspect, usage) = (f.doc.max_id() + 1, f.doc.max_id() + 2);
    let mut text = String::from_utf8(f.doc.bytes().to_vec()).unwrap();
    let at = text.rfind("ENDSEC;").unwrap();
    text.insert_str(
        at,
        &format!(
            "#{aspect}=SHAPE_ASPECT('','',#{},.T.);\n#{usage}=ITEM_IDENTIFIED_REPRESENTATION_USAGE('','',#{aspect},#{rep},SET_REPRESENTATION_ITEM((#{face9},#{face10})));\n",
            def.shape
        ),
    );
    let g = open_bytes(text.into_bytes());
    let add = PartPmi {
        features: vec![faces(&[9, 10])],
        tolerances: vec![tolerance(
            ToleranceKind::SurfaceProfile,
            ToleranceTarget::Feature(FeatureId(0)),
            "0.1",
        )],
        ..PartPmi::default()
    };
    let w = write(&g, &[(PartId(0), add)], Mode::Add).unwrap_or_else(|e| panic!("{e}"));
    let added = w.added();
    let compositions: Vec<u64> = added
        .iter()
        .copied()
        .filter(|&id| w.names(id) == ["shape_aspect_relationship"])
        .collect();
    assert_eq!(compositions.len(), 1);
    assert_eq!(refs_of(&w.doc, compositions[0])[1], aspect);
    assert!(
        added
            .iter()
            .all(|&id| w.names(id) != ["geometric_item_specific_usage"]),
        "no new usage of faces 9 and 10"
    );
    let t = w.read.parts[0]
        .tolerances
        .iter()
        .find(|t| t.kind == ToleranceKind::SurfaceProfile && t.datums.is_none())
        .expect("the added profile tolerance");
    let ToleranceTarget::Feature(fid) = t.target else {
        panic!("{t:?}");
    };
    assert_eq!(w.read.parts[0].features[fid.0], faces(&[9, 10]));
}

/// Part notes in words (coating, heat treatment, edges) as the PMI practice's editable text
/// (§7.4: 'semantic text', a user defined attribute; on the part, on its
/// `product_definition_shape`, Table 17).
fn part_notes() -> Vec<AttributeSet> {
    [
        ("coating", "Anodise to MIL-A-8625 Type II, black"),
        ("heat treatment", "None"),
        ("edges", "Break sharp edges 0.2 max"),
    ]
    .into_iter()
    .map(|(k, v)| AttributeSet {
        name: "semantic text".into(),
        on: None,
        items: vec![(k.into(), AttributeValue::Text(v.into()))],
    })
    .collect()
}

/// Ra 3.2 µm on the part (any process) and Ra 0.8 µm, Rz 4 µm on `faces` (material removal
/// required).
fn textures(faces: Option<FeatureId>) -> Vec<SurfaceTexture> {
    let um = |t| length(t, LengthUnit::Micrometre);
    let mut out = vec![SurfaceTexture {
        on: None,
        material_removal: MaterialRemoval::AnyProcessAllowed,
        parameters: vec![SurfaceTextureParameter {
            characteristic: "Ra".into(),
            value: um("3.2"),
        }],
    }];
    if let Some(f) = faces {
        out.push(SurfaceTexture {
            on: Some(f),
            material_removal: MaterialRemoval::Required,
            parameters: vec![
                SurfaceTextureParameter {
                    characteristic: "Ra".into(),
                    value: um("0.8"),
                },
                SurfaceTextureParameter {
                    characteristic: "Rz".into(),
                    value: um("4."),
                },
            ],
        });
    }
    out
}

/// `text` with `from` replaced by `to` in the line of instance `#id` (which holds it once).
fn edited(text: &str, id: u64, from: &str, to: &str) -> String {
    let tag = format!("#{id}=");
    let old = text.lines().find(|l| l.starts_with(&tag)).unwrap();
    assert_eq!(old.matches(from).count(), 1, "{from} in {old}");
    text.replacen(old, &old.replacen(from, to, 1), 1)
}

/// `text` with `line` appended to its data section.
fn appended(text: &str, line: &str) -> String {
    let at = text.rfind("ENDSEC;").unwrap();
    format!("{}{line}\n{}", &text[..at], &text[at..])
}

/// Notes and surface finish in standard forms (decision 5; maintainer decisions 2026-10-09,
/// specify-core's U3): part notes as 'semantic text' (PMI practice §7.4) and surface texture as
/// AP242's surface conditions (ISO 10303-1110: `Surface_texture`, its
/// `Standard_surface_texture_parameter`s in `surface_texture_representation`s). Both read back
/// as written; the instances satisfy `surface_texture_representation` WR1–WR5,
/// `general_property_association` WR1–WR2 and the global rule
/// `restrict_representation_for_surface_condition` (`express_rules::check_all`, each rule also
/// shown failing on an edit of the written file that breaks it), and the writer's own schema
/// and rule checks. WR5 with the association's WR2
/// makes the parameter 'surface_condition', not the mapping's 'surface texture parameter'
/// (docs/step-ap242.md, question 3). A parameter without its 'surface_condition' association
/// is reported, not read.
#[test]
fn notes_and_surface_textures_are_standard_forms() {
    let f = spool();
    let mut p = PartPmi {
        features: vec![faces(&[9, 10])],
        attributes: part_notes(),
        surface_textures: textures(Some(FeatureId(0))),
        ..PartPmi::default()
    };
    let w = written(&f, &p);
    assert_verified(&f, &w, &p);
    let pds = f.parts[0].shape;
    let name_of = |id: u64| w.text(id).split('\'').nth(1).unwrap_or("").to_string();
    // A general_property's name follows its id.
    let gp_name = |id: u64| w.text(id).split('\'').nth(3).unwrap_or("").to_string();
    let added = w.added();
    let of = |entity: &str| -> Vec<u64> {
        added
            .iter()
            .copied()
            .filter(|&id| w.names(id) == [entity])
            .collect()
    };
    // Notes: 'semantic text' on the part's shape.
    let notes: Vec<u64> = of("property_definition")
        .into_iter()
        .filter(|&id| name_of(id) == "semantic text")
        .collect();
    assert_eq!(notes.len(), 3);
    for n in notes {
        assert_eq!(refs_of(&w.doc, n), [pds], "{}", w.text(n));
    }
    // Surface textures: 'surface texture' with one representation, each parameter related to
    // it and represented by a surface_texture_representation of two items (the 'measuring
    // method' and the value), associated with a general property. The rules these forms meet
    // are express_rules', below.
    let pdrs = of("property_definition_representation");
    let rep_of = |pd: u64| -> u64 {
        let found: Vec<u64> = pdrs
            .iter()
            .filter(|&&r| refs_of(&w.doc, r)[0] == pd)
            .map(|&r| refs_of(&w.doc, r)[1])
            .collect();
        assert_eq!(found.len(), 1, "{}", w.text(pd));
        found[0]
    };
    let textures_: Vec<u64> = of("property_definition")
        .into_iter()
        .filter(|&id| name_of(id) == "surface texture")
        .collect();
    assert_eq!(textures_.len(), 2);
    for &t in &textures_ {
        rep_of(t);
    }
    let mut parameters = Vec::new();
    for r in of("property_definition_relationship") {
        assert_eq!(name_of(r), "surface texture parameter");
        let [relating, related] = refs_of(&w.doc, r)[..] else {
            panic!("{}", w.text(r));
        };
        assert!(textures_.contains(&relating));
        assert_eq!(name_of(related), "surface_condition");
        let rep = rep_of(related);
        assert_eq!(w.names(rep), ["surface_texture_representation"]);
        // The representation's references: its items, then its context.
        let mut items = refs_of(&w.doc, rep);
        items.pop();
        assert_eq!(items.len(), 2, "{}", w.text(rep));
        // The writer relates it to no 'measuring direction' (which WR4 would allow): it is
        // used by its property_definition_representation alone.
        assert!(
            w.doc
                .referrers(rep)
                .iter()
                .all(|&x| w.names(x) == ["property_definition_representation"])
        );
        parameters.push((related, rep, items));
    }
    assert_eq!(parameters.len(), 3);
    // The rules (express_rules::check_all): surface_texture_representation WR1–WR5,
    // general_property_association WR1–WR2 (the notes' associations and the parameters') and
    // the global rule restrict_representation_for_surface_condition hold on what the write
    // added; an edit of the written file breaking each is reported as that rule alone.
    let rules = |doc: &Document| -> BTreeSet<(u64, &'static str)> {
        haecceity::express_rules::check_all(doc)
            .into_iter()
            .map(|v| (v.id, v.rule))
            .collect()
    };
    let base = rules(&w.doc);
    let on_added: Vec<_> = base.iter().filter(|&&(id, _)| id > w.max).collect();
    assert!(on_added.is_empty(), "{on_added:?}");
    let text = String::from_utf8(w.doc.bytes().to_vec()).unwrap();
    let broken = |t: &str| -> BTreeSet<&'static str> {
        assert_ne!(t, text);
        rules(&Document::parse(t.as_bytes().to_vec()).unwrap())
            .difference(&base)
            .map(|&(_, r)| r)
            .collect()
    };
    let (param, rep, ref items) = parameters[0];
    let (method, value) = (items[0], items[1]);
    assert_eq!(name_of(method), "measuring method");
    let other_rep = parameters[1].1;
    let point = w
        .doc
        .ids()
        .find(|&id| w.names(id) == ["cartesian_point"])
        .unwrap();
    let gpa = of("general_property_association")
        .into_iter()
        .find(|&g| gp_name(refs_of(&w.doc, g)[0]) == "semantic text")
        .unwrap();
    let [gp, derived] = refs_of(&w.doc, gpa)[..] else {
        panic!("{}", w.text(gpa));
    };
    let next = w.doc.max_id() + 1;
    let items_ = format!("(#{method},#{value})");
    for (case, t, rule) in [
        (
            "a texture's representation named otherwise",
            text.replace(
                "REPRESENTATION('surface texture'",
                "REPRESENTATION('surface finish'",
            ),
            "restrict_representation_for_surface_condition.WR1",
        ),
        (
            "a point among the items",
            edited(
                &text,
                rep,
                &items_,
                &format!("(#{method},#{value},#{point})"),
            ),
            "surface_texture_representation.WR1",
        ),
        (
            "no 'measuring method'",
            edited(&text, method, "'measuring method'", "'measuring mode'"),
            "surface_texture_representation.WR2",
        ),
        (
            "no measure",
            edited(&text, rep, &items_, &format!("(#{method})")),
            "surface_texture_representation.WR3",
        ),
        (
            "related to another parameter's representation",
            appended(
                &text,
                &format!("#{next}=REPRESENTATION_RELATIONSHIP('','',#{rep},#{other_rep});"),
            ),
            "surface_texture_representation.WR4",
        ),
        (
            "a second representation of the parameter",
            appended(
                &text,
                &format!("#{next}=PROPERTY_DEFINITION_REPRESENTATION(#{param},#{rep});"),
            ),
            "surface_texture_representation.WR5",
        ),
        (
            "a second association of a definition",
            appended(
                &text,
                &format!("#{next}=GENERAL_PROPERTY_ASSOCIATION('',$,#{gp},#{derived});"),
            ),
            "general_property_association.WR1",
        ),
    ] {
        assert_eq!(broken(&t), BTreeSet::from([rule]), "{case}");
    }
    // A parameter named as the mapping names it (breaking the association's WR2) reads the
    // same.
    let mapped = text.replace(
        "PROPERTY_DEFINITION('surface_condition'",
        "PROPERTY_DEFINITION('surface texture parameter'",
    );
    assert_eq!(
        broken(&mapped),
        BTreeSet::from(["general_property_association.WR2"])
    );
    let r = pmi::read(&Document::parse(mapped.into_bytes()).unwrap(), &f.parts).unwrap();
    assert_eq!(r.parts[0].surface_textures, p.surface_textures);
    // A parameter whose general property is not 'surface_condition' breaks WR5 (and the
    // association's WR2: the names differ): reported, and its texture not read.
    let renamed = text.replace(
        "GENERAL_PROPERTY('','surface_condition',$)",
        "GENERAL_PROPERTY('','roughness',$)",
    );
    assert_eq!(
        broken(&renamed),
        BTreeSet::from([
            "general_property_association.WR2",
            "surface_texture_representation.WR5"
        ])
    );
    let r = pmi::read(&Document::parse(renamed.into_bytes()).unwrap(), &f.parts).unwrap();
    assert!(r.parts[0].surface_textures.is_empty());
    assert!(
        r.findings.iter().any(|x| x.detail.contains("WR5")),
        "{:?}",
        r.findings
    );
    // Remove: nothing of them is left.
    let again = File {
        parts: read_part_definitions(w.doc.bytes()).unwrap(),
        doc: Document::parse(w.doc.bytes().to_vec()).unwrap(),
    };
    let w2 = write(&again, &[(PartId(0), PartPmi::default())], Mode::Remove).unwrap();
    assert!(w2.read.parts[0].surface_textures.is_empty());
    assert!(w2.read.parts[0].attributes.is_empty());
    // The micrometre the first write added stays: the removal plan never removes units
    // (shared infrastructure, docs/step-ap242.md stage 2).
    let left: Vec<String> = w2
        .doc
        .ids()
        .filter(|&id| id > f.doc.max_id())
        .map(|id| w2.text(id))
        .collect();
    assert_eq!(
        left,
        ["(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MICRO.,.METRE.));"].map(|t| {
            let id = w2.doc.ids().find(|&id| id > f.doc.max_id()).unwrap_or(0);
            format!("#{id}={t}")
        })
    );
    // A value that is not a Part 21 REAL is refused, naming the texture.
    p.surface_textures[1].parameters[0].value = length("35", LengthUnit::Micrometre);
    match write(&f, &[(PartId(0), p)], Mode::Replace) {
        Err(WriteError::Refused(r)) => {
            assert!(
                r.iter().any(|x| x.item == ItemRef::SurfaceTexture(1)),
                "{r:?}"
            )
        }
        other => panic!("not refused: {:?}", other.err()),
    }
}

/// One datum per name per document (assemblies): datum A on each part of the two-part
/// assembly, each on its own faces; replacing one part's PMI leaves the other's PMI and every
/// byte of its instances untouched. The AP214 file becomes AP242 (decision 1) only because
/// every instance validates against the target edition.
#[test]
fn datums_belong_to_their_part() {
    let f = fixture("ap242/assembly/assembly.step");
    assert_eq!(f.parts.len(), 2);
    let a = |faces_: &[usize], m: &str| PartPmi {
        features: vec![faces(faces_), faces(&[faces_[0] + 1])],
        datums: vec![datum("A", 0)],
        tolerances: vec![{
            let mut t = tolerance(
                ToleranceKind::Perpendicularity,
                ToleranceTarget::Feature(FeatureId(1)),
                m,
            );
            t.datums = Some(system(&[0]));
            t
        }],
        ..PartPmi::default()
    };
    let (p0, p1) = (a(&[0], "0.05"), a(&[1], "0.07"));
    let w = write(
        &f,
        &[(PartId(1), p1.clone()), (PartId(0), p0.clone())],
        Mode::Replace,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert!(matches!(w.report.schema, SchemaChange::Upgraded { .. }));
    assert_eq!(w.doc.file_schema().unwrap(), [express::TARGET_SCHEMA]);
    assert_reads_back(&w, PartId(0), &p0);
    assert_reads_back(&w, PartId(1), &p1);
    let datums = w
        .added()
        .into_iter()
        .filter(|&id| w.names(id) == ["datum"])
        .count();
    assert_eq!(datums, 2, "datum A once per part");

    // Replace part 0 only.
    let bytes = w.doc.bytes().to_vec();
    let g = open_bytes(bytes);
    let p0b = a(&[2], "0.01");
    let w2 =
        write(&g, &[(PartId(0), p0b.clone())], Mode::Replace).unwrap_or_else(|e| panic!("{e}"));
    assert_reads_back(&w2, PartId(0), &p0b);
    assert!(differences_as_stated(&p1, &w2.read.parts[1]).is_empty());
    let r = pmi::read(&g.doc, &g.parts).unwrap();
    for id in r.provenance.parts[1].all() {
        assert_eq!(
            g.doc.bytes()[g.doc.span(id).unwrap()],
            w2.doc.bytes()[w2.doc.span(id).unwrap()],
            "part 1's #{id} changed"
        );
    }
    assert_eq!(
        w2.report.schema,
        SchemaChange::Kept(vec![express::TARGET_SCHEMA.to_string()])
    );
}

fn open_bytes(bytes: Vec<u8>) -> File {
    let parts = read_part_definitions(&bytes).unwrap();
    let doc = Document::parse(bytes).unwrap();
    File { parts, doc }
}

/// Threads, knurls and the general tolerance appended as raw text: written as `thread`,
/// `turned_knurl` and the 'default tolerances' class through the emission layer, their
/// WHERE rules checked, read back from those entities. A `default_tolerance_table` (an ISO
/// 2768-1 or an ASME-style one) is read, not written (decision 3): refused by name.
#[test]
fn threads_knurls_and_general_tolerances_are_standard_entities() {
    let p = PartPmi {
        features: vec![faces(&[0]), faces(&[1]), faces(&[2])],
        threads: vec![Thread {
            feature: FeatureId(0),
            partial_area: Some(FeatureId(0)),
            side: ThreadSide::Internal,
            major_diameter: length("8.", LengthUnit::Millimetre),
            minor_diameter: Some(length("6.647", LengthUnit::Millimetre)),
            pitch_diameter: Some(length("7.188", LengthUnit::Millimetre)),
            number_of_threads: Ratio(dec("1.")),
            form: "M".into(),
            fit_class: "6H".into(),
            fit_class_2: None,
            hand: Hand::Right,
            crest: None,
            qualifier: None,
            nominal_size: None,
            runout: Some(FeatureId(1)),
        }],
        knurls: vec![Knurl {
            feature: FeatureId(2),
            pattern: KnurlPattern::Diamond,
            major_diameter: length("20.", LengthUnit::Millimetre),
            nominal_diameter: length("19.8", LengthUnit::Millimetre),
            diametral_pitch: length("0.8", LengthUnit::Millimetre),
            number_of_teeth: Some(Count(78)),
            tooth_depth: Some(length("0.4", LengthUnit::Millimetre)),
            root_fillet: None,
            helix_angle: Some(Angle {
                value: dec("30."),
                unit: AngleUnit::Degree,
            }),
            helix_hand: None,
        }],
        general: vec![GeneralTolerance::Class {
            text: "ISO 2768-mK".into(),
            standard: pmi::standards::Iso2768::recognise("ISO 2768-mK"),
        }],
        ..PartPmi::default()
    };
    let w = written(&spool(), &p);
    let names: BTreeSet<Vec<String>> = w.added().into_iter().map(|id| w.names(id)).collect();
    for e in ["thread", "turned_knurl", "applied_area", "thread_runout"] {
        assert!(names.contains(&vec![e.to_string()]), "no {e}");
    }

    for table in [
        GeneralTolerance::Table {
            name: "ISO 2768-1 m".into(),
            cells: Vec::new(),
        },
        GeneralTolerance::Table {
            name: "ASME Y14.5 2 places".into(),
            cells: Vec::new(),
        },
    ] {
        let p = PartPmi {
            general: vec![table],
            ..PartPmi::default()
        };
        match write(&spool(), &[(PartId(0), p)], Mode::Replace) {
            Err(WriteError::Refused(r)) => {
                assert_eq!(r.len(), 1);
                assert_eq!(r[0].item, ItemRef::General(0));
                assert!(r[0].why.contains("decision 3"), "{}", r[0].why);
            }
            other => panic!("a table is refused: {:?}", other.err()),
        }
    }
}

fn thread_on(feature: usize, side: ThreadSide, runout: Option<usize>) -> Thread {
    Thread {
        feature: FeatureId(feature),
        partial_area: Some(FeatureId(feature)),
        side,
        major_diameter: length("6.", LengthUnit::Millimetre),
        minor_diameter: Some(length("4.917", LengthUnit::Millimetre)),
        pitch_diameter: Some(length("5.35", LengthUnit::Millimetre)),
        number_of_threads: Ratio(dec("1.")),
        form: "M".into(),
        fit_class: if side == ThreadSide::Internal {
            "6H"
        } else {
            "6g"
        }
        .into(),
        fit_class_2: None,
        hand: Hand::Right,
        crest: None,
        qualifier: None,
        nominal_size: None,
        runout: runout.map(FeatureId),
    }
}

/// The schema violations (`express::validate_document`) and rule violations
/// (`express_rules::check_all`) of `after` that `before` does not have.
fn new_violations(before: &Document, after: &Document) -> Vec<String> {
    let schema = express::validate_document(before.graph());
    let rules = haecceity::express_rules::check_all(before);
    let mut out: Vec<String> = express::validate_document(after.graph())
        .into_iter()
        .filter(|v| !schema.contains(v))
        .map(|v| v.to_string())
        .collect();
    out.extend(
        haecceity::express_rules::check_all(after)
            .into_iter()
            .filter(|v| !rules.contains(v))
            .map(|v| v.to_string()),
    );
    out
}

/// The `product_definition_shape`s of feature definitions (a thread's or knurl's, thread WR13).
fn feature_definition_shapes(doc: &Document) -> Vec<u64> {
    doc.ids()
        .filter(|&id| match doc.get(id).unwrap() {
            RawEntity::Simple {
                name, attributes, ..
            } if name.eq_ignore_ascii_case("product_definition_shape") => {
                matches!(attributes.get(2), Some(Attribute::EntityRef(d))
                    if matches!(doc.get(*d).unwrap(), RawEntity::Simple { name, .. }
                        if express::is_a(&name.to_ascii_lowercase(), "characterized_object")))
            }
            _ => false,
        })
        .collect()
}

/// specify-core-rust's U14 PMI on the spool's first four free faces: an internal thread (no
/// runout); an external thread with a runout and a straight knurl; and other PMI (a flatness)
/// to replace either with.
struct ThreadSets {
    internal: PartPmi,
    external_and_knurl: PartPmi,
    other: PartPmi,
}

fn thread_sets(f: &File) -> ThreadSets {
    let free = free_faces(f, 0, 4);
    ThreadSets {
        internal: PartPmi {
            features: vec![faces(&[free[0]])],
            threads: vec![thread_on(0, ThreadSide::Internal, None)],
            ..PartPmi::default()
        },
        external_and_knurl: PartPmi {
            features: vec![faces(&[free[0]]), faces(&[free[1]]), faces(&[free[2]])],
            threads: vec![thread_on(0, ThreadSide::External, Some(1))],
            knurls: vec![Knurl {
                feature: FeatureId(2),
                pattern: KnurlPattern::Straight,
                major_diameter: length("20.", LengthUnit::Millimetre),
                nominal_diameter: length("19.8", LengthUnit::Millimetre),
                diametral_pitch: length("0.8", LengthUnit::Millimetre),
                number_of_teeth: Some(Count(78)),
                tooth_depth: Some(length("0.4", LengthUnit::Millimetre)),
                root_fillet: Some(length("0.1", LengthUnit::Millimetre)),
                helix_angle: None,
                helix_hand: None,
            }],
            ..PartPmi::default()
        },
        other: PartPmi {
            features: vec![faces(&[free[3]])],
            tolerances: vec![tolerance(
                ToleranceKind::Flatness,
                ToleranceTarget::Feature(FeatureId(0)),
                "0.05",
            )],
            ..PartPmi::default()
        },
    }
}

/// A part with a thread haecceity wrote is written again (specify-core-rust's U14): read back
/// with nothing unconsumed (thread WR16's bare 'thread runout' aspect is the thread's), then
/// replaced with other PMI, replaced with the other thread set, and removed. Each write takes
/// the feature definition's `product_definition_shape` with its thread, is schema- and
/// rule-valid, and verifies (`pmi::verify`: exactly the items written, no consumed instance
/// surviving).
#[test]
fn a_written_thread_is_replaced_and_removed() {
    let f = spool();
    let s = thread_sets(&f);
    for (case, p, swap) in [
        ("internal", &s.internal, &s.external_and_knurl),
        ("external and knurl", &s.external_and_knurl, &s.internal),
    ] {
        let w = written(&f, p);
        assert_verified(&f, &w, p);
        assert!(
            w.read
                .findings
                .iter()
                .all(|x| x.kind != FindingKind::Unconsumed),
            "{case}: {:#?}",
            w.read.findings
        );
        let v = new_violations(&f.doc, &w.doc);
        assert!(v.is_empty(), "{case}: {v:#?}");
        let shapes = feature_definition_shapes(&w.doc);
        assert_eq!(shapes.len(), p.threads.len() + p.knurls.len(), "{case}");

        let g = open_bytes(w.doc.bytes().to_vec());
        for (mode, q) in [
            (Mode::Replace, &s.other),
            (Mode::Replace, swap),
            (Mode::Remove, &PartPmi::default()),
        ] {
            let w2 = write(&g, &[(PartId(0), q.clone())], mode)
                .unwrap_or_else(|e| panic!("{case} {mode:?}: {e}"));
            assert_reads_back(&w2, PartId(0), q);
            assert_verified_as(&g, &w2, q, mode);
            let v = new_violations(&g.doc, &w2.doc);
            assert!(v.is_empty(), "{case} {mode:?}: {v:#?}");
            for &id in &shapes {
                assert!(w2.doc.get(id).is_none(), "{case} {mode:?}: #{id} kept");
            }
            assert_eq!(
                feature_definition_shapes(&w2.doc).len(),
                q.threads.len() + q.knurls.len(),
                "{case} {mode:?}"
            );
        }
    }
}

/// The removal plan takes a feature definition's `product_definition_shape` only with what
/// uses it (`product_definition_shape` WR1 and UR1: the shape of a characterized object is that
/// object's), and still refuses a part's own shape as shared infrastructure and a feature
/// definition's shape that kept instances reference.
#[test]
fn removal_takes_a_feature_definitions_shape_only_with_its_feature() {
    use haecceity::removal::{self, PresentationPolicy as Policy};
    let f = spool();
    let free = free_faces(&f, 0, 1);
    let p = PartPmi {
        features: vec![faces(&[free[0]])],
        threads: vec![thread_on(0, ThreadSide::Internal, None)],
        ..PartPmi::default()
    };
    let w = written(&f, &p);
    let g = open_bytes(w.doc.bytes().to_vec());
    let [shape] = feature_definition_shapes(&g.doc)[..] else {
        panic!("one feature definition shape")
    };
    let thread: BTreeSet<u64> = w.read.provenance.parts[0].threads[0]
        .iter()
        .copied()
        .collect();
    assert!(
        thread.contains(&shape),
        "the reader consumes it with its thread"
    );
    for policy in [Policy::Refuse, Policy::RemovePresentation] {
        let plan =
            removal::plan(&g.doc, &thread, policy).unwrap_or_else(|e| panic!("{policy:?}: {e}"));
        assert!(plan.removed.contains(&shape), "{policy:?}");

        // Alone, the thread's aspects (kept) still reference it.
        let alone = removal::plan(&g.doc, &BTreeSet::from([shape]), policy).unwrap_err();
        assert!(alone.infrastructure.is_empty(), "{policy:?}: {alone}");
        assert!(!alone.blockers.is_empty(), "{policy:?}: {alone}");

        // The part's own shape stays shared infrastructure.
        let own = g.parts[0].shape;
        let refused = removal::plan(&g.doc, &BTreeSet::from([own]), policy).unwrap_err();
        assert_eq!(
            refused.infrastructure,
            [(own, "product_definition_shape".to_string())],
            "{policy:?}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Editing: add, replace, remove; refusals; determinism
// ---------------------------------------------------------------------------------------------

/// Faces of `part` no usage of the file identifies (so new features take them without
/// sharing an item with the file's own), the first `n`.
fn free_faces(f: &File, part: usize, n: usize) -> Vec<usize> {
    let mut out = Vec::new();
    for (i, &face) in f.parts[part].faces.iter().enumerate() {
        let used = f.doc.referrers(face).iter().any(|&r| {
            let names: Vec<String> = match f.doc.get(r).unwrap() {
                RawEntity::Simple { name, .. } => vec![name.to_ascii_lowercase()],
                RawEntity::Complex { parts, .. } => {
                    parts.iter().map(|p| p.name.to_ascii_lowercase()).collect()
                }
            };
            names
                .iter()
                .any(|n| express::is_a(n, "item_identified_representation_usage"))
        });
        if !used {
            out.push(i);
            if out.len() == n {
                break;
            }
        }
    }
    assert_eq!(out.len(), n, "not enough free faces");
    out
}

/// A set of PMI to add beside a file's own: a datum on a free face, a fit, a perpendicularity
/// to the new datum.
fn addition(f: &File, part: usize, existing: &PartPmi) -> PartPmi {
    let free = free_faces(f, part, 2);
    let mut l = "Z".to_string();
    while existing.datums.iter().any(|d| d.label().as_str() == l) {
        l.push('Z');
    }
    let mut perp = tolerance(
        ToleranceKind::Perpendicularity,
        ToleranceTarget::Dimension(DimensionId(0)),
        "0.05",
    );
    perp.datums = Some(system(&[0]));
    PartPmi {
        features: vec![faces(&[free[0]]), faces(&[free[1]])],
        datums: vec![datum(&l, 0)],
        dimensions: vec![size(
            1,
            "10.",
            DimTolerance::Fit {
                class: fit("H7"),
                limits: None,
            },
        )],
        tolerances: vec![perp],
        ..PartPmi::default()
    }
}

/// Byte preservation: PMI added onto every NIST AP242 file and every specify-core input
/// leaves every original instance byte for byte, the original PMI reads back unchanged (the
/// new items besides), and FILE_SCHEMA as it was.
#[test]
fn add_keeps_every_original_byte() {
    let mut files: Vec<PathBuf> = std::fs::read_dir(common::fixtures().join("ap242/specify"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().ends_with(".step.gz"))
        .collect();
    // The committed NIST models, and the rest of NIST's set when HAECCEITY_NIST_PMI names it.
    let committed: Vec<PathBuf> = std::fs::read_dir(common::fixtures().join("ap242/nist"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().ends_with(".stp.gz"))
        .collect();
    let name = |p: &Path| p.file_name().unwrap().to_string_lossy().replace(".gz", "");
    let committed_names: BTreeSet<String> = committed.iter().map(|p| name(p)).collect();
    files.extend(committed);
    files.extend(
        nist_files()
            .into_iter()
            .filter(|p| !committed_names.contains(&name(p))),
    );
    files.sort();
    let mut problems = Vec::new();
    for path in files {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let bytes = file_bytes(&path);
        let Ok(parts) = read_part_definitions(&bytes) else {
            continue;
        };
        let f = File {
            parts,
            doc: Document::parse(bytes).unwrap(),
        };
        let r0 = pmi::read(&f.doc, &f.parts).unwrap();
        let add = addition(&f, 0, &r0.parts[0]);
        let w = match write(&f, &[(PartId(0), add.clone())], Mode::Add) {
            Ok(w) => w,
            Err(e) => {
                problems.push(format!("{name}: {e}"));
                continue;
            }
        };
        for id in f.doc.ids() {
            if f.doc.bytes()[f.doc.span(id).unwrap()] != w.doc.bytes()[w.doc.span(id).unwrap()] {
                problems.push(format!("{name}: #{id} changed"));
            }
        }
        if w.doc.file_schema() != f.doc.file_schema() {
            problems.push(format!("{name}: FILE_SCHEMA changed"));
        }
        for (i, p) in r0.parts.iter().enumerate() {
            let d: Vec<String> = differences_as_stated(p, &w.read.parts[i])
                .into_iter()
                .filter(|d| d.contains("only in the first"))
                .collect();
            if !d.is_empty() {
                problems.push(format!(
                    "{name} part {i}: original PMI lost: {}",
                    d.join("; ")
                ));
            }
        }
        // What was added reads back: the original's items plus exactly these.
        let mut expect = r0.parts[0].clone();
        merge(&mut expect, &add);
        let d = differences_as_stated(&expect, &w.read.parts[0]);
        if !d.is_empty() {
            problems.push(format!("{name}: added PMI differs: {}", d.join("; ")));
        }
        if w.read
            .findings
            .iter()
            .any(|x| x.ids.iter().any(|&id| id > w.max))
        {
            problems.push(format!("{name}: findings on added instances"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// `b`'s items appended to `a` (indices shifted).
fn merge(a: &mut PartPmi, b: &PartPmi) {
    let (nf, nd, nn) = (a.features.len(), a.datums.len(), a.dimensions.len());
    let f = |x: FeatureId| FeatureId(x.0 + nf);
    a.features.extend(b.features.iter().cloned());
    for d in &b.datums {
        a.datums
            .push(Datum::new(d.label().clone(), d.feature().map(f), Vec::new()).unwrap());
    }
    for d in &b.dimensions {
        let mut d = d.clone();
        if let DimensionKind::Size { feature, .. } = &mut d.kind {
            *feature = f(*feature);
        }
        a.dimensions.push(d);
    }
    for t in &b.tolerances {
        let mut t = t.clone();
        t.target = match t.target {
            ToleranceTarget::Feature(x) => ToleranceTarget::Feature(f(x)),
            ToleranceTarget::Dimension(x) => ToleranceTarget::Dimension(DimensionId(x.0 + nn)),
            other => other,
        };
        if let Some(s) = &t.datums {
            t.datums = Some(
                DatumSystem::new(
                    s.compartments()
                        .iter()
                        .map(|c| Compartment {
                            references: c
                                .references
                                .iter()
                                .map(|r| DatumReference {
                                    datum: DatumId(r.datum.0 + nd),
                                    modifiers: r.modifiers.clone(),
                                })
                                .collect(),
                            modifiers: c.modifiers.clone(),
                        })
                        .collect(),
                )
                .unwrap(),
            );
        }
        a.tolerances.push(t);
    }
}

/// Add resolves a label the part already has to its datum (same faces); other faces are
/// refused by name.
#[test]
fn add_resolves_an_existing_datum_label() {
    let f = spool();
    let r0 = pmi::read(&f.doc, &f.parts).unwrap();
    let old = &r0.parts[0];
    let a = old
        .datums
        .iter()
        .find(|d| d.label().as_str() == "A")
        .expect("datum A");
    let fa = old.features[a.feature().unwrap().0].clone();
    let free = free_faces(&f, 0, 1);
    let mut t = tolerance(
        ToleranceKind::Parallelism,
        ToleranceTarget::Feature(FeatureId(1)),
        "0.04",
    );
    t.datums = Some(system(&[0]));
    let add = PartPmi {
        features: vec![fa, faces(&free)],
        datums: vec![datum("A", 0)],
        tolerances: vec![t],
        ..PartPmi::default()
    };
    let w = write(&f, &[(PartId(0), add)], Mode::Add).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(w.report.reused_datums.len(), 1);
    assert!(w.added().into_iter().all(|id| w.names(id) != ["datum"]));
    let after = &w.read.parts[0];
    assert_eq!(after.datums.len(), old.datums.len());
    let par = after
        .tolerances
        .iter()
        .find(|t| t.kind == ToleranceKind::Parallelism)
        .unwrap();
    let ds = par.datums.as_ref().unwrap();
    assert_eq!(
        after.datums[ds.compartments()[0].references[0].datum.0]
            .label()
            .as_str(),
        "A"
    );

    let other = PartPmi {
        features: vec![faces(&free)],
        datums: vec![datum("A", 0)],
        ..PartPmi::default()
    };
    match write(&f, &[(PartId(0), other)], Mode::Add) {
        Err(WriteError::Refused(r)) => {
            assert_eq!(r[0].item, ItemRef::Datum(0));
            assert!(r[0].why.contains("other faces"), "{}", r[0].why);
        }
        other => panic!("refused: {:?}", other.err()),
    }
}

/// Remove on a specify-core-written file removes exactly its PMI: nothing is read afterwards,
/// every instance the reader did not consume stays byte for byte (presentation containers
/// rewritten, listed), and what the reader consumed is gone.
#[test]
fn remove_takes_exactly_the_parts_pmi() {
    for name in ["spool_fits", "bolt_thread_knurl", "assembly_plate_pin"] {
        let f = fixture(&format!("ap242/specify/{name}.step.gz"));
        let r0 = pmi::read(&f.doc, &f.parts).unwrap();
        let all: Vec<(PartId, PartPmi)> = (0..f.parts.len())
            .map(|i| (PartId(i), PartPmi::default()))
            .collect();
        let w = write(&f, &all, Mode::Remove).unwrap_or_else(|e| panic!("{name}: {e}"));
        for (i, p) in w.read.parts.iter().enumerate() {
            assert_eq!(p, &PartPmi::default(), "{name} part {i} keeps PMI");
        }
        let consumed: BTreeSet<u64> = r0.provenance.parts.iter().flat_map(|p| p.all()).collect();
        let removed: BTreeSet<u64> = w.report.removed.iter().copied().collect();
        assert!(
            consumed.is_subset(&removed),
            "{name}: consumed instances survive"
        );
        let rewritten: BTreeSet<u64> = w.report.rewritten.iter().copied().collect();
        for id in f.doc.ids() {
            if removed.contains(&id) || rewritten.contains(&id) {
                assert!(w.doc.get(id).is_none() || rewritten.contains(&id));
                continue;
            }
            assert_eq!(
                f.doc.bytes()[f.doc.span(id).unwrap()],
                w.doc.bytes()[w.doc.span(id).unwrap()],
                "{name}: #{id} changed"
            );
        }
        // Everything removed beyond the consumed PMI is presentation or the PMI's own parts.
        for id in removed.difference(&consumed) {
            let fam = haecceity::removal::instance_family(&f.doc, *id);
            assert_ne!(
                fam,
                express::Family::SemanticPmi,
                "{name}: unconsumed PMI #{id} removed"
            );
        }
        assert!(w.added().is_empty());
    }
}

/// Every refusal of every part at once, each naming its item.
#[test]
fn refusals_name_every_item() {
    let f = spool();
    let p = PartPmi {
        features: vec![
            faces(&[0]),
            Feature::Group {
                members: vec![FeatureId(0)],
                kind: GroupKind::Unstated,
            },
        ],
        datum_targets: vec![
            DatumTarget::new(1, TargetShape::Area(FeatureId(0)), None, None, None).unwrap(),
        ],
        datums: vec![Datum::new(label("A"), None, vec![DatumTargetId(0)]).unwrap()],
        notes: vec![Note {
            kind: "surface texture".into(),
            text: "Ra 1.6".into(),
            on: None,
        }],
        material: Some(Material {
            id: "steel".into(),
            name: None,
            density: Some(Density {
                value: dec("7.85"),
                unit: vec![("gram".into(), dec("1."))],
            }),
        }),
        decimal_places: Some(3),
        dimensions: vec![Dimension {
            nominal: Some(Value::length(dec("35"), LengthUnit::Millimetre)),
            ..size(0, "1.", DimTolerance::None)
        }],
        ..PartPmi::default()
    };
    match write(&f, &[(PartId(0), p)], Mode::Replace) {
        Err(WriteError::Refused(r)) => {
            let items: BTreeSet<ItemRef> = r.iter().map(|x| x.item).collect();
            for i in [
                ItemRef::Feature(1),
                ItemRef::DatumTarget(0),
                ItemRef::Datum(0),
                ItemRef::Note(0),
                ItemRef::Material,
                ItemRef::DecimalPlaces,
                ItemRef::Dimension(0),
            ] {
                assert!(items.contains(&i), "{i:?} not refused: {r:#?}");
            }
        }
        other => panic!("refused: {:?}", other.err()),
    }
}

/// Determinism: the same input gives the same bytes (each write's maps hash with fresh seeds).
#[test]
fn writes_are_deterministic() {
    let f = fixture("ap242/nist/nist_stc_06_asme1_ap242-e3.stp.gz");
    let r = pmi::read(&f.doc, &f.parts).unwrap();
    let mut p = r.parts[0].clone();
    p.tolerance_relations.clear();
    let run = || {
        let (edit, _) = pmi::write(
            &f.doc,
            &f.parts,
            &[(PartId(0), p.clone())],
            Mode::Replace,
            PresentationPolicy::RemovePresentation,
        )
        .unwrap();
        f.doc.apply(&edit).unwrap().bytes
    };
    let a = run();
    let b = run();
    assert!(a == b, "two writes differ");
}

/// NIST's MBE PMI test models: the `.stp` files of the directory `HAECCEITY_NIST_PMI` names
/// (NIST's `NIST-PMI-STEP-Files`); none when it is not set, unless
/// `HAECCEITY_NIST_PMI_REQUIRED` is.
fn nist_files() -> Vec<PathBuf> {
    let Some(dir) = std::env::var_os("HAECCEITY_NIST_PMI").map(PathBuf::from) else {
        assert!(
            std::env::var_os("HAECCEITY_NIST_PMI_REQUIRED").is_none(),
            "HAECCEITY_NIST_PMI_REQUIRED is set but HAECCEITY_NIST_PMI is not"
        );
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "stp"))
        .collect();
    out.sort();
    out
}

// ---------------------------------------------------------------------------------------------
// Corpus: specify-core's intents written onto the original files, compared with what
// specify-core wrote
// ---------------------------------------------------------------------------------------------

type Json = serde_json::Value;

fn num(v: &Json) -> Decimal {
    // JSON numbers as their shortest decimal, written with a point (a Part 21 REAL).
    let t = v.to_string();
    let t = if t.contains('.') || t.contains('e') || t.contains('E') {
        t
    } else {
        format!("{t}.")
    };
    Decimal::parse(&t).unwrap()
}

/// specify-core's intent for one part as a [`PartPmi`] (millimetres, specify-core's face
/// numbering), and what of it has no standard form (refused here, compared nowhere).
fn intent_pmi(intent: &Json) -> (PartPmi, Vec<String>) {
    let mut p = PartPmi::default();
    let mut not_written = Vec::new();
    let feature = |p: &mut PartPmi, faces_: &Json| -> FeatureId {
        let f = faces(
            &faces_
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_u64().unwrap() as usize)
                .collect::<Vec<_>>(),
        );
        if let Some(i) = p.features.iter().position(|x| *x == f) {
            return FeatureId(i);
        }
        p.features.push(f);
        FeatureId(p.features.len() - 1)
    };
    let mut labels: Vec<String> = Vec::new();
    for d in intent["datums"].as_array().unwrap() {
        let f = feature(&mut p, &d["faces"]);
        let l = d["letter"].as_str().unwrap();
        labels.push(l.to_string());
        p.datums
            .push(Datum::new(label(l), Some(f), Vec::new()).unwrap());
    }
    let datums = |r: &Json| -> Option<DatumSystem> {
        let ls = r["datums"].as_array()?;
        Some(system(
            &ls.iter()
                .map(|l| {
                    labels
                        .iter()
                        .position(|x| x == l.as_str().unwrap())
                        .unwrap()
                })
                .collect::<Vec<_>>(),
        ))
    };
    let mm_n = |v: &Json| Value::length(num(v), LengthUnit::Millimetre);
    for r in intent["requirements"].as_array().unwrap() {
        let kind = r["kind"].as_str().unwrap();
        match kind {
            "size" => {
                let f = feature(&mut p, &r["faces"]);
                let tolerance = match r["fit"].as_str() {
                    Some(c) => DimTolerance::Fit {
                        class: fit(c),
                        limits: None,
                    },
                    None if r["upper"].is_number() => DimTolerance::Deviations(
                        Bounds::new(mm_n(&r["upper"]), mm_n(&r["lower"])).unwrap(),
                    ),
                    None => DimTolerance::None,
                };
                p.dimensions.push(Dimension {
                    nominal: Some(mm_n(&r["nominal"])),
                    ..size(f.0, "1.", tolerance)
                });
            }
            "location" => {
                let to = feature(&mut p, &r["faces"]);
                let from = feature(&mut p, &r["reference"]);
                p.dimensions.push(Dimension {
                    kind: DimensionKind::Location {
                        from,
                        to,
                        kind: LocationKind::LinearDistance,
                        path: None,
                        directed: false,
                        angle: None,
                    },
                    nominal: Some(mm_n(&r["distance"])),
                    tolerance: if r["basic"].as_bool() == Some(true) {
                        DimTolerance::Basic
                    } else {
                        DimTolerance::None
                    },
                    qualifier: None,
                    modifiers: Vec::new(),
                    principle: None,
                });
            }
            "position" | "runout" | "flatness" | "perpendicularity" | "parallelism" | "profile" => {
                let f = feature(&mut p, &r["faces"]);
                let k = match kind {
                    "position" => ToleranceKind::Position,
                    "runout" => ToleranceKind::CircularRunout,
                    "flatness" => ToleranceKind::Flatness,
                    "perpendicularity" => ToleranceKind::Perpendicularity,
                    "parallelism" => ToleranceKind::Parallelism,
                    _ => ToleranceKind::SurfaceProfile,
                };
                let mut t = tolerance(k, ToleranceTarget::Feature(f), "1.");
                t.magnitude = Some(mm_n(&r["tolerance"]));
                t.datums = datums(r);
                if r["diametral"].as_bool() == Some(true) {
                    t.zone = Some(Zone {
                        form: ZoneForm::CylindricalOrCircular,
                        projected: None,
                        non_uniform: None,
                        runout_angle: None,
                        affected_plane: None,
                    });
                }
                if r["mmc"].as_bool() == Some(true) {
                    t.modifiers
                        .push(ToleranceModifier::MaximumMaterialRequirement);
                }
                p.tolerances.push(t);
            }
            "thread" => {
                let f = feature(&mut p, &r["faces"]);
                let designation = r["designation"].as_str().unwrap();
                let major = num(&r.get("nominal").cloned().unwrap_or_else(|| {
                    serde_json::from_str(designation.trim_start_matches('M')).unwrap()
                }));
                p.threads.push(Thread {
                    feature: f,
                    partial_area: Some(f),
                    side: if r["side"] == "internal" {
                        ThreadSide::Internal
                    } else {
                        ThreadSide::External
                    },
                    major_diameter: Length {
                        value: major.clone(),
                        unit: LengthUnit::Millimetre,
                    },
                    minor_diameter: None,
                    pitch_diameter: None,
                    number_of_threads: Ratio(dec("1.")),
                    form: "M".into(),
                    fit_class: r["class"].as_str().unwrap().into(),
                    fit_class_2: None,
                    hand: if r["hand"] == "LH" {
                        Hand::Left
                    } else {
                        Hand::Right
                    },
                    crest: None,
                    qualifier: Some(r["spec"].as_str().unwrap().into()),
                    nominal_size: Some(Length {
                        value: major,
                        unit: LengthUnit::Millimetre,
                    }),
                    runout: None,
                });
                // Pitch, tapping drill and depths are not thread semantics (design: Threads
                // and knurls): a user defined attribute set on the thread's feature.
                let mut items = vec![(
                    "pitch".to_string(),
                    AttributeValue::Measure(mm_n(&r["pitch"])),
                )];
                for (k, n) in [
                    ("drill_diameter", "tapping drill diameter"),
                    ("drill_depth", "tapping drill depth"),
                    ("full_thread", "full thread depth"),
                    ("length", "thread length"),
                ] {
                    if r[k].is_number() {
                        items.push((n.to_string(), AttributeValue::Measure(mm_n(&r[k]))));
                    }
                }
                if r["through"].is_boolean() {
                    not_written.push(format!(
                        "thread on {}: 'through' is a boolean, which the writer refuses (the reader cannot read it back)",
                        r["faces"]
                    ));
                }
                p.attributes.push(AttributeSet {
                    name: "thread manufacturing".into(),
                    on: Some(NoteOwner::Feature(f)),
                    items,
                });
            }
            other => not_written.push(format!(
                "{other} on {}: the intent lacks parameters its standard entity requires",
                r["faces"]
            )),
        }
    }
    let part = &intent["part"];
    if let Some(m) = part["material"].as_str() {
        p.material = Some(Material {
            id: m.into(),
            name: None,
            density: None,
        });
    }
    if let Some(g) = part["general_tolerance"].as_str() {
        p.general.push(GeneralTolerance::Class {
            text: g.into(),
            standard: pmi::standards::Iso2768::recognise(g),
        });
    }
    for k in ["surface_finish", "edges"] {
        if part[k].is_string() {
            not_written.push(format!("{k}: no standard form in the model (decision 5)"));
        }
    }
    (p, not_written)
}

/// The original file of a specify-core case.
fn original(input: &str) -> Option<PathBuf> {
    let (kind, rel) = input.split_once(':').unwrap();
    match kind {
        "corpus" => {
            let dir = common::corpus_dir();
            if dir.is_none() {
                assert!(
                    std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
                    "QUIDDITY_CORPUS_REQUIRED is set but the corpus is missing"
                );
            }
            dir.map(|d| d.join(rel))
        }
        "assembly" => Some(common::fixtures().join("ap242/assembly").join(rel)),
        "nist" => Some(
            common::fixtures()
                .join("ap242/nist")
                .join(format!("{rel}.gz")),
        ),
        _ => panic!("{input}"),
    }
}

/// For every specify-core case: its intent written onto the ORIGINAL file (AP214 files become
/// AP242, decision 1, each pinned upgradable in corpus_upgrade.json) reads back as written;
/// compared with what specify-core wrote, every difference matches a pinned pattern with a
/// verdict (`known_pmi_write.json` "corpus").
#[test]
fn specify_intents_written_onto_the_original_files() {
    let pins = load_pins();
    let patterns = pins["corpus"]["patterns"].as_array().unwrap();
    common::check_verdicts("known_pmi_write.json corpus", patterns);
    let mut problems = Vec::new();
    let mut actual = serde_json::Map::new();
    let dir = common::fixtures().join("ap242/specify");
    let mut cases: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .filter_map(|n| n.strip_suffix(".meta.json").map(str::to_string))
        .collect();
    cases.sort();
    for case in cases {
        let meta: Json = serde_json::from_str(
            &std::fs::read_to_string(dir.join(format!("{case}.meta.json"))).unwrap(),
        )
        .unwrap();
        let Some(path) = original(meta["input"].as_str().unwrap()) else {
            continue;
        };
        let intents: Json = serde_json::from_str(
            &std::fs::read_to_string(dir.join(format!("{case}.intent.json"))).unwrap(),
        )
        .unwrap();
        let intents: Vec<Json> = match intents {
            Json::Array(a) => a,
            one => vec![one],
        };
        let f = open(&path);
        let spec = fixture(&format!("ap242/specify/{case}.step.gz"));
        let spec_read = pmi::read(&spec.doc, &spec.parts).unwrap();
        let mut pmi_ = Vec::new();
        let mut entry = serde_json::Map::new();
        for intent in &intents {
            let part = intent["binding"]["part"].as_u64().unwrap() as usize;
            let (p, not_written) = intent_pmi(intent);
            for n in not_written {
                entry
                    .entry("not written")
                    .or_insert(Json::Array(Vec::new()))
                    .as_array_mut()
                    .unwrap()
                    .push(format!("part {part}: {n}").into());
            }
            pmi_.push((PartId(part), p));
        }
        let w = match write(&f, &pmi_, Mode::Add) {
            Ok(w) => w,
            Err(e) => {
                problems.push(format!("{case}: {e}"));
                continue;
            }
        };
        entry.insert("schema".into(), format!("{:?}", w.report.schema).into());
        let mut diffs: Vec<String> = Vec::new();
        let before = pmi::read(&f.doc, &f.parts).unwrap();
        for (part, p) in &pmi_ {
            if before.parts[part.0] == PartPmi::default() {
                assert_reads_back(&w, *part, p);
            } else {
                // Beside the file's own PMI (NIST CTC-01): what was written is all there.
                let lost: Vec<String> = differences_as_stated(p, &w.read.parts[part.0])
                    .into_iter()
                    .filter(|d| d.contains("only in the first"))
                    .collect();
                assert!(lost.is_empty(), "{case}: {lost:?}");
            }
            // Compared by meaning; and as stated only for values in metres (specify-core's
            // tolerance unit), which are equal in meaning.
            let stated = differences_as_stated(&spec_read.parts[part.0], &w.read.parts[part.0])
                .into_iter()
                .filter(|d| d.contains("Metre"));
            for d in differences(&spec_read.parts[part.0], &w.read.parts[part.0])
                .into_iter()
                .chain(stated)
            {
                let matched = patterns.iter().find(|pt| {
                    pt["match"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|m| d.contains(m.as_str().unwrap()))
                });
                match matched {
                    Some(pt) => {
                        diffs.push(format!("part {}: {}", part.0, pt["key"].as_str().unwrap()))
                    }
                    None => problems.push(format!(
                        "{case} part {}: unexplained difference: {d}",
                        part.0
                    )),
                }
            }
        }
        let mut counts: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        for d in diffs {
            *counts.entry(d).or_insert(0) += 1;
        }
        entry.insert("differences".into(), serde_json::to_value(counts).unwrap());
        actual.insert(case, Json::Object(entry));
    }
    let pinned = pins["corpus"]["cases"].as_object().unwrap();
    for (case, got) in &actual {
        if pinned.get(case) != Some(got) {
            problems.push(format!(
                "corpus {case}:\n  pinned {:?}\n  actual {got}",
                pinned.get(case)
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

fn load_pins() -> Json {
    serde_json::from_str(
        &std::fs::read_to_string(common::fixtures().join("known_pmi_write.json")).unwrap(),
    )
    .unwrap()
}

// ---------------------------------------------------------------------------------------------
// OpenCascade XCAF's reading of written files (adapted from the reader's oracle in pmi_read.rs)
// ---------------------------------------------------------------------------------------------

/// The item's own instance among its provenance: the first of the given entity types.
fn root(doc: &Document, prov: &[u64], types: &[&str]) -> u64 {
    prov.iter()
        .copied()
        .find(|&id| {
            let names: Vec<String> = match doc.get(id).unwrap() {
                haecceity::p21::RawEntity::Simple { name, .. } => vec![name.to_ascii_lowercase()],
                haecceity::p21::RawEntity::Complex { parts, .. } => {
                    parts.iter().map(|p| p.name.to_ascii_lowercase()).collect()
                }
            };
            names
                .iter()
                .any(|n| types.iter().any(|t| haecceity::express::is_a(n, t)))
        })
        .unwrap_or(0)
}

fn cover(p: &PartPmi, f: pmi::FeatureId) -> Vec<pmi::Anchor> {
    let mut out = Vec::new();
    let mut stack = vec![f];
    while let Some(f) = stack.pop() {
        match &p.features[f.0] {
            Feature::Items(a) => out.extend(a.iter().copied()),
            Feature::Group { members, .. } => stack.extend(members.iter().copied()),
            Feature::Derived { from, .. } => stack.extend(from.iter().copied()),
        }
    }
    out.sort();
    out.dedup();
    out
}

fn target_feature(p: &PartPmi, t: &ToleranceTarget) -> Option<pmi::FeatureId> {
    match t {
        ToleranceTarget::Feature(f) => Some(*f),
        ToleranceTarget::Dimension(d) => match &p.dimensions[d.0].kind {
            DimensionKind::Size { feature, .. } => Some(*feature),
            DimensionKind::Location { .. } => None,
        },
        _ => None,
    }
}

/// A value OpenCascade gives against one the reader read: equal in millimetres (degrees for
/// angles), or equal only in the file's own unit (OpenCascade did not convert it).
fn occt_value(occt: f64, v: &pmi::Value) -> Option<&'static str> {
    let close = |a: f64, b: f64| (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0);
    let si = match &v.quantity {
        pmi::Quantity::Length(l) => l.mm(),
        pmi::Quantity::Angle(a) => a.rad().to_degrees(),
    };
    if close(occt, si) {
        None
    } else if close(occt, v.decimal().to_f64()) {
        Some("in the file's unit, not converted")
    } else {
        Some("different")
    }
}

fn face_cover(p: &PartPmi, f: pmi::FeatureId) -> Vec<usize> {
    let mut v: Vec<usize> = cover(p, f)
        .into_iter()
        .map(|a| match a {
            pmi::Anchor::Face(i) => i.0,
            pmi::Anchor::Edge(e) => usize::MAX - e.0,
            pmi::Anchor::Geometry(g) => usize::MAX / 2 - g.0,
        })
        .collect();
    v.sort_unstable();
    v
}

fn occt_dim_type(d: &pmi::Dimension) -> String {
    match &d.kind {
        DimensionKind::Size { angle: Some(_), .. } => "Size_Angular".into(),
        DimensionKind::Size { kind, .. } => {
            let camel: String = kind
                .name()
                .split(' ')
                .map(|w| {
                    let mut c = w.chars();
                    c.next()
                        .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
                        .unwrap_or_default()
                })
                .collect();
            format!("Size_{camel}")
        }
        DimensionKind::Location { angle: Some(_), .. } => "Location_Angular".into(),
        DimensionKind::Location { kind, .. } => match kind {
            pmi::LocationKind::LinearDistance => "Location_LinearDistance".into(),
            pmi::LocationKind::CurvedDistance => "Location_CurvedDistance".into(),
            k => {
                let n = k.name().trim_start_matches("linear distance ");
                let w: Vec<&str> = n.split(' ').collect();
                let cap = |x: &str| {
                    match x {
                        "centre" => "Center",
                        "outer" => "Outer",
                        "inner" => "Inner",
                        o => o,
                    }
                    .to_string()
                };
                match w.as_slice() {
                    [a, b] => format!("Location_LinearDistance_From{}To{}", cap(a), cap(b)),
                    [a] => format!("Location_LinearDistance_{}", cap(a)),
                    _ => format!("Location_{}", k.name()),
                }
            }
        },
    }
}

fn occt_tol_type(k: ToleranceKind) -> &'static str {
    match k {
        ToleranceKind::Angularity => "Angularity",
        ToleranceKind::CircularRunout => "CircularRunout",
        ToleranceKind::Coaxiality => "Coaxiality",
        ToleranceKind::Concentricity => "Concentricity",
        ToleranceKind::Cylindricity => "Cylindricity",
        ToleranceKind::Flatness => "Flatness",
        ToleranceKind::LineProfile => "ProfileOfLine",
        ToleranceKind::Parallelism => "Parallelism",
        ToleranceKind::Perpendicularity => "Perpendicularity",
        ToleranceKind::Position => "Position",
        ToleranceKind::Roundness => "CircularityOrRoundness",
        ToleranceKind::Straightness => "Straightness",
        ToleranceKind::SurfaceProfile => "ProfileOfSurface",
        ToleranceKind::Symmetry => "Symmetry",
        ToleranceKind::TotalRunout => "TotalRunout",
    }
}

/// The differences between OpenCascade's reading of a dimension and one the reader read on the
/// same faces.
fn occt_dim_fields(o: &Json, p: &PartPmi, d: &pmi::Dimension) -> Vec<String> {
    let mut out = Vec::new();
    let t = o["type"].as_str().unwrap();
    if t != occt_dim_type(d) {
        out.push(format!("type {t} vs {}", occt_dim_type(d)));
    }
    let _ = p;
    let num = |k: &str| o[k].as_f64().unwrap_or(0.0);
    let cmp = |what: &str, occt: f64, v: &pmi::Value| {
        occt_value(occt, v)
            .map(|why| format!("{what} {occt} vs {} {} ({why})", v.decimal(), unit_name(v)))
    };
    match &d.tolerance {
        DimTolerance::Limits(b) => {
            if !o["range"].as_bool().unwrap_or(false) {
                out.push("not a range".into());
            }
            out.extend(cmp("lower_bound", num("lower_bound"), b.lower()));
            out.extend(cmp("upper_bound", num("upper_bound"), b.upper()));
        }
        DimTolerance::Deviations(b) => {
            if let Some(n) = &d.nominal {
                out.extend(cmp("value", num("value"), n));
            }
            if !o["plus_minus"].as_bool().unwrap_or(false) {
                out.push(format!(
                    "not plus/minus (range {} lower_bound {} upper_bound {})",
                    o["range"],
                    num("lower_bound"),
                    num("upper_bound")
                ));
            } else {
                out.extend(cmp("upper_tol", num("upper_tol"), b.upper()));
                let mut lower = b.lower().clone();
                // OpenCascade gives the lower deviation as a magnitude below nominal.
                let neg = format!("{}", -b.lower().decimal().to_f64());
                let neg = pmi::Decimal::parse(&neg).unwrap_or_else(|_| b.lower().decimal().clone());
                match &mut lower.quantity {
                    pmi::Quantity::Length(l) => l.value = neg,
                    pmi::Quantity::Angle(a) => a.value = neg,
                }
                out.extend(cmp("lower_tol", num("lower_tol"), &lower));
            }
        }
        DimTolerance::Fit { class, .. } => {
            if let Some(n) = &d.nominal {
                out.extend(cmp("value", num("value"), n));
            }
            if !o["class_of_tolerance"].as_bool().unwrap_or(false) {
                out.push(format!("class {class} not read as a class"));
            } else {
                let c = &o["class"];
                let theirs = format!(
                    "{}{}",
                    c["form_variance"].as_str().unwrap_or(""),
                    c["grade"].as_str().unwrap_or("").trim_start_matches("IT")
                );
                let hole = c["is_hole"].as_u64() == Some(1);
                let ours_hole = class.deviation.of == pmi::FitFeature::Hole;
                if !theirs.eq_ignore_ascii_case(&class.to_string()) || hole != ours_hole {
                    out.push(format!("class {c} vs {class}"));
                }
            }
        }
        DimTolerance::None | DimTolerance::Basic => {
            match &d.nominal {
                Some(n) => out.extend(cmp("value", num("value"), n)),
                // OpenCascade gives 0 where the file states no value.
                None if num("value") == 0.0 => {}
                None => out.push(format!("value {} vs none stated", num("value"))),
            }
            if o["plus_minus"].as_bool().unwrap_or(false) || o["range"].as_bool().unwrap_or(false) {
                out.push("toleranced in OpenCascade".into());
            }
        }
    }
    out
}

fn unit_name(v: &pmi::Value) -> String {
    match &v.quantity {
        pmi::Quantity::Length(l) => format!("{:?}", l.unit),
        pmi::Quantity::Angle(a) => format!("{:?}", a.unit),
    }
}

fn occt_tol_fields(o: &Json, p: &PartPmi, t: &pmi::GeometricTolerance) -> Vec<String> {
    let mut out = Vec::new();
    match &t.magnitude {
        Some(m) => {
            if let Some(why) = occt_value(o["value"].as_f64().unwrap_or(0.0), m) {
                out.push(format!("value {} vs {} ({why})", o["value"], m.decimal()));
            }
        }
        None => out.push(format!("value {} vs none", o["value"])),
    }
    let mm = o["material_modifier"].as_str().unwrap_or("None");
    let ours = if t
        .modifiers
        .contains(&ToleranceModifier::MaximumMaterialRequirement)
    {
        "M"
    } else if t
        .modifiers
        .contains(&ToleranceModifier::LeastMaterialRequirement)
    {
        "L"
    } else {
        "None"
    };
    if mm != ours {
        out.push(format!("material modifier {mm} vs {ours}"));
    }
    let tv = o["type_of_value"].as_str().unwrap_or("None");
    let zone = match &t.zone {
        Some(z) if z.form.is_diametral() => "Diameter",
        Some(z) if z.form.is_spherical() => "SphericalDiameter",
        _ => "None",
    };
    if tv != zone {
        out.push(format!("type of value {tv} vs {zone}"));
    }
    let projected = t.zone.as_ref().and_then(|z| z.projected.as_ref());
    match (o["zone_modifier"].as_str().unwrap_or("None"), projected) {
        ("Projected", Some(pz)) => {
            if let Some(why) = occt_value(o["zone_value"].as_f64().unwrap_or(0.0), &pz.length) {
                out.push(format!(
                    "projected length {} vs {} ({why})",
                    o["zone_value"],
                    pz.length.decimal()
                ));
            }
        }
        ("None", None) => {}
        (z, p) => out.push(format!("zone modifier {z} vs projected {}", p.is_some())),
    }
    let theirs: Vec<(String, u64)> = o["datums"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            (
                d["name"].as_str().unwrap().to_string(),
                d["position"].as_u64().unwrap(),
            )
        })
        .collect();
    let mut ours: Vec<(String, u64)> = Vec::new();
    if let Some(s) = &t.datums {
        for (i, c) in s.compartments().iter().enumerate() {
            for r in &c.references {
                ours.push((p.datums[r.datum.0].label().to_string(), i as u64 + 1));
            }
        }
    }
    if theirs != ours {
        out.push(format!("datums {theirs:?} vs {ours:?}"));
    }
    out
}

/// The part (index) and its anchors that OpenCascade's faces and edges are, by their
/// `advanced_face` / edge instances (OpenCascade's shapes need not be the file's product
/// definitions); faces are numbered, edges as `usize::MAX - index`.
fn occt_faces(
    j: &Json,
    parts: &[haecceity::step::PartDefinition],
    r: &PmiRead,
) -> Option<(usize, Vec<usize>)> {
    let mut part = None;
    let mut out = Vec::new();
    for f in j.as_array().unwrap() {
        let (n, edge) = match f["advanced_face"].as_str() {
            Some(a) => (a, false),
            None => (f["instance"].as_str()?, true),
        };
        let n: u64 = n.trim_start_matches('#').parse().ok()?;
        let (pi, i) = parts.iter().enumerate().find_map(|(pi, p)| {
            if edge {
                p.edge_index(n).map(|i| (pi, usize::MAX - i)).or_else(|| {
                    // An edge OpenCascade made of supplemental geometry the reader holds.
                    r.provenance.parts[pi]
                        .geometry_items
                        .iter()
                        .position(|&g| g == n)
                        .map(|g| (pi, usize::MAX / 2 - g))
                })
            } else {
                p.face_index(n).map(|i| (pi, i))
            }
        })?;
        if part.is_some_and(|q| q != pi) {
            return None;
        }
        part = Some(pi);
        out.push(i);
    }
    out.sort_unstable();
    out.dedup();
    Some((part?, out))
}

/// Oracle 3: each item OpenCascade read (on faces of a part, matched by instance) against the
/// reader's item of that part on the same faces, and the reader's items OpenCascade did not
/// read.
fn compare_occt(
    name: &str,
    occt: &Json,
    r: &PmiRead,
    doc: &Document,
    parts: &[haecceity::step::PartDefinition],
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let all = |k: &str| -> Vec<Json> {
        occt["parts"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|p| p[k].as_array().unwrap().iter().cloned())
            .collect()
    };
    let (odims, otols, odatums) = (
        all("dimensions"),
        all("geometric_tolerances"),
        all("datums"),
    );
    for (pi, p) in r.parts.iter().enumerate() {
        let prov = &r.provenance.parts[pi];
        let dim_id = |i: usize| {
            root(
                doc,
                &prov.dimensions[i],
                &["dimensional_size", "dimensional_location"],
            )
        };
        let tol_id = |i: usize| root(doc, &prov.tolerances[i], &["geometric_tolerance"]);
        let on_part = |j: &Json| {
            occt_faces(j, parts, r)
                .filter(|(q, _)| *q == pi)
                .map(|(_, f)| f)
        };
        let mut dims_left: Vec<bool> = vec![true; p.dimensions.len()];
        let mut callouts: Vec<Vec<usize>> = Vec::new();
        for o in &odims {
            let t = o["type"].as_str().unwrap();
            let label = o["label"].as_str().unwrap();
            if t == "DimensionPresentation" {
                // Presentation only: OpenCascade's record of a callout, no semantics; its faces
                // are compared with the notes' below.
                callouts.extend(on_part(&o["faces"]));
                continue;
            }
            let Some(f1) = on_part(&o["faces"]) else {
                continue;
            };
            let f2 = if o["faces2"].as_array().unwrap().is_empty() {
                Vec::new()
            } else {
                match on_part(&o["faces2"]) {
                    Some(f) => f,
                    None => continue,
                }
            };
            let candidates: Vec<usize> = (0..p.dimensions.len())
                .filter(|&i| match &p.dimensions[i].kind {
                    DimensionKind::Size { feature, .. } => {
                        f2.is_empty() && face_cover(p, *feature) == f1
                    }
                    DimensionKind::Location { from, to, .. } => {
                        let (a, b) = (face_cover(p, *from), face_cover(p, *to));
                        (a == f1 && b == f2) || (a == f2 && b == f1)
                    }
                })
                .collect();
            let mut best = candidates
                .iter()
                .filter(|&&i| dims_left[i])
                .min_by_key(|&&i| occt_dim_fields(o, p, &p.dimensions[i]).len())
                .copied();
            let mut subset = false;
            if best.is_none() && f2.is_empty() {
                // OpenCascade may attach a pattern's dimension to some of its faces only.
                best = (0..p.dimensions.len())
                    .filter(|&i| dims_left[i] && occt_dim_type(&p.dimensions[i]) == t)
                    .filter(|&i| match &p.dimensions[i].kind {
                        DimensionKind::Size { feature, .. } => {
                            let c = face_cover(p, *feature);
                            f1.iter().all(|x| c.contains(x))
                        }
                        DimensionKind::Location { .. } => false,
                    })
                    .min_by_key(|&i| occt_dim_fields(o, p, &p.dimensions[i]).len());
                subset = best.is_some();
            }
            match best {
                Some(i) => {
                    dims_left[i] = false;
                    let mut diff = occt_dim_fields(o, p, &p.dimensions[i]);
                    if subset {
                        let c = match &p.dimensions[i].kind {
                            DimensionKind::Size { feature, .. } => face_cover(p, *feature),
                            DimensionKind::Location { .. } => Vec::new(),
                        };
                        diff.insert(
                            0,
                            format!("on faces {f1:?}, a part of the dimensioned feature's {c:?}"),
                        );
                    }
                    if !diff.is_empty() {
                        out.push((
                            format!("{name} occt dimension {label}"),
                            format!("#{}: {}", dim_id(i), diff.join("; ")),
                        ));
                    }
                }
                None => out.push((
                    format!("{name} occt dimension {label}"),
                    format!(
                        "{t} {} on faces {f1:?} {f2:?}: no dimension read on those faces",
                        o["value"]
                    ),
                )),
            }
        }
        for (i, left) in dims_left.iter().enumerate() {
            if *left {
                out.push((
                    format!("{name} occt missing dimension #{}", dim_id(i)),
                    occt_dim_type(&p.dimensions[i]),
                ));
            }
        }
        // specify-core's notes on faces (U10): OpenCascade links each note's callout to the
        // faces of the aspect the reader puts the note on.
        for n in &p.notes {
            let Some(NoteOwner::Feature(x)) = n.on else {
                continue;
            };
            let c = face_cover(p, x);
            match callouts.iter().position(|f| *f == c) {
                Some(j) => {
                    callouts.remove(j);
                }
                None => out.push((
                    format!("{name} occt note {:?} {c:?}", n.kind),
                    "no callout OpenCascade links to those faces".into(),
                )),
            }
        }
        for c in callouts {
            out.push((
                format!("{name} occt callout {c:?}"),
                "on faces where the reader has no note".into(),
            ));
        }
        let mut tols_left = vec![true; p.tolerances.len()];
        for o in &otols {
            let t = o["type"].as_str().unwrap();
            let label = o["label"].as_str().unwrap();
            let f = if o["faces"].as_array().unwrap().is_empty() {
                if pi != 0 {
                    continue;
                }
                Vec::new()
            } else {
                match on_part(&o["faces"]) {
                    Some(f) => f,
                    None => continue,
                }
            };
            let on_faces = |i: usize, exact: bool| -> bool {
                match target_feature(p, &p.tolerances[i].target) {
                    Some(x) => {
                        let c = face_cover(p, x);
                        !exact && !f.is_empty() && c != f && f.iter().all(|y| c.contains(y))
                    }
                    None => false,
                }
            };
            let mut best = (0..p.tolerances.len())
                .filter(|&i| {
                    tols_left[i]
                        && occt_tol_type(p.tolerances[i].kind) == t
                        && match &p.tolerances[i].target {
                            ToleranceTarget::WholePart => f.is_empty(),
                            tt => target_feature(p, tt).is_some_and(|x| face_cover(p, x) == f)
                                || matches!(tt, ToleranceTarget::Dimension(d) if match &p.dimensions[d.0].kind {
                                    DimensionKind::Location { from, to, .. } => {
                                        let mut c = face_cover(p, *from);
                                        c.extend(face_cover(p, *to));
                                        c.sort_unstable();
                                        c.dedup();
                                        c == f
                                    }
                                    _ => false,
                                }),
                        }
                })
                .min_by_key(|&i| occt_tol_fields(o, p, &p.tolerances[i]).len());
            let mut subset = false;
            if best.is_none() {
                best = (0..p.tolerances.len())
                    .filter(|&i| {
                        tols_left[i]
                            && occt_tol_type(p.tolerances[i].kind) == t
                            && on_faces(i, false)
                    })
                    .min_by_key(|&i| occt_tol_fields(o, p, &p.tolerances[i]).len());
                subset = best.is_some();
            }
            match best {
                Some(i) => {
                    tols_left[i] = false;
                    let mut diff = occt_tol_fields(o, p, &p.tolerances[i]);
                    if subset {
                        let c = target_feature(p, &p.tolerances[i].target)
                            .map(|x| face_cover(p, x))
                            .unwrap_or_default();
                        diff.insert(
                            0,
                            format!("on faces {f:?}, a part of the toleranced feature's {c:?}"),
                        );
                    }
                    if !diff.is_empty() {
                        out.push((
                            format!("{name} occt tolerance {label}"),
                            format!("#{}: {}", tol_id(i), diff.join("; ")),
                        ));
                    }
                }
                None => out.push((
                    format!("{name} occt tolerance {label}"),
                    format!(
                        "{t} {} on faces {f:?}: no tolerance read on those faces",
                        o["value"]
                    ),
                )),
            }
        }
        for (i, left) in tols_left.iter().enumerate() {
            if *left {
                out.push((
                    format!("{name} occt missing tolerance #{}", tol_id(i)),
                    format!("{:?}", p.tolerances[i].kind),
                ));
            }
        }
        // Datums by label and faces (OpenCascade keeps one per tolerance that cites it).
        let mut theirs: Vec<(String, Vec<usize>)> = odatums
            .iter()
            .filter(|d| !d["is_target"].as_bool().unwrap_or(false))
            .filter_map(|d| {
                Some((
                    d["name"].as_str().unwrap().to_string(),
                    on_part(&d["faces"])?,
                ))
            })
            .collect();
        theirs.sort();
        theirs.dedup();
        for (i, d) in p.datums.iter().enumerate() {
            let ours = d.feature().map(|f| face_cover(p, f)).unwrap_or_default();
            let key = (d.label().to_string(), ours.clone());
            if let Some(j) = theirs.iter().position(|x| *x == key) {
                theirs.remove(j);
            } else if d.feature().is_some() {
                let id = root(doc, &prov.datums[i], &["datum"]);
                out.push((
                    format!("{name} occt missing datum {} #{id}", d.label()),
                    format!("faces {ours:?}"),
                ));
            }
        }
        for (l, f) in theirs {
            out.push((
                format!("{name} occt datum {l} {f:?}"),
                "a datum the reader does not have on those faces".into(),
            ));
        }
    }
    // Items on faces of no part the reader has.
    for (k, list) in [("dimension", &odims), ("tolerance", &otols)] {
        for o in list.iter() {
            if o["type"] == "DimensionPresentation" || o["faces"].as_array().unwrap().is_empty() {
                continue;
            }
            if occt_faces(&o["faces"], parts, r).is_none() {
                out.push((
                    format!("{name} occt {k} {}", o["label"].as_str().unwrap()),
                    format!(
                        "{} on faces that are not one part's: {}",
                        o["type"], o["faces"]
                    ),
                ));
            }
        }
    }
    // Notes on the part ('semantic text', §7.4): XCAF's metadata of the part's label, which
    // holds one string per property name; and what it holds that the reader does not.
    let metadata: Vec<&Json> = occt["metadata"].as_array().unwrap().iter().collect();
    for (pi, p) in r.parts.iter().enumerate() {
        let strings = metadata
            .iter()
            .find(|m| m["part"].as_u64() == Some(pi as u64))
            .map(|m| m["strings"].clone())
            .unwrap_or(Json::Null);
        let mut ours: Vec<String> = Vec::new();
        for a in p.attributes.iter().filter(|a| a.on.is_none()) {
            for (item, v) in &a.items {
                let AttributeValue::Text(text) = v else {
                    continue;
                };
                ours.push(text.clone());
                if strings[&a.name].as_str() != Some(text.as_str()) {
                    out.push((
                        format!("{name} occt part {pi} {} {item:?}", a.name),
                        format!(
                            "not in XCAF's metadata, which holds {:?} = {}",
                            a.name, strings[&a.name]
                        ),
                    ));
                }
            }
        }
        if let Json::Object(m) = &strings {
            for (k, v) in m {
                if !ours.iter().any(|t| Some(t.as_str()) == v.as_str()) {
                    out.push((
                        format!("{name} occt part {pi} metadata {k:?}"),
                        format!("{v} is no text the reader has on the part"),
                    ));
                }
            }
        }
        // Surface texture: XCAF has none.
        for (i, s) in p.surface_textures.iter().enumerate() {
            out.push((
                format!("{name} occt part {pi} surface texture {i}"),
                format!(
                    "not read: {} {}",
                    s.material_removal.term(),
                    s.parameters
                        .iter()
                        .map(|x| format!(
                            "{} {}{:?}",
                            x.characteristic, x.value.value, x.value.unit
                        ))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ));
        }
    }
    // Material.
    let materials: Vec<String> = occt["materials"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["name"] != "pmi-assist answers")
        .map(|m| m["name"].as_str().unwrap().to_string())
        .collect();
    let ours: Vec<String> = r
        .parts
        .iter()
        .filter_map(|p| p.material.as_ref().map(|m| m.id.clone()))
        .collect();
    for m in &materials {
        if !ours.contains(m) {
            out.push((format!("{name} occt material {m}"), "not read".into()));
        }
    }
    out
}

/// The written files OpenCascade is asked to read (`tools/check_pmi_occt.py`): one with every
/// kind of item the writer makes, on a specify-core part; a written thread replaced and removed
/// (U14); an add beside specify-core's notes on faces (U10); and specify-core's intents written
/// onto their original corpus files. `None` for a case whose input is missing (the corpus).
fn written_cases() -> Vec<(String, Option<Vec<u8>>)> {
    let mut out = Vec::new();
    // Every kind of item, replacing a specify-core part's PMI.
    let f = spool();
    let mut p = four_datums();
    p.features.extend([
        faces(&[4]),
        faces(&[5]),
        faces(&[6]),
        faces(&[7]),
        faces(&[8]),
        faces(&[9, 10]),
    ]);
    p.dimensions = vec![
        size(4, "20.", DimTolerance::Deviations(bounds("0.1", "-0.05"))),
        size(
            5,
            "70.",
            DimTolerance::Fit {
                class: fit("g6"),
                limits: None,
            },
        ),
        size(
            6,
            "20.",
            DimTolerance::Fit {
                class: fit("H7"),
                limits: Some(bounds("20.021", "20.000")),
            },
        ),
        size(7, "12.", DimTolerance::Limits(bounds("12.1", "11.9"))),
        Dimension {
            kind: DimensionKind::Location {
                from: FeatureId(0),
                to: FeatureId(8),
                kind: LocationKind::LinearDistance,
                path: None,
                directed: false,
                angle: None,
            },
            nominal: Some(mm("30.")),
            tolerance: DimTolerance::Basic,
            qualifier: None,
            modifiers: Vec::new(),
            principle: None,
        },
    ];
    let zone = |form: ZoneForm| Zone {
        form,
        projected: None,
        non_uniform: None,
        runout_angle: None,
        affected_plane: None,
    };
    let mut pos = tolerance(
        ToleranceKind::Position,
        ToleranceTarget::Dimension(DimensionId(0)),
        "0.2",
    );
    pos.datums = Some(system(&[0, 1, 2]));
    pos.modifiers = vec![ToleranceModifier::MaximumMaterialRequirement];
    pos.zone = Some(zone(ZoneForm::CylindricalOrCircular));
    let mut perp = tolerance(
        ToleranceKind::Perpendicularity,
        ToleranceTarget::Feature(FeatureId(1)),
        "0.05",
    );
    perp.datums = Some(system(&[0]));
    let mut par = tolerance(
        ToleranceKind::Parallelism,
        ToleranceTarget::Feature(FeatureId(3)),
        "0.04",
    );
    par.datums = Some(system(&[3, 1, 2]));
    let mut run = tolerance(
        ToleranceKind::CircularRunout,
        ToleranceTarget::Feature(FeatureId(5)),
        "0.02",
    );
    run.datums = Some(system(&[0, 1]));
    let mut total = tolerance(
        ToleranceKind::TotalRunout,
        ToleranceTarget::Feature(FeatureId(6)),
        "0.03",
    );
    total.datums = Some(system(&[0]));
    let mut prof = tolerance(
        ToleranceKind::SurfaceProfile,
        ToleranceTarget::Feature(FeatureId(9)),
        "0.1",
    );
    prof.datums = Some(system(&[0, 1]));
    p.tolerances = vec![
        tolerance(
            ToleranceKind::Flatness,
            ToleranceTarget::Feature(FeatureId(0)),
            "0.02",
        ),
        pos,
        perp,
        par,
        run,
        total,
        prof,
    ];
    p.general = vec![GeneralTolerance::Class {
        text: "ISO 2768-mK".into(),
        standard: pmi::standards::Iso2768::recognise("ISO 2768-mK"),
    }];
    p.material = Some(Material {
        id: "Aluminium 6082-T6".into(),
        name: None,
        density: None,
    });
    p.attributes = vec![AttributeSet {
        name: "inspection".into(),
        on: Some(NoteOwner::Feature(FeatureId(4))),
        items: vec![("gauge".into(), AttributeValue::Text("plug".into()))],
    }];
    p.attributes.extend(part_notes());
    p.surface_textures = textures(Some(FeatureId(9)));
    let (edit, _) = pmi::write(
        &f.doc,
        &f.parts,
        &[(PartId(0), p)],
        Mode::Replace,
        PresentationPolicy::RemovePresentation,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    out.push((
        "every_kind".to_string(),
        Some(f.doc.apply(&edit).unwrap().bytes),
    ));

    // A thread haecceity wrote, written again (U14, `a_written_thread_is_replaced_and_removed`):
    // the internal thread replaced by the external thread and knurl, and those removed.
    let f = spool();
    let s = thread_sets(&f);
    for (name, first, mode, second) in [
        (
            "thread_replaced",
            &s.internal,
            Mode::Replace,
            &s.external_and_knurl,
        ),
        (
            "thread_removed",
            &s.external_and_knurl,
            Mode::Remove,
            &PartPmi::default(),
        ),
    ] {
        let g = open_bytes(written(&f, first).doc.bytes().to_vec());
        let w = write(&g, &[(PartId(0), second.clone())], mode)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_reads_back(&w, PartId(0), second);
        out.push((name.to_string(), Some(w.doc.bytes().to_vec())));
    }

    // specify-core's notes on faces (U10, `specify_core_notes_on_faces_are_anchored`): its
    // thumbwheel, whose external thread, internal thread and knurl notes the reader puts on
    // their faces, with a flatness added beside them.
    let dir = common::fixtures().join("ap242/specify");
    let f = open(&dir.join("thumbwheel_thread_knurl.step.gz"));
    let free = free_faces(&f, 0, 1);
    let flat = PartPmi {
        features: vec![faces(&[free[0]])],
        tolerances: vec![tolerance(
            ToleranceKind::Flatness,
            ToleranceTarget::Feature(FeatureId(0)),
            "0.05",
        )],
        ..PartPmi::default()
    };
    let w = write(&f, &[(PartId(0), flat)], Mode::Add).unwrap_or_else(|e| panic!("{e}"));
    let anchored = w.read.parts[0]
        .notes
        .iter()
        .filter(|n| n.on.is_some())
        .count();
    assert_eq!(anchored, 3, "{:#?}", w.read.parts[0].notes);
    out.push((
        "thumbwheel_notes_add".to_string(),
        Some(w.doc.bytes().to_vec()),
    ));

    // specify-core's intents onto the original files.
    for case in [
        "assembly_plate_pin",
        "spool_fits",
        "string_post_tapped",
        "thumbwheel_thread_knurl",
    ] {
        let meta: Json = serde_json::from_str(
            &std::fs::read_to_string(dir.join(format!("{case}.meta.json"))).unwrap(),
        )
        .unwrap();
        let Some(path) = original(meta["input"].as_str().unwrap()) else {
            out.push((format!("{case}_intent"), None));
            continue;
        };
        let intents: Json = serde_json::from_str(
            &std::fs::read_to_string(dir.join(format!("{case}.intent.json"))).unwrap(),
        )
        .unwrap();
        let intents: Vec<Json> = match intents {
            Json::Array(a) => a,
            one => vec![one],
        };
        let f = open(&path);
        let pmi_: Vec<(PartId, PartPmi)> = intents
            .iter()
            .map(|i| {
                (
                    PartId(i["binding"]["part"].as_u64().unwrap() as usize),
                    intent_pmi(i).0,
                )
            })
            .collect();
        let (edit, _) = pmi::write(
            &f.doc,
            &f.parts,
            &pmi_,
            Mode::Add,
            PresentationPolicy::RemovePresentation,
        )
        .unwrap_or_else(|e| panic!("{case}: {e}"));
        out.push((
            format!("{case}_intent"),
            Some(f.doc.apply(&edit).unwrap().bytes),
        ));
    }
    out
}

fn write_dir() -> PathBuf {
    common::fixtures().join("ap242/write")
}

/// Development aid: writes the written cases to `tests/fixtures/ap242/write/`
/// (`cargo test --release --test pmi_write export_written_files -- --ignored`), then
/// `tools/check_pmi_occt.py` captures OpenCascade's reading of them.
#[test]
#[ignore]
fn export_written_files() {
    std::fs::create_dir_all(write_dir()).unwrap();
    for (name, bytes) in written_cases() {
        let bytes = bytes.unwrap_or_else(|| panic!("{name}: input missing"));
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        std::io::Write::write_all(&mut gz, &bytes).unwrap();
        std::fs::write(
            write_dir().join(format!("{name}.step.gz")),
            gz.finish().unwrap(),
        )
        .unwrap();
    }
}

/// The committed written files are what the writer writes now (so OpenCascade's captures of
/// them are of the current writer's output); re-export and re-capture when this fails.
#[test]
fn written_files_are_the_writers_current_output() {
    for (name, bytes) in written_cases() {
        let Some(bytes) = bytes else { continue };
        let committed = file_bytes(&write_dir().join(format!("{name}.step.gz")));
        assert!(
            committed == bytes,
            "{name}: the writer's output changed; run export_written_files and tools/check_pmi_occt.py"
        );
    }
}

/// Oracle: OpenCascade XCAF's reading of each written file against haecceity's (equal to
/// what was written, checked above); every difference pinned with a verdict
/// (`known_pmi_write.json` "occt"); and every datum's symbol read as its presentation.
#[test]
fn opencascade_reads_the_written_files() {
    let pins = load_pins();
    let pinned = pins["occt"].as_array().unwrap();
    common::check_verdicts("known_pmi_write.json occt", pinned);
    let mut problems = Vec::new();
    let mut actual = Vec::new();
    let mut names: Vec<String> = std::fs::read_dir(write_dir())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .filter_map(|n| n.strip_suffix(".occt.json.gz").map(str::to_string))
        .collect();
    names.sort();
    assert!(
        !names.is_empty(),
        "no OpenCascade captures of written files"
    );
    for name in names {
        let capture: Json = serde_json::from_slice(&file_bytes(
            &write_dir().join(format!("{name}.occt.json.gz")),
        ))
        .unwrap();
        let path = write_dir().join(format!("{name}.step.gz"));
        assert_eq!(
            capture["sha256"].as_str(),
            Some(common::sha256_hex(&std::fs::read(&path).unwrap()).as_str()),
            "{name}: the capture is not of the committed file"
        );
        // specify-core's load.load_all opens it (it crashed on the datum feature symbols'
        // mechanical_design_and_draughting_relationship, no longer written), and XCAF reads it
        // and every shape label's name.
        if capture["loads"] != true || capture["reads"] != true {
            problems.push(format!(
                "{name}: OpenCascade does not open it: loads {} reads {}",
                capture["loads"], capture["reads"]
            ));
            continue;
        }
        // Every datum in these files is one the writer added, so each has its datum feature
        // symbol (decision 6), which OpenCascade links to the datum as its presentation
        // (thumbwheel_notes_add's are specify-core's, with specify-core's symbols).
        let datums = capture["parts"]
            .as_array()
            .unwrap()
            .iter()
            .chain([&capture["unattached"]])
            .flat_map(|p| p["datums"].as_array().unwrap());
        for d in datums {
            let pres = &d["presentation"];
            let want = format!("Datum {}", d["name"].as_str().unwrap_or(""));
            if pres["name"].as_str() != Some(want.as_str())
                || pres["edges"].as_u64().unwrap_or(0) == 0
                || pres["plane"] != true
            {
                problems.push(format!(
                    "{name}: OpenCascade reads datum {} without its symbol: {pres}",
                    d["name"]
                ));
            }
        }
        let f = open(&path);
        let r = pmi::read(&f.doc, &f.parts).unwrap();
        for (key, detail) in compare_occt(&name, &capture, &r, &f.doc, &f.parts) {
            match pinned.iter().find(|e| e["key"] == key.as_str()) {
                Some(e) if e["detail"] == detail.as_str() => {}
                Some(e) => problems.push(format!(
                    "{key}: changed: pinned {} actual {detail}",
                    e["detail"]
                )),
                None => problems.push(format!("{key}: not pinned: {detail}")),
            }
            actual.push(serde_json::json!({"key": key, "detail": detail}));
        }
    }
    for e in pinned {
        if !actual.iter().any(|a| a["key"] == e["key"]) {
            problems.push(format!("{}: pinned but not found", e["key"]));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
