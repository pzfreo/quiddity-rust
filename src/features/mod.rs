//! The feature recognisers, ported family by family from the Python implementation, and the
//! per-run machinery they share.
//!
//! Each family module exposes a public `recognise_*` (records only, the Python public
//! entry point) and a `discover` that takes the run's [`Context`] and returns
//! [`Occurrence`]s carrying the faces that define each record. Shared analysis — the cylinder
//! inventory, the point classifier, the part's box — lives in the context so a run computes it
//! once however many families ask.

pub mod angled_steps;
pub mod bevel;
pub mod body;
pub mod bosses;
pub mod chamfers;
pub mod context;
pub mod countersinks;
pub mod cylinders;
pub mod evidence;
pub mod fillets;
pub mod flats;
pub mod hole_patterns;
pub mod holes;
pub mod levels;
pub mod oriented_chamfers;
pub mod paired_ramp_steps;
pub mod planes;
pub mod stacks;
pub mod turned;

pub use context::Context;
pub use evidence::{EvidenceError, Occurrence};

use serde::Serialize;

use crate::kernel::brep::Part;

/// Every ported family's records for one part, computed in one run that shares its analysis.
/// Families are independent here: the Python aggregate's cross-family reconciliation
/// (`build_recognition_result`) is not ported.
#[derive(Clone, Debug, Serialize)]
pub struct Features {
    pub fillets: Vec<fillets::Fillet>,
    pub chamfers: Vec<chamfers::Chamfer>,
    pub bosses: Vec<bosses::BossRecord>,
    pub angled_steps: Vec<angled_steps::AngledStep>,
    pub flats: Vec<flats::Flat>,
    pub paired_ramp_steps: Vec<paired_ramp_steps::PairedRampStep>,
    pub oriented_chamfers: Vec<oriented_chamfers::OrientedChamfer>,
    pub holes: Vec<holes::HoleRecord>,
    pub countersinks: Vec<countersinks::CounterSink>,
    pub hole_patterns: Vec<hole_patterns::HolePattern>,
}

/// Recognise every ported family on *part* with default options; holes carry their
/// countersinks, and patterns are found among those holes.
pub fn recognise(part: &Part) -> Features {
    let ctx = Context::new(part);
    let seats = countersinks::discover(&ctx);
    let holes = records(holes::discover(&ctx, &seats));
    let countersinks = records(seats);
    Features {
        fillets: records(fillets::discover(&ctx, &Default::default())),
        chamfers: records(chamfers::discover(&ctx, &Default::default())),
        bosses: records(bosses::discover(&ctx)),
        angled_steps: records(angled_steps::discover(&ctx)),
        flats: records(flats::discover(&ctx)),
        paired_ramp_steps: records(paired_ramp_steps::discover(&ctx)),
        oriented_chamfers: records(oriented_chamfers::discover(&ctx, &Default::default())),
        hole_patterns: hole_patterns::recognise_hole_patterns(&holes),
        holes,
        countersinks,
    }
}

/// The records of a family's occurrences, in order.
pub fn records<R>(found: Vec<Occurrence<R>>) -> Vec<R> {
    found.into_iter().map(|o| o.record).collect()
}
