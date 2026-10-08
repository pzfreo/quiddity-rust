//! Semantic PMI: the model ([`model`]), ISO general tolerance tables ([`standards`]) and the
//! AP242 reader ([`read()`]). The design is `docs/step-ap242.md`.
//!
//! [`read()`] maps a Part 21 document's semantic PMI to one [`PartPmi`] per distinct part of
//! the file, with [`Finding`]s for everything it did not read (or read through a
//! nonconformance) and the [`Provenance`] of every model item: the instance ids it consumed,
//! which a replace removes.

pub mod model;
mod read;
pub mod standards;

pub use model::*;
pub use read::read;

/// The result of reading a file's PMI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PmiRead {
    /// One per part of `read_part_definitions`, in its order (`PartId` is the index).
    pub parts: Vec<PartPmi>,
    /// What was not read, or was read through a nonconformance, in a deterministic order.
    pub findings: Vec<Finding>,
    pub provenance: Provenance,
    pub accounting: Accounting,
}

/// Why a file's PMI could not be read at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadError {
    /// The part definitions do not belong to this document (an id is not an instance of it).
    Parts(String),
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadError::Parts(s) => write!(f, "part definitions do not match the document: {s}"),
        }
    }
}

impl std::error::Error for ReadError {}

/// Something the reader did not read, or read only through a stated interpretation.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Finding {
    pub kind: FindingKind,
    /// The part it concerns, when known.
    pub part: Option<PartId>,
    /// The subject instance first, then the others concerned.
    pub ids: Vec<u64>,
    /// The subject's entity type, lower case (a complex instance's leaves joined by `+`).
    pub entity: String,
    pub detail: String,
}

/// The kinds of finding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FindingKind {
    /// A construct the reader does not read (a form the practice does not define, or one the
    /// model does not hold yet); not consumed.
    Unsupported,
    /// A construct that violates the schema or the practice, read through as the detail says
    /// (consumed), or not read when it cannot be (not consumed).
    Nonconformance,
    /// A reference that does not resolve: a face or edge that is not the part's, an instance
    /// missing or of the wrong type; the item is not read.
    Unresolved,
    /// A measure whose unit cannot be resolved; its value is not used.
    Unitless,
    /// PMI on an assembly's product definition: out of scope, not attributed to any part.
    AssemblyPmi,
    /// PMI on an occurrence (`assembly_component_usage`, `specified_higher_usage_occurrence`
    /// paths): out of scope, not attributed to any part.
    OccurrencePmi,
    /// A construct whose practice was not available (material practice not obtained).
    UndeterminedPractice,
    /// Information the model does not hold, consumed with the item it belongs to (e.g.
    /// explicit geometry of a derived feature, CAD-system-specific aspects); the detail says
    /// what is dropped.
    NotModelled,
    /// Two statements of one thing that disagree; neither is guessed between.
    Conflict,
    /// A semantic-PMI instance reached by no part's PMI.
    Unconsumed,
}

impl FindingKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            FindingKind::Unsupported => "unsupported",
            FindingKind::Nonconformance => "nonconformance",
            FindingKind::Unresolved => "unresolved-reference",
            FindingKind::Unitless => "unitless-measure",
            FindingKind::AssemblyPmi => "assembly-pmi",
            FindingKind::OccurrencePmi => "occurrence-pmi",
            FindingKind::UndeterminedPractice => "undetermined-practice",
            FindingKind::NotModelled => "not-modelled",
            FindingKind::Conflict => "conflict",
            FindingKind::Unconsumed => "unconsumed",
        }
    }
}

/// Per part, per model item, the instance ids the reader consumed for it (ascending). Shared
/// instances (a datum system two tolerances use, a qualifier several values use) are listed
/// under each item that consumed them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Provenance {
    pub parts: Vec<PartProvenance>,
}

/// The ids behind each item of one [`PartPmi`], index for index.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PartProvenance {
    pub standards: Vec<Vec<u64>>,
    pub decimal_places: Vec<u64>,
    pub features: Vec<Vec<u64>>,
    pub datum_targets: Vec<Vec<u64>>,
    pub datums: Vec<Vec<u64>>,
    pub dimensions: Vec<Vec<u64>>,
    pub tolerances: Vec<Vec<u64>>,
    pub tolerance_relations: Vec<Vec<u64>>,
    pub general: Vec<Vec<u64>>,
    pub threads: Vec<Vec<u64>>,
    pub knurls: Vec<Vec<u64>>,
    pub material: Vec<u64>,
    pub notes: Vec<Vec<u64>>,
    pub attributes: Vec<Vec<u64>>,
    /// Per supplemental geometry: every instance of its item tree.
    pub geometry: Vec<Vec<u64>>,
    /// Per supplemental geometry: its item's own instance.
    pub geometry_items: Vec<u64>,
}

impl PartProvenance {
    /// Every id consumed for the part, ascending, once.
    #[must_use]
    pub fn all(&self) -> Vec<u64> {
        let mut out: Vec<u64> = self
            .decimal_places
            .iter()
            .chain(&self.material)
            .copied()
            .chain(
                [
                    &self.standards,
                    &self.features,
                    &self.datum_targets,
                    &self.datums,
                    &self.dimensions,
                    &self.tolerances,
                    &self.tolerance_relations,
                    &self.general,
                    &self.threads,
                    &self.knurls,
                    &self.notes,
                    &self.attributes,
                    &self.geometry,
                ]
                .into_iter()
                .flatten()
                .flatten()
                .copied(),
            )
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }
}

/// How the file's instances were accounted for: every semantic-PMI instance is consumed by a
/// part or named by a finding.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Accounting {
    /// Instances of the semantic-PMI family (`express::Family::SemanticPmi`).
    pub semantic: usize,
    /// Of them, consumed by some part.
    pub consumed: usize,
    /// Of them, not consumed but named by a finding.
    pub reported: usize,
    /// Presentation instances (counted, not interpreted).
    pub presentation: usize,
    /// Validation property definitions (counted, not interpreted).
    pub validation: usize,
}
