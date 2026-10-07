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
pub mod circular_blind_steps;
pub mod circular_face_patterns;
pub mod context;
pub mod countersinks;
pub mod cylinders;
pub mod evidence;
pub mod fillets;
pub mod flats;
pub mod grooves;
pub mod gussets;
pub mod hole_patterns;
pub mod holes;
pub mod interior_voids;
pub mod levels;
pub mod oblique_through_steps;
pub mod oriented_chamfers;
pub mod paired_ramp_steps;
pub mod planes;
pub mod plates;
pub mod probes;
pub mod regions;
pub mod round_bottom_slots;
pub mod stacks;
pub mod thin_walls;
pub mod through_steps;
pub mod turned;
pub mod turned_steps;
pub mod volume_probe;

pub use context::Context;
pub use evidence::{EvidenceError, Occurrence};

use std::collections::BTreeMap;

use serde::Serialize;

use crate::kernel::brep::Part;

/// Every ported family's records for one part, computed in one run that shares its analysis.
/// Families are independent here: the Python aggregate's cross-family reconciliation
/// (`build_recognition_result`) is not ported. Face levels and risers are measurements with no
/// evidence path yet, so they are called on their own.
#[derive(Clone, Debug, Serialize)]
pub struct Features {
    pub fillets: Vec<fillets::Fillet>,
    pub chamfers: Vec<chamfers::Chamfer>,
    pub bosses: Vec<bosses::BossRecord>,
    pub angled_steps: Vec<angled_steps::AngledStep>,
    pub flats: Vec<flats::Flat>,
    pub paired_ramp_steps: Vec<paired_ramp_steps::PairedRampStep>,
    pub oriented_chamfers: Vec<oriented_chamfers::OrientedChamfer>,
    pub circular_face_patterns: Vec<circular_face_patterns::CircularFacePattern>,
    pub oblique_through_steps: Vec<oblique_through_steps::ObliqueThroughStep>,
    pub circular_blind_steps: Vec<circular_blind_steps::CircularBlindStep>,
    pub holes: Vec<holes::HoleRecord>,
    pub countersinks: Vec<countersinks::CounterSink>,
    pub hole_patterns: Vec<hole_patterns::HolePattern>,
    pub gusset_ribs: Vec<gussets::GussetRib>,
    pub gusset_rib_patterns: Vec<gussets::GussetRibPattern>,
    pub thin_wall_bodies: Vec<thin_walls::ThinWallBody>,
    pub interior_voids: Vec<interior_voids::InteriorVoid>,
    pub through_steps: Vec<through_steps::ThroughStep>,
    pub turned_steps: Vec<turned_steps::TurnedStep>,
    pub grooves: Vec<grooves::Groove>,
    pub plates: Vec<plates::Plate>,
    pub round_bottom_blind_slots: Vec<round_bottom_slots::RoundBottomBlindSlot>,
    /// Each family's defining faces, record by record, under the family's field name. Derived
    /// families (hole and gusset rib patterns) have none of their own: their members' faces are
    /// theirs ([`crate::correspondence`]).
    #[serde(skip)]
    pub defining: Defining,
}

/// Defining faces by family field name, then record.
pub type Defining = BTreeMap<&'static str, Vec<Vec<usize>>>;

/// Recognise every ported family on *part* with default options; holes carry their
/// countersinks, and patterns are found among those holes.
pub fn recognise(part: &Part) -> Features {
    let ctx = Context::new(part);
    let seats = countersinks::discover(&ctx);
    let mut defining = BTreeMap::new();
    let holes = kept(&mut defining, "holes", holes::discover(&ctx, &seats));
    let countersinks = kept(&mut defining, "countersinks", seats);
    let gusset_ribs = kept(&mut defining, "gusset_ribs", gussets::discover(&ctx));
    Features {
        fillets: kept(
            &mut defining,
            "fillets",
            fillets::discover(&ctx, &Default::default()),
        ),
        chamfers: kept(
            &mut defining,
            "chamfers",
            chamfers::discover(&ctx, &Default::default()),
        ),
        bosses: kept(&mut defining, "bosses", bosses::discover(&ctx)),
        angled_steps: kept(&mut defining, "angled_steps", angled_steps::discover(&ctx)),
        flats: kept(&mut defining, "flats", flats::discover(&ctx)),
        paired_ramp_steps: kept(
            &mut defining,
            "paired_ramp_steps",
            paired_ramp_steps::discover(&ctx),
        ),
        oriented_chamfers: kept(
            &mut defining,
            "oriented_chamfers",
            oriented_chamfers::discover(&ctx, &Default::default()),
        ),
        circular_face_patterns: kept(
            &mut defining,
            "circular_face_patterns",
            circular_face_patterns::discover(&ctx),
        ),
        oblique_through_steps: kept(
            &mut defining,
            "oblique_through_steps",
            oblique_through_steps::discover(&ctx),
        ),
        circular_blind_steps: kept(
            &mut defining,
            "circular_blind_steps",
            circular_blind_steps::discover(&ctx),
        ),
        hole_patterns: hole_patterns::recognise_hole_patterns(&holes),
        holes,
        gusset_rib_patterns: gussets::recognise_gusset_rib_patterns(&gusset_ribs),
        gusset_ribs,
        countersinks,
        thin_wall_bodies: kept(
            &mut defining,
            "thin_wall_bodies",
            thin_walls::discover(&ctx),
        ),
        interior_voids: kept(
            &mut defining,
            "interior_voids",
            interior_voids::discover(&ctx),
        ),
        through_steps: kept(
            &mut defining,
            "through_steps",
            through_steps::discover(&ctx),
        ),
        turned_steps: kept(
            &mut defining,
            "turned_steps",
            turned_steps::sorted_occurrences(turned_steps::discover(&ctx)),
        ),
        grooves: kept(&mut defining, "grooves", grooves::discover(&ctx)),
        plates: kept(
            &mut defining,
            "plates",
            plates::discover(&ctx, &Default::default()),
        ),
        round_bottom_blind_slots: kept(
            &mut defining,
            "round_bottom_blind_slots",
            round_bottom_slots::discover(&ctx),
        ),
        defining,
    }
}

/// The records of a family's occurrences, their defining faces kept under the family's name.
fn kept<R>(defining: &mut Defining, family: &'static str, found: Vec<Occurrence<R>>) -> Vec<R> {
    defining.insert(family, found.iter().map(|o| o.defining.clone()).collect());
    records(found)
}

/// The records of a family's occurrences, in order.
pub fn records<R>(found: Vec<Occurrence<R>>) -> Vec<R> {
    found.into_iter().map(|o| o.record).collect()
}
