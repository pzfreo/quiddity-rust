//! Occurrences issued from geometry-facade faces (`quiddity._geometry_evidence.
//! GeometryEvidenceBridge`): faces resolved against the facade's own faces, in its order, and
//! published only when they keep one valid solid's proof.
//!
//! Python writes through an aggregate evidence writer; here the bridge builds the
//! [`Occurrence`] the family returns, its defining faces and the further constituent faces the
//! claim covers. Python's `local_degradation` (the aggregate's retry that skips, rather than
//! refuses, a candidate whose faces prove no solid) is not ported here: three corpus parts reach
//! that retry (`tests/fixtures/captured/local_degradation/capture.json`), but none of its skips
//! is a polygonal boss, the one family issuing through this bridge, so no capture shows the
//! bridge's degraded branch. Families whose skips the capture does show carry their own degraded
//! path (holes and pockets, `discover_locally_degraded`).

use super::evidence::{EvidenceError, Occurrence, common_valid_solid};
use super::experimental_geometry::GeometryGraph;

/// `GeometryEvidenceBridge`.
pub struct GeometryEvidence<'g, 'a> {
    pub geometry: &'g GeometryGraph<'a>,
}

impl<'g, 'a> GeometryEvidence<'g, 'a> {
    pub fn new(geometry: &'g GeometryGraph<'a>) -> Self {
        GeometryEvidence { geometry }
    }

    /// `refs`: the distinct faces among *faces*, in the graph's order. A face outside the graph
    /// is the caller's mistake and panics (Python raises).
    pub fn refs(&self, faces: &[usize]) -> Vec<usize> {
        for &f in faces {
            assert!(
                self.geometry.contains(f),
                "face {f} is foreign to this geometry graph"
            );
        }
        self.geometry
            .faces()
            .iter()
            .copied()
            .filter(|f| faces.contains(f))
            .collect()
    }

    /// `proves_solid`: the faces all belong to one valid solid.
    pub fn proves_solid(&self, refs: &[usize]) -> bool {
        common_valid_solid(self.geometry.part, refs).is_some()
    }

    /// `validate_defining`: refuse a publication whose faces do not belong to one valid solid.
    pub fn validate_defining(&self, refs: &[usize]) -> Result<(), EvidenceError> {
        if self.proves_solid(refs) {
            Ok(())
        } else {
            Err(EvidenceError::NoValidSolid)
        }
    }

    /// `add_defining`: the occurrence of *record* defined by *refs*, its claim covering also the
    /// *constituent* faces beyond them (held as the occurrence's consulted faces).
    pub fn occurrence<R>(&self, record: R, refs: &[usize], constituent: &[usize]) -> Occurrence<R> {
        Occurrence {
            record,
            defining: refs.to_vec(),
            context: constituent
                .iter()
                .copied()
                .filter(|f| !refs.contains(f))
                .collect(),
        }
    }
}
