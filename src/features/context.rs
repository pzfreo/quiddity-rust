//! One recognition run over one part: the part plus the analysis several families share,
//! each computed on first use (`FaceGraph` and `SolidProperties` in the Python implementation).
//!
//! The rule for where a cache lives: facts about the topology (neighbours, validity, face
//! bounds, parameter-space domains) belong to [`Part`]; recognition analysis that more than one
//! family reads (the classifier, the cylinder inventory) belongs here.

use std::sync::OnceLock;

use super::body::{BodyKey, body_signature, unambiguous_body_keys};
use super::cylinders::{CylinderEvidence, analyse_cylinders};
use crate::kernel::brep::Part;
use crate::kernel::classify::Classifier;
use crate::kernel::geom::Bounds;

pub struct Context<'a> {
    pub part: &'a Part,
    bounds: OnceLock<Bounds>,
    classifier: OnceLock<Classifier<'a>>,
    solid_classifiers: Vec<OnceLock<Classifier<'a>>>,
    cylinders: OnceLock<Vec<CylinderEvidence>>,
    signatures: OnceLock<Vec<Option<BodyKey>>>,
}

impl<'a> Context<'a> {
    pub fn new(part: &'a Part) -> Self {
        Context {
            part,
            bounds: OnceLock::new(),
            classifier: OnceLock::new(),
            solid_classifiers: part.solids.iter().map(|_| OnceLock::new()).collect(),
            cylinders: OnceLock::new(),
            signatures: OnceLock::new(),
        }
    }

    /// The whole part's box.
    pub fn bounds(&self) -> &Bounds {
        self.bounds.get_or_init(|| self.part.bounds())
    }

    /// The point classifier for the whole part.
    pub fn classifier(&self) -> &Classifier<'a> {
        self.classifier.get_or_init(|| Classifier::new(self.part))
    }

    /// The point classifier for one solid alone.
    pub fn solid_classifier(&self, solid: usize) -> &Classifier<'a> {
        self.solid_classifiers[solid].get_or_init(|| Classifier::for_solid(self.part, solid))
    }

    /// Each solid's body key (`unambiguous_body_keys`).
    pub fn body_keys(&self, require_valid_solid: bool) -> Vec<Option<BodyKey>> {
        let signatures = self.signatures.get_or_init(|| {
            (0..self.part.solids.len())
                .map(|s| body_signature(self.part, s))
                .collect()
        });
        unambiguous_body_keys(self.part, signatures, require_valid_solid)
    }

    /// Every native cylindrical face (`analyse_cylinders`), in solid then face order.
    pub fn cylinders(&self) -> &[CylinderEvidence] {
        self.cylinders.get_or_init(|| analyse_cylinders(self.part))
    }
}
