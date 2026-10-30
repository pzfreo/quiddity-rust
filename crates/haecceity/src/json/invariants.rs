//! Deserialize constructor-constrained model values through their public constructors.
use crate::pmi::*;
use serde::Deserialize;

impl TryFrom<String> for Decimal {
    type Error = ModelError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::parse(&s)
    }
}
impl TryFrom<String> for DatumLabel {
    type Error = ModelError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::new(&s)
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BoundsInput {
    upper: Value,
    lower: Value,
}
impl TryFrom<BoundsInput> for Bounds {
    type Error = ModelError;
    fn try_from(v: BoundsInput) -> Result<Self, Self::Error> {
        Self::new(v.upper, v.lower)
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DatumInput {
    label: DatumLabel,
    feature: Option<FeatureId>,
    targets: Vec<DatumTargetId>,
}
impl TryFrom<DatumInput> for Datum {
    type Error = ModelError;
    fn try_from(v: DatumInput) -> Result<Self, Self::Error> {
        Self::new(v.label, v.feature, v.targets)
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TargetInput {
    number: u32,
    shape: TargetShape,
    placement: Option<Placement>,
    movable: Option<Direction>,
    on: Option<FeatureId>,
}
impl TryFrom<TargetInput> for DatumTarget {
    type Error = ModelError;
    fn try_from(v: TargetInput) -> Result<Self, Self::Error> {
        Self::new(v.number, v.shape, v.placement, v.movable, v.on)
    }
}
impl TryFrom<Vec<Compartment>> for DatumSystem {
    type Error = ModelError;
    fn try_from(v: Vec<Compartment>) -> Result<Self, Self::Error> {
        Self::new(v)
    }
}
