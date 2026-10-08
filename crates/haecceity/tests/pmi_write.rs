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

fn assert_reads_back(w: &Written, part: PartId, p: &PartPmi) {
    let d = differences(p, &w.read.parts[part.0]);
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
    assert!(differences(&p1, &w2.read.parts[1]).is_empty());
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
    files.extend(nist_files());
    if files
        .iter()
        .all(|p| !p.to_string_lossy().contains("nist_stc"))
    {
        files.extend(
            std::fs::read_dir(common::fixtures().join("ap242/nist"))
                .unwrap()
                .map(|e| e.unwrap().path())
                .filter(|p| p.to_string_lossy().ends_with(".stp.gz")),
        );
    }
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
            let d: Vec<String> = differences(p, &w.read.parts[i])
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
        let d = differences(&expect, &w.read.parts[0]);
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

/// NIST's MBE PMI test models: `HAECCEITY_NIST_PMI`, else the AP242 track's download.
fn nist_files() -> Vec<PathBuf> {
    let dir = std::env::var("HAECCEITY_NIST_PMI")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(
                "/private/tmp/claude-501/-Users-paul-repos-quiddity-rust/\
                 4f6aac0c-d5a0-4c82-bc67-2d2552947882/scratchpad/ap242-nist/x/NIST-PMI-STEP-Files",
            )
        });
    if !dir.is_dir() {
        assert!(
            std::env::var_os("HAECCEITY_NIST_PMI_REQUIRED").is_none(),
            "NIST PMI files required but {} is missing",
            dir.display()
        );
        return Vec::new();
    }
    let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "stp"))
        .collect();
    out.sort();
    out
}
