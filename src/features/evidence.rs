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
        }
    }
}

impl std::error::Error for EvidenceError {}

/// All occurrences, or none: each one's faces must share one valid solid.
pub fn verified<R>(
    part: &Part,
    found: Vec<Occurrence<R>>,
) -> Result<Vec<Occurrence<R>>, EvidenceError> {
    for o in &found {
        let owner = o.defining.first().and_then(|&f| part.faces[f].solid);
        let valid = owner.is_some_and(|s| part.solid_is_valid(s))
            && o.defining
                .iter()
                .chain(&o.context)
                .all(|&f| part.faces[f].solid == owner);
        if !valid {
            return Err(EvidenceError::NoValidSolid);
        }
    }
    Ok(found)
}
