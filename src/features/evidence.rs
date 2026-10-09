//! Records with the faces that prove them, and the check that publishes them only when every
//! defining and consulted face belongs to one valid solid (`common_valid_solid`).

use crate::kernel::brep::Part;

/// A recognised record, the faces that define it, and the faces consulted to prove it.
#[derive(Clone, Debug, PartialEq)]
pub struct Occurrence<R> {
    pub record: R,
    pub defining: Vec<usize>,
    pub context: Vec<usize>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum EvidenceError {
    /// A defining face has no single valid solid shared with the rest of its evidence.
    NoValidSolid,
    /// Two occurrences claim one defining face, or one face both defines and is consulted.
    SharedEvidence,
    /// One turned profile key would identify two valid solids.
    AmbiguousProfile,
    /// Two obround cap clusters compete for one end of a slot or pocket, so which faces end it
    /// is undecided.
    CompetingCaps,
    /// A pocket found from opposed walls or obround caps has no floor faces to consult.
    MissingFloor,
}

impl std::fmt::Display for EvidenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvidenceError::NoValidSolid => {
                write!(f, "defining face has no unambiguous valid solid")
            }
            EvidenceError::SharedEvidence => {
                write!(f, "evidence faces are shared between occurrences")
            }
            EvidenceError::AmbiguousProfile => {
                write!(f, "turned profile key identifies multiple valid solids")
            }
            EvidenceError::CompetingCaps => {
                write!(f, "multiple obround cap clusters compete for one endpoint")
            }
            EvidenceError::MissingFloor => write!(f, "pocket floor faces are unavailable"),
        }
    }
}

impl std::error::Error for EvidenceError {}

/// The one valid solid every face belongs to (`FaceGraph.common_valid_solid`). Valid as
/// Python's `solid.is_valid` (`BRepCheck_Analyzer`) reads it: sound topology and no faulted face
/// (`Part::solid_is_geometrically_valid`).
pub fn common_valid_solid(part: &Part, faces: &[usize]) -> Option<usize> {
    let owner = part.faces[*faces.first()?].solid?;
    (part.solid_is_geometrically_valid(owner)
        && faces.iter().all(|&f| part.faces[f].solid == Some(owner)))
    .then_some(owner)
}

/// Most faulted faces a locally degraded run admits a solid with.
const MAX_BAD_FACES: usize = 3;

/// [`common_valid_solid`] as Python's locally degraded run proves it (`local_degradation`): a
/// solid with faulted faces is also admitted when its topology is sound, it has one shell and a
/// positive volume and at most three faulted faces, but then no face may be a faulted one or
/// share an edge with one.
pub fn locally_valid_solid(part: &Part, faces: &[usize]) -> Option<usize> {
    let owner = part.faces[*faces.first()?].solid?;
    if !faces.iter().all(|&f| part.faces[f].solid == Some(owner)) || !part.solid_is_valid(owner) {
        return None;
    }
    let bad = part.bad_faces(owner);
    if bad.is_empty() {
        return Some(owner);
    }
    let admitted = bad.len() <= MAX_BAD_FACES
        && part.shell_count(owner) == 1
        && part
            .solid_mass(owner)
            .is_some_and(|(volume, _)| volume > 0.0);
    let unsafe_face =
        |f: usize| bad.contains(&f) || part.neighbours(f).iter().any(|n| bad.contains(n));
    (admitted && !faces.iter().any(|&f| unsafe_face(f))).then_some(owner)
}

/// All occurrences, or none: each one's faces must share one valid solid.
pub fn verified<R>(
    part: &Part,
    found: Vec<Occurrence<R>>,
) -> Result<Vec<Occurrence<R>>, EvidenceError> {
    for o in &found {
        let faces: Vec<usize> = o.defining.iter().chain(&o.context).copied().collect();
        if o.defining.is_empty() || common_valid_solid(part, &faces).is_none() {
            return Err(EvidenceError::NoValidSolid);
        }
    }
    Ok(found)
}
