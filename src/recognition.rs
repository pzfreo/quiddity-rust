//! The versioned recognition document (review finding M5): one record per recognised feature,
//! with its family, Python's record class name, its defining faces and its record, for
//! draftwright to read in place of Python quiddity's document.
//!
//! It is shaped after Python's `build_recognition_document` features (`family`, `record_type`,
//! `record`, `defining_faces`), flattened to draftwright's bridge `Record` (`faces`,
//! `parameters`). Differences from Python's: hole patterns are records here (Python keeps them
//! under `derived`), with their members' faces; records carry no `dependents` or
//! `named_dimensions` (Python adds them when enriching its document); coordinates are the STEP
//! file's (Python's are in a working frame it reports); faces are the file's own indices
//! (Python's `caller_index`). The input's sha256 is not carried: hashing needs a new dependency,
//! a maintainer question (docs/review-2026-10-08.md); the face count is.
//!
//! Version 2 (the maintainer's decision on review M8): the part is recognised in its own frame
//! and the records reported in the file's coordinates ([`crate::framed_records`]). `frame` says
//! where the records were found: the inferred frame (origin and axes in the file's coordinates,
//! and its gauge), the reason none could be inferred (the part was then recognised as placed in
//! the file, so its records depend on that placement), or `caller-space` for a part recognised in
//! memory without one. Each record's `local` lists the parameters, by path (`a.b` a nested
//! field, `a[]` each item of a list), that no file axis corresponds to and so are left in the
//! frame: an axis letter names the frame's axis, and a coordinate along it is measured from the
//! frame's origin. It is empty wherever the frame's axes are the file's axes up to order and
//! sign. Version 1 had neither field, and every axis letter named a file axis.

use std::collections::BTreeMap;

use serde::de::{Deserializer, MapAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::correspondence::Recognition;
use crate::features::Features;
use crate::features::gussets::GussetRibPattern;
use crate::features::hole_patterns::HolePattern;
use crate::features::oriented_slots::OrientedSlotPattern;
use crate::features::recess_patterns::{PocketPattern, SlotPattern};
use crate::frames::RecordFrame;

/// Bumped whenever the document's shape or a field's meaning changes; a reader refuses another.
pub const SCHEMA_VERSION: &str = "quiddity-rust/recognition/2";

/// One part's recognition, as draftwright reads it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecognitionDocument {
    pub schema_version: String,
    /// The recogniser: `quiddity-rust` and its crate version.
    pub package: Package,
    /// Coordinates in the records are the STEP file's (`file`), not a working frame's, except
    /// the fields each record lists in `local`.
    pub coordinate_space: String,
    /// Where the records were found: the part's own frame, or why none could be inferred.
    pub frame: RecordFrame,
    /// The part's face count: faces are indexed `0..face_count` in the file's order, as in
    /// the fingerprints.
    pub face_count: usize,
    /// In `Features`' family order, then each family's own.
    pub records: Vec<Record>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    pub version: String,
}

/// A recognised feature.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Record {
    /// `<family>/<index>`, the feature's fingerprint ID.
    pub id: String,
    /// The family's field in [`Features`], its serde key.
    pub family: String,
    /// Python's record class name ([`RECORD_TYPES`]).
    pub record_type: String,
    /// The faces that define it, ascending (a pattern's are its members').
    pub faces: Vec<usize>,
    /// The record as the family serialises it.
    pub parameters: Map<String, Value>,
    /// The parameters left in the document's `frame` (no file axis corresponds to them), by
    /// path; empty when every parameter is in the file's coordinates.
    pub local: Vec<String>,
}

/// Python's record class names for each family's records, in [`Features`]' order: one, or one
/// per variant of an untagged family ([`record_type`] picks it).
pub const RECORD_TYPES: &[(&str, &[&str])] = &[
    ("fillets", &["Fillet"]),
    ("chamfers", &["Chamfer"]),
    ("bosses", &["BossRecord"]),
    ("angled_steps", &["AngledStep"]),
    ("flats", &["Flat"]),
    ("paired_ramp_steps", &["PairedRampStep"]),
    ("oriented_chamfers", &["OrientedChamfer"]),
    ("circular_face_patterns", &["CircularFacePattern"]),
    ("oblique_through_steps", &["ObliqueThroughStep"]),
    ("circular_blind_steps", &["CircularBlindStep"]),
    ("holes", &["HoleRecord"]),
    ("countersinks", &["CounterSink"]),
    (
        "hole_patterns",
        &[
            "RectGrid",
            "RectangularHoleSet",
            "BoltCircle",
            "LinearArray",
        ],
    ),
    ("gusset_ribs", &["GussetRib"]),
    (
        "gusset_rib_patterns",
        &["GussetRibArray", "GussetRibMirrorPair"],
    ),
    ("thin_wall_bodies", &["ThinWallBody"]),
    ("interior_voids", &["InteriorVoid"]),
    ("through_steps", &["ThroughStep"]),
    ("turned_steps", &["TurnedStep"]),
    ("grooves", &["Groove"]),
    ("plates", &["Plate"]),
    ("round_bottom_blind_slots", &["RoundBottomBlindSlot"]),
    ("rectangular_blind_slots", &["RectangularBlindSlot"]),
    ("double_d_bores", &["DoubleDBore"]),
    ("edge_open_circular_pockets", &["EdgeOpenCircularPocket"]),
    ("edge_open_prismatic_recesses", &["EdgeOpenPrismaticRecess"]),
    ("blends", &["Blend"]),
    ("sheet_metal_bodies", &["SheetMetalBody"]),
    ("slots", &["Slot"]),
    ("pockets", &["Pocket"]),
    ("channels", &["Channel"]),
    ("slot_patterns", &["SlotGrid", "SlotArray"]),
    ("pocket_patterns", &["PocketGrid", "PocketArray"]),
    ("repeating_radial_profiles", &["RepeatingRadialProfile"]),
    ("freeform_surfaces", &["FreeformSurface"]),
    ("polygonal_bosses", &["PolygonalBoss"]),
    ("polygonal_stock", &["PolygonalStock"]),
    ("section_passages", &["SectionPassage"]),
    ("prismatic_pockets", &["PrismaticPocket"]),
    ("oriented_slots", &["OrientedSlot"]),
    (
        "oriented_slot_patterns",
        &["OrientedSlotGrid", "OrientedSlotArray"],
    ),
    ("pads", &["RaisedPad"]),
    ("step_levels", &["FaceLevel"]),
    ("risers", &["RiserEvidence"]),
];

/// The record class names a family's records can have.
pub fn record_types(family: &str) -> Option<&'static [&'static str]> {
    RECORD_TYPES
        .iter()
        .find(|(f, _)| *f == family)
        .map(|(_, types)| *types)
}

/// Python's record class name for record *n* of *family*.
pub fn record_type(features: &Features, family: &str, n: usize) -> &'static str {
    match family {
        "hole_patterns" => match &features.hole_patterns[n] {
            HolePattern::RectGrid { .. } => "RectGrid",
            HolePattern::RectangularHoleSet { .. } => "RectangularHoleSet",
            HolePattern::BoltCircle { .. } => "BoltCircle",
            HolePattern::LinearArray { .. } => "LinearArray",
        },
        "gusset_rib_patterns" => match &features.gusset_rib_patterns[n] {
            GussetRibPattern::Array { .. } => "GussetRibArray",
            GussetRibPattern::MirrorPair { .. } => "GussetRibMirrorPair",
        },
        "slot_patterns" => match &features.slot_patterns[n] {
            SlotPattern::SlotGrid { .. } => "SlotGrid",
            SlotPattern::SlotArray { .. } => "SlotArray",
        },
        "pocket_patterns" => match &features.pocket_patterns[n] {
            PocketPattern::PocketGrid { .. } => "PocketGrid",
            PocketPattern::PocketArray { .. } => "PocketArray",
        },
        "oriented_slot_patterns" => match &features.oriented_slot_patterns[n] {
            OrientedSlotPattern::OrientedSlotGrid { .. } => "OrientedSlotGrid",
            OrientedSlotPattern::OrientedSlotArray { .. } => "OrientedSlotArray",
        },
        _ => match record_types(family) {
            Some([one]) => one,
            _ => panic!(
                "family {family} has no single record type: add (\"{family}\", \
                 &[\"<Python record class>\"]) to RECORD_TYPES in src/recognition.rs"
            ),
        },
    }
}

/// Each family's records as JSON, in [`Features`]' field order (a `serde_json::Value` object
/// would sort the families by name).
pub fn families(features: &Features) -> Vec<(String, Vec<Value>)> {
    struct Ordered(Vec<(String, Vec<Value>)>);
    impl<'de> Deserialize<'de> for Ordered {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            struct V;
            impl<'de> Visitor<'de> for V {
                type Value = Ordered;
                fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                    f.write_str("families of records")
                }
                fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Ordered, A::Error> {
                    let mut out = Vec::new();
                    while let Some(entry) = map.next_entry()? {
                        out.push(entry);
                    }
                    Ok(Ordered(out))
                }
            }
            d.deserialize_map(V)
        }
    }
    let text = serde_json::to_string(features).expect("records serialise");
    serde_json::from_str::<Ordered>(&text)
        .expect("every family is a list of records")
        .0
}

/// The document of *recognition*, a part of *face_count* faces: each record with its
/// fingerprint's faces.
pub fn document(recognition: &Recognition, face_count: usize) -> RecognitionDocument {
    let faces: BTreeMap<&str, &Vec<usize>> = recognition
        .fingerprints
        .features
        .iter()
        .map(|f| (f.id.as_str(), &f.faces))
        .collect();
    let mut records = Vec::new();
    for (family, list) in families(&recognition.features) {
        for (n, record) in list.into_iter().enumerate() {
            let id = format!("{family}/{n}");
            let Value::Object(parameters) = record else {
                panic!("{id} is not a record");
            };
            let faces = faces
                .get(id.as_str())
                .unwrap_or_else(|| panic!("{id} has no fingerprint"));
            records.push(Record {
                record_type: record_type(&recognition.features, &family, n).into(),
                faces: faces.to_vec(),
                local: recognition.local.get(&id).cloned().unwrap_or_default(),
                id,
                family: family.clone(),
                parameters,
            });
        }
    }
    RecognitionDocument {
        schema_version: SCHEMA_VERSION.into(),
        package: Package {
            name: "quiddity-rust".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        },
        coordinate_space: "file".into(),
        frame: recognition.frame.clone(),
        face_count,
        records,
    }
}
