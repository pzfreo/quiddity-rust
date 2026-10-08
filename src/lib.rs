//! Quiddity: deterministic, geometry-only feature recognition for STEP B-Rep — Rust port.
//!
//! The recognisers ([`features`]) stand on the geometry kernel, [`haecceity`], re-exported as
//! [`kernel`].

pub mod correspondence;
pub mod features;
pub mod frames;
pub mod recognition;
pub mod serve;
/// The geometry kernel, [`haecceity`], under the name the recognisers use.
pub use haecceity as kernel;

pub use correspondence::{Correspondence, Fingerprints, correspond};
pub use features::angled_steps::{AngledStep, recognise_angled_steps};
pub use features::blends::{
    Blend, BlendPath, CircularBlendPath, StraightBlendPath, recognise_blends,
};
pub use features::bosses::{BossRecord, recognise_bosses};
pub use features::chamfers::{Chamfer, ChamferOptions, recognise_chamfers};
pub use features::channels::{Channel, recognise_channels};
pub use features::circular_blind_steps::{CircularBlindStep, recognise_circular_blind_steps};
pub use features::circular_face_patterns::{CircularFacePattern, recognise_circular_face_patterns};
pub use features::countersinks::{CounterSink, recognise_countersinks};
pub use features::edge_open_circular::{
    EdgeOpenCircularPocket, recognise_edge_open_circular_pockets,
};
pub use features::edge_open_prismatic::{
    EdgeOpenPrismaticRecess, recognise_edge_open_prismatic_recesses,
};
pub use features::fillets::{Fillet, FilletOptions, recognise_fillets};
pub use features::flats::{Flat, recognise_flats};
pub use features::grooves::{Groove, recognise_grooves};
pub use features::gussets::{
    GussetRib, GussetRibPattern, recognise_gusset_rib_patterns, recognise_gusset_ribs,
};
pub use features::hole_patterns::{HolePattern, recognise_hole_patterns};
pub use features::holes::{HoleOptions, HoleRecord, recognise_holes};
pub use features::interior_voids::{InteriorVoid, recognise_interior_voids};
pub use features::levels::{
    FaceLevel, FaceLevelOptions, RiserEvidence, RiserOptions, recognise_face_levels,
    recognise_risers,
};
pub use features::oblique_through_steps::{ObliqueThroughStep, recognise_oblique_through_steps};
pub use features::oriented_chamfers::{
    OrientedChamfer, OrientedChamferOptions, recognise_oriented_chamfers,
};
pub use features::paired_ramp_steps::{PairedRampStep, recognise_paired_ramp_steps};
pub use features::plates::{Plate, PlateOptions, recognise_plates};
pub use features::pockets::{Pocket, recognise_pockets};
pub use features::profiled_bores::{DoubleDBore, DoubleDBoreOptions, recognise_double_d_bores};
pub use features::rectangular_blind_slots::{
    RectangularBlindSlot, recognise_rectangular_blind_slots,
};
pub use features::round_bottom_slots::{RoundBottomBlindSlot, recognise_round_bottom_blind_slots};
pub use features::sheet_metal::{SheetMetalBody, SheetMetalOptions, recognise_sheet_metal_bodies};
pub use features::slots::{Slot, recognise_slots};
pub use features::thin_walls::{ThinWallBody, recognise_thin_wall_bodies};
pub use features::through_steps::{ThroughStep, recognise_through_steps};
pub use features::turned::TurnedProfileKey;
pub use features::turned_steps::{TurnedStep, recognise_turned_steps};
pub use kernel::brep::Part;
pub use kernel::step::{read_step, read_step_file};
