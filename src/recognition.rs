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
//! (Python's `caller_index`). The input's SHA256 identifies the exact source file bytes when
//! recognised from a file; in-memory recognition has no source digest.
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
pub use crate::features::evidence::{EvidenceStatus, RunMode};
use crate::features::gussets::GussetRibPattern;
use crate::features::hole_patterns::HolePattern;
use crate::features::oriented_slots::OrientedSlotPattern;
use crate::features::recess_patterns::{PocketPattern, SlotPattern};
use crate::features::section_recess::SectionRecessPattern;
use crate::frames::RecordFrame;

/// Latest contract: version 4 adds measured modelled threads and explicit refusals.
/// Documents using only version 3 record types retain the legacy tag and record payloads.
pub const SCHEMA_VERSION: &str = "quiddity-rust/recognition/4";
/// Documents without native measured-thread or bounded-air refusal records retain version 3.
pub const LEGACY_SCHEMA_VERSION: &str = "quiddity-rust/recognition/3";

/// The minimum document contract required by these records.
pub fn schema_version(features: &Features) -> &'static str {
    if features.modelled_threads.is_empty()
        && features.modelled_thread_refusals.is_empty()
        && features.interior_void_refusals.is_empty()
    {
        LEGACY_SCHEMA_VERSION
    } else {
        SCHEMA_VERSION
    }
}

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
    /// Lowercase SHA256 of input file bytes (compressed bytes for .gz), or null in memory.
    #[serde(default)]
    pub source_sha256: Option<String>,
    /// Strict inventory or its local-degradation retry; unknown only in older documents.
    #[serde(default)]
    pub run_mode: RunMode,
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
    /// Strict valid-solid proof on defining and consulted faces, or on every pattern member.
    /// Refusals and records from older documents are unproved.
    #[serde(default)]
    pub evidence_status: EvidenceStatus,
}

impl Record {
    /// A published explanation of a refused projection, rather than a feature claim.
    /// Consumers should inspect its type and reason separately from feature evidence status.
    pub fn is_refusal(&self) -> bool {
        matches!(
            self.record_type.as_str(),
            "SectionRecessRefusal" | "ModelledThreadRefusal" | "InteriorVoidRefusal"
        )
    }
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
    ("interior_voids", &["InteriorVoid", "InteriorVoidRefusal"]),
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
    ("modelled_threads", &["ModelledThread"]),
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
    (
        "section_recesses",
        &["SectionRecess", "SectionRecessRefusal"],
    ),
    (
        "section_recess_patterns",
        &["SectionRecessArray", "SectionRecessGrid"],
    ),
    ("modelled_thread_refusals", &["ModelledThreadRefusal"]),
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
        "section_recess_patterns" => match &features.section_recess_patterns[n] {
            SectionRecessPattern::Array(_) => "SectionRecessArray",
            SectionRecessPattern::Grid(_) => "SectionRecessGrid",
        },
        "interior_voids" => {
            if n < features.interior_voids.len() {
                "InteriorVoid"
            } else {
                "InteriorVoidRefusal"
            }
        }
        "section_recesses" => {
            if n < features.section_recesses.len() {
                "SectionRecess"
            } else {
                "SectionRecessRefusal"
            }
        }
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

/// Each family's document records as JSON, in [`Features`]' field order, with section-recess
/// source refusals appended after occurrences (a `serde_json::Value` object
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
    let mut families = serde_json::from_str::<Ordered>(&text)
        .expect("every family is a list of records")
        .0;
    families
        .iter_mut()
        .find(|(f, _)| f == "interior_voids")
        .unwrap()
        .1
        .extend(
            features
                .interior_void_refusals
                .iter()
                .map(|r| serde_json::to_value(r).expect("void refusal serialises")),
        );
    let (_, recesses) = families
        .iter_mut()
        .find(|(f, _)| f == "section_recesses")
        .unwrap();
    recesses.extend(
        features
            .section_recess_refusals
            .iter()
            .map(|r| serde_json::to_value(r).expect("refusal serialises")),
    );
    families.push((
        "section_recess_patterns".into(),
        features
            .section_recess_patterns
            .iter()
            .map(|r| serde_json::to_value(r).expect("pattern serialises"))
            .collect(),
    ));
    families.push((
        "modelled_thread_refusals".into(),
        features
            .modelled_thread_refusals
            .iter()
            .map(|r| serde_json::to_value(r).expect("thread refusal serialises"))
            .collect(),
    ));
    families
}

/// The document of *recognition*, a part of *face_count* faces: each record with its
/// fingerprint's faces (or source evidence's defining faces for a refusal).
pub fn document(recognition: &Recognition, face_count: usize) -> RecognitionDocument {
    let faces: BTreeMap<&str, &Vec<usize>> = recognition
        .fingerprints
        .features
        .iter()
        .map(|f| (f.id.as_str(), &f.faces))
        .collect();
    let mut records = Vec::new();
    let families = families(&recognition.features);
    for (family, list) in &families {
        for (n, record) in list.iter().enumerate() {
            let id = format!("{family}/{n}");
            let Value::Object(parameters) = record else {
                panic!("{id} is not a record");
            };
            // Refusals describe source evidence, not recognised geometry to fingerprint.
            let faces =
                if family == "interior_voids" && n >= recognition.features.interior_voids.len() {
                    recognition.features.interior_void_refusals
                        [n - recognition.features.interior_voids.len()]
                    .candidate
                    .void_faces
                    .clone()
                } else if family == "modelled_thread_refusals" {
                    recognition.features.modelled_thread_refusals[n]
                        .faces
                        .clone()
                } else if family == "section_recess_patterns" {
                    let mut faces: Vec<_> = recognition.features.section_recess_patterns[n]
                        .members()
                        .iter()
                        .flat_map(|&m| {
                            recognition.features.section_recesses[m]
                                .evidence()
                                .defining_faces()
                                .iter()
                                .copied()
                        })
                        .collect();
                    faces.sort_unstable();
                    faces.dedup();
                    faces
                } else if family == "section_recesses"
                    && n >= recognition.features.section_recesses.len()
                {
                    recognition.features.section_recess_refusals
                        [n - recognition.features.section_recesses.len()]
                    .evidence()
                    .defining_faces()
                    .to_vec()
                } else {
                    faces
                        .get(id.as_str())
                        .unwrap_or_else(|| panic!("{id} has no fingerprint"))
                        .to_vec()
                };
            records.push(Record {
                record_type: record_type(&recognition.features, family, n).into(),
                faces,
                local: recognition.local.get(&id).cloned().unwrap_or_default(),
                id,
                family: family.clone(),
                evidence_status: record_status(
                    &recognition.features,
                    family,
                    n,
                    parameters,
                    &families,
                ),
                parameters: parameters.clone(),
            });
        }
    }
    RecognitionDocument {
        schema_version: schema_version(&recognition.features).into(),
        package: Package {
            name: "quiddity-rust".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        },
        coordinate_space: "file".into(),
        frame: recognition.frame.clone(),
        face_count,
        records,
        source_sha256: recognition.source_sha256.clone(),
        run_mode: recognition.features.run_mode,
    }
}

/// Derived records inherit every member's proof, including consulted faces absent from the
/// fingerprint. Embedded members are resolved in the same order as fingerprint::derived_faces.
fn record_status(
    features: &Features,
    family: &str,
    n: usize,
    parameters: &Map<String, Value>,
    families: &[(String, Vec<Value>)],
) -> EvidenceStatus {
    let status = |family: &str, n: usize| {
        features
            .evidence_status
            .get(family)
            .and_then(|s| s.get(n))
            .copied()
            .unwrap_or_default()
    };
    if family == "section_recess_patterns" {
        let members = features.section_recess_patterns[n].members();
        return if !members.is_empty()
            && members
                .iter()
                .all(|&m| status("section_recesses", m) == EvidenceStatus::Verified)
        {
            EvidenceStatus::Verified
        } else {
            EvidenceStatus::Unproved
        };
    }
    if let Some((field, base)) =
        crate::correspondence::fingerprint::family(family).and_then(|f| f.members)
    {
        let Some(members) = parameters.get(field).and_then(Value::as_array) else {
            return EvidenceStatus::Unproved;
        };
        let Some((_, pool)) = families.iter().find(|(f, _)| f == base) else {
            return EvidenceStatus::Unproved;
        };
        let mut taken = vec![false; pool.len()];
        return if !members.is_empty()
            && members.iter().all(|member| {
                let Some(k) = (0..pool.len()).find(|&k| !taken[k] && pool[k] == *member) else {
                    return false;
                };
                taken[k] = true;
                status(base, k) == EvidenceStatus::Verified
            }) {
            EvidenceStatus::Verified
        } else {
            EvidenceStatus::Unproved
        };
    }
    status(family, n)
}
