//! One recognition run over one part: the part plus the analysis several families share,
//! each computed on first use (`FaceGraph` and `SolidProperties` in the Python implementation).

use std::sync::OnceLock;

use super::cylinders::{CylinderEvidence, analyse_cylinders};
use crate::kernel::brep::Part;
use crate::kernel::classify::Classifier;
use crate::kernel::geom::Bounds;

pub struct Context<'a> {
    pub part: &'a Part,
    bounds: OnceLock<Bounds>,
    classifier: OnceLock<Classifier<'a>>,
    cylinders: OnceLock<Vec<CylinderEvidence>>,
}

impl<'a> Context<'a> {
    pub fn new(part: &'a Part) -> Self {
        Context {
            part,
            bounds: OnceLock::new(),
            classifier: OnceLock::new(),
            cylinders: OnceLock::new(),
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

    /// Every native cylindrical face (`analyse_cylinders`), in solid then face order.
    pub fn cylinders(&self) -> &[CylinderEvidence] {
        self.cylinders.get_or_init(|| analyse_cylinders(self.part))
    }
}
