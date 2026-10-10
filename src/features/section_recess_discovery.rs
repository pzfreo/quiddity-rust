//! Aggregate discovery of geometry-first section recesses (`quiddity._section_recess_discovery`):
//! every candidate [`section_recess_geometry::candidates`](super::section_recess_geometry::
//! candidates) proves, numbered in its order as a [`SectionRecess`] with its classification and
//! face evidence.
//!
//! Python hands each record to its claims writer (`EvidenceWriter.add_defining`) with its
//! defining and constituent faces; here each is an [`Occurrence`] whose defining faces are the
//! record's and whose consulted faces are its other constituent faces, as the other families
//! publish them. Wiring the family into recognition and its public projection
//! (`recognise_section_recesses`, which runs Python's aggregate reconciliation) is not part of
//! this module.

use super::Context;
use super::effective_surfaces::EffectiveFaces;
use super::evidence::Occurrence;
use super::section_recess::{
    SectionRecess, SectionRecessClassification, SectionRecessError, SectionRecessEvidence,
};
use super::section_recess_geometry::{Candidate, candidates};

/// The part's native section recesses (`discover_section_recesses`), in candidate order.
pub fn discover_section_recesses(
    ctx: &Context<'_>,
    surfaces: &EffectiveFaces<'_, '_>,
) -> Result<Vec<Occurrence<SectionRecess>>, SectionRecessError> {
    section_recesses_of(candidates(ctx, surfaces)?)
}

/// The section recesses [`discover_section_recesses`] numbers from the part's candidates, for a
/// caller that already has them.
pub fn section_recesses_of(
    candidates: Vec<Candidate>,
) -> Result<Vec<Occurrence<SectionRecess>>, SectionRecessError> {
    candidates
        .into_iter()
        .enumerate()
        .map(|(index, candidate)| {
            let record = SectionRecess::new(
                index,
                candidate.body,
                candidate.geometry,
                SectionRecessClassification::new(candidate.feature_kind, candidate.section_shape)?,
                SectionRecessEvidence::new(
                    candidate.defining_faces.clone(),
                    candidate.constituent_faces.clone(),
                )?,
            )?;
            let context = candidate
                .constituent_faces
                .iter()
                .copied()
                .filter(|f| !candidate.defining_faces.contains(f))
                .collect();
            Ok(Occurrence {
                record,
                defining: candidate.defining_faces,
                context,
            })
        })
        .collect()
}
