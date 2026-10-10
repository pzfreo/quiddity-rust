//! The feature recognisers, ported family by family from the Python implementation, and the
//! per-run machinery they share.
//!
//! Each family module exposes a public `recognise_*` (records only, the Python public
//! entry point) and a `discover` that takes the run's [`Context`] and returns
//! [`Occurrence`]s carrying the faces that define each record. Shared analysis — the cylinder
//! inventory, the point classifier, the part's box — lives in the context so a run computes it
//! once however many families ask.

pub mod analytic_surfaces;
pub mod angled_steps;
pub mod bevel;
pub mod blend_view;
pub mod blends;
pub mod body;
pub mod bosses;
pub mod chamfers;
pub mod channels;
pub mod circular_blind_steps;
pub mod circular_face_patterns;
pub mod context;
pub mod countersinks;
pub mod cylinders;
pub mod cylindrical_channels;
pub mod cylindrical_end_surface;
pub mod cylindrical_passages;
pub mod cylindrical_pockets;
pub mod cylindrical_seats;
pub mod edge_open;
pub mod edge_open_circular;
pub mod edge_open_prismatic;
pub mod effective_surfaces;
pub mod entry_treatments;
pub mod evidence;
pub mod experimental_geometry;
pub mod fillets;
pub mod flats;
pub mod freeform_surfaces;
pub mod geometry_evidence;
pub mod graph;
pub mod grooves;
pub mod gussets;
pub mod hole_patterns;
pub mod holes;
pub mod interior_voids;
pub mod levels;
pub mod oblique_through_steps;
pub mod oriented_chamfers;
pub mod oriented_slots;
pub mod outer_profile;
pub mod pads;
pub mod paired_ramp_steps;
pub mod passage_compat;
pub mod passages;
pub(crate) mod pattern_geometry;
pub mod plane_envelope_passages;
pub mod planes;
pub mod plates;
pub mod pockets;
pub mod policy;
pub mod polygonal_bosses;
pub mod prismatic_pockets;
pub mod probes;
pub mod profiled_bores;
pub mod recess_core;
pub mod recess_faces;
pub mod recess_obround;
pub mod recess_patterns;
pub mod recess_radii;
pub mod recess_records;
pub mod recess_reduce;
pub mod reconcile;
pub mod rectangular_blind_slots;
pub mod regions;
pub mod repeating_profiles;
pub mod rings;
pub mod round_bottom_slots;
pub mod section_passages;
pub mod section_recess;
pub mod section_recess_discovery;
pub mod section_recess_family;
pub mod section_recess_geometry;
pub mod sections;
pub mod sheet_metal;
pub mod slots;
pub mod solid_properties;
pub mod stacks;
pub mod support_patches;
pub mod thin_walls;
pub mod through_steps;
pub mod turned;
pub mod turned_steps;
pub mod volume_probe;
pub mod wire_seed;

pub use context::Context;
pub use evidence::{EvidenceError, Occurrence};

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::kernel::brep::Part;

/// Every ported family's records for one part, computed in one run that shares its analysis.
/// [`recognise`] gives the records the Python aggregate's cross-family reconciliation
/// (`_reconcile_existing`, [`reconcile`]) accepts, with patterns derived from accepted members
/// only; [`inventory`] keeps every candidate and the decisions.
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
    pub rectangular_blind_slots: Vec<rectangular_blind_slots::RectangularBlindSlot>,
    pub double_d_bores: Vec<profiled_bores::DoubleDBore>,
    pub edge_open_circular_pockets: Vec<edge_open_circular::EdgeOpenCircularPocket>,
    pub edge_open_prismatic_recesses: Vec<edge_open_prismatic::EdgeOpenPrismaticRecess>,
    pub blends: Vec<blends::Blend>,
    pub sheet_metal_bodies: Vec<sheet_metal::SheetMetalBody>,
    pub slots: Vec<slots::Slot>,
    pub pockets: Vec<pockets::Pocket>,
    pub channels: Vec<channels::Channel>,
    pub slot_patterns: Vec<recess_patterns::SlotPattern>,
    pub pocket_patterns: Vec<recess_patterns::PocketPattern>,
    pub repeating_radial_profiles: Vec<repeating_profiles::RepeatingRadialProfile>,
    pub freeform_surfaces: Vec<freeform_surfaces::FreeformSurface>,
    pub polygonal_bosses: Vec<polygonal_bosses::PolygonalBoss>,
    pub polygonal_stock: Vec<polygonal_bosses::PolygonalStock>,
    /// Python's legacy inventory field (`_LegacyRecognitionResult.section_passages`): every
    /// accepted section passage (reconciliation rejects a passage a slot or an oriented slot
    /// dimensions).
    pub section_passages: Vec<passages::SectionPassage>,
    pub prismatic_pockets: Vec<prismatic_pockets::PrismaticPocket>,
    pub oriented_slots: Vec<oriented_slots::OrientedSlot>,
    pub oriented_slot_patterns: Vec<oriented_slots::OrientedSlotPattern>,
    pub pads: Vec<pads::RaisedPad>,
    /// The aggregate's step levels (`RecognitionResult.step_levels`, `_discover_step_levels`),
    /// unproved like every family here (Python's proof: [`levels::proved`]).
    pub step_levels: Vec<levels::FaceLevel>,
    /// The aggregate's risers (`RecognitionResult.risers`, `_discover_risers`), less those
    /// reconciliation rejects for a thin-wall body.
    pub risers: Vec<levels::RiserEvidence>,
    /// Each family's defining faces, record by record, under the family's field name. Derived
    /// families (hole, gusset rib, slot, pocket and oriented slot patterns) have none of their own: their
    /// members' faces are theirs ([`crate::correspondence`]).
    #[serde(skip)]
    pub defining: Defining,
}

/// Defining faces by family field name, then record.
pub type Defining = BTreeMap<&'static str, Vec<Vec<usize>>>;

/// Recognise every ported family on *part* with default options, as Python's aggregate does:
/// holes carry their countersinks, every candidate the reconciliation rejects is dropped (with
/// its defining faces), and patterns are found among the accepted holes, gusset ribs, slots,
/// pockets and oriented slots (`_take_inventory_once`). Python refuses the whole recognition
/// where two rules decide one candidate; `recognise` has no refusal to return, so it panics with
/// the message.
pub fn recognise(part: &Part) -> Features {
    inventory(part)
        .unwrap_or_else(|e| panic!("reconciliation refused: {e}"))
        .accepted()
}

/// One run's physical candidates and the aggregate's decisions on them (Python's
/// `InventoryProduct.physical`, its evidence and its `reconciliation`).
#[derive(Clone, Debug)]
pub struct Inventory {
    /// Every family's candidates before reconciliation, each with its defining faces. The
    /// derived pattern families are empty: Python derives them from accepted members only.
    pub physical: Features,
    /// What the rules read beyond the records.
    pub evidence: reconcile::Evidence,
    /// Every decision the rules make, in rule order (no default acceptances).
    pub dispositions: Vec<reconcile::Disposition>,
    /// Each family's constituent faces, candidate by candidate under the family's field name
    /// (Python's `EvidenceIndex.constituent_of`): its defining faces and the further faces its
    /// proof consulted, sorted. The section-recess projection reads them
    /// ([`section_recess_family`]).
    pub constituent: Defining,
}

impl Inventory {
    /// The accepted inventory (`accepted_set` on every family) with its patterns derived
    /// (`_derive_patterns`): each rejected candidate's record and defining faces dropped, in
    /// order.
    pub fn accepted(self) -> Features {
        let mut f = self.physical;
        let mut rejected: BTreeMap<reconcile::Family, BTreeSet<usize>> = BTreeMap::new();
        for d in &self.dispositions {
            if d.outcome == reconcile::Outcome::Rejected {
                rejected
                    .entry(d.candidate.family)
                    .or_default()
                    .insert(d.candidate.index);
            }
        }
        for (family, indices) in &rejected {
            use reconcile::Family as F;
            match family {
                F::AngledSteps => drop_rejected(&mut f.angled_steps, indices),
                F::Blends => drop_rejected(&mut f.blends, indices),
                F::Bosses => drop_rejected(&mut f.bosses, indices),
                F::Chamfers => drop_rejected(&mut f.chamfers, indices),
                F::CircularBlindSteps => drop_rejected(&mut f.circular_blind_steps, indices),
                F::DoubleDBores => drop_rejected(&mut f.double_d_bores, indices),
                F::EdgeOpenCircularPockets => {
                    drop_rejected(&mut f.edge_open_circular_pockets, indices)
                }
                F::Fillets => drop_rejected(&mut f.fillets, indices),
                F::Grooves => drop_rejected(&mut f.grooves, indices),
                F::Holes => drop_rejected(&mut f.holes, indices),
                F::OrientedSlots => drop_rejected(&mut f.oriented_slots, indices),
                F::Passages => drop_rejected(&mut f.section_passages, indices),
                F::Plates => drop_rejected(&mut f.plates, indices),
                F::Pockets => drop_rejected(&mut f.pockets, indices),
                F::PrismaticPockets => drop_rejected(&mut f.prismatic_pockets, indices),
                F::RectangularBlindSlots => drop_rejected(&mut f.rectangular_blind_slots, indices),
                F::Risers => {
                    // Not a reconciliation field (`Family::field`), so dropped here.
                    drop_rejected(&mut f.risers, indices);
                    let defining = f.defining.get_mut("risers").expect("risers are carried");
                    drop_rejected(defining, indices);
                    continue;
                }
                F::Slots => drop_rejected(&mut f.slots, indices),
                F::ThinWallBodies => drop_rejected(&mut f.thin_wall_bodies, indices),
                F::TurnedSteps => drop_rejected(&mut f.turned_steps, indices),
            }
            let field = family.field().expect("a carried family has a field");
            let defining = f
                .defining
                .get_mut(field)
                .expect("every family has defining faces");
            drop_rejected(defining, indices);
        }
        f.hole_patterns = hole_patterns::recognise_hole_patterns(&f.holes);
        f.gusset_rib_patterns = gussets::recognise_gusset_rib_patterns(&f.gusset_ribs);
        f.slot_patterns = recess_patterns::recognise_slot_patterns(&f.slots);
        f.pocket_patterns = recess_patterns::recognise_pocket_patterns(&f.pockets);
        f.oriented_slot_patterns =
            oriented_slots::recognise_oriented_slot_patterns(&f.oriented_slots);
        f
    }
}

/// *items* without those at *indices*, in order.
fn drop_rejected<T>(items: &mut Vec<T>, indices: &BTreeSet<usize>) {
    let mut at = 0;
    items.retain(|_| {
        let keep = !indices.contains(&at);
        at += 1;
        keep
    });
}

/// Every ported family's candidates on *part* with default options and the reconciliation's
/// decisions on them, or the reconciliation's refusal.
pub fn inventory(part: &Part) -> Result<Inventory, reconcile::ReconcileError> {
    inventory_in(&Context::new(part))
}

/// [`inventory`] in a run the caller keeps, so a projection of it reads the same analysis.
pub(crate) fn inventory_in(ctx: &Context<'_>) -> Result<Inventory, reconcile::ReconcileError> {
    let seats = countersinks::discover(ctx);
    let mut defining = Kept::default();
    let holes = kept(&mut defining, "holes", holes::discover(ctx, &seats));
    let countersinks = kept(&mut defining, "countersinks", seats);
    let gusset_ribs = kept(&mut defining, "gusset_ribs", gussets::discover(ctx));
    let slots = kept(&mut defining, "slots", slots::discover(ctx));
    let pocket_occurrences = pockets::discover(ctx);
    let thin_wall_bodies = kept(&mut defining, "thin_wall_bodies", thin_walls::discover(ctx));
    // Python's aggregate raises on the same internal inconsistencies, refusing the whole
    // recognition; `inventory` carries only the reconciliation's refusal, so it panics with the
    // message.
    let passages =
        passages::discover(ctx).unwrap_or_else(|e| panic!("section passages refused: {e}"));
    // The passages' refusal is the panic above, so the evidence's own discovery of them agrees.
    let evidence = reconcile::Evidence::with_pockets(ctx, &pocket_occurrences)?;
    let pockets = kept(&mut defining, "pockets", pocket_occurrences);
    let oriented_slots = kept(
        &mut defining,
        "oriented_slots",
        oriented_slots::discover(ctx, &passages)
            .unwrap_or_else(|e| panic!("oriented slots refused: {e}")),
    );
    let freeform_surfaces = kept(
        &mut defining,
        "freeform_surfaces",
        freeform_surfaces::discover(ctx, &thin_wall_bodies),
    );
    let step_level_occurrences = levels::discover_step_levels(ctx);
    let risers = kept(
        &mut defining,
        "risers",
        levels::discover_risers(ctx, &step_level_occurrences),
    );
    let step_levels = kept(&mut defining, "step_levels", step_level_occurrences);
    let physical = Features {
        fillets: kept(
            &mut defining,
            "fillets",
            fillets::discover(ctx, &Default::default()),
        ),
        chamfers: kept(
            &mut defining,
            "chamfers",
            chamfers::discover(ctx, &Default::default()),
        ),
        bosses: kept(&mut defining, "bosses", bosses::discover(ctx)),
        angled_steps: kept(&mut defining, "angled_steps", angled_steps::discover(ctx)),
        flats: kept(&mut defining, "flats", flats::discover(ctx)),
        paired_ramp_steps: kept(
            &mut defining,
            "paired_ramp_steps",
            paired_ramp_steps::discover(ctx),
        ),
        oriented_chamfers: kept(
            &mut defining,
            "oriented_chamfers",
            oriented_chamfers::discover(ctx, &Default::default()),
        ),
        circular_face_patterns: kept(
            &mut defining,
            "circular_face_patterns",
            circular_face_patterns::discover(ctx),
        ),
        oblique_through_steps: kept(
            &mut defining,
            "oblique_through_steps",
            oblique_through_steps::discover(ctx),
        ),
        circular_blind_steps: kept(
            &mut defining,
            "circular_blind_steps",
            circular_blind_steps::discover(ctx),
        ),
        hole_patterns: Vec::new(),
        holes,
        gusset_rib_patterns: Vec::new(),
        gusset_ribs,
        countersinks,
        thin_wall_bodies,
        interior_voids: kept(
            &mut defining,
            "interior_voids",
            interior_voids::discover(ctx),
        ),
        through_steps: kept(&mut defining, "through_steps", through_steps::discover(ctx)),
        turned_steps: kept(
            &mut defining,
            "turned_steps",
            turned_steps::sorted_occurrences(turned_steps::discover(ctx)),
        ),
        grooves: kept(&mut defining, "grooves", grooves::discover(ctx)),
        plates: kept(
            &mut defining,
            "plates",
            plates::discover(ctx, &Default::default()),
        ),
        round_bottom_blind_slots: kept(
            &mut defining,
            "round_bottom_blind_slots",
            round_bottom_slots::discover(ctx),
        ),
        rectangular_blind_slots: kept(
            &mut defining,
            "rectangular_blind_slots",
            rectangular_blind_slots::discover(ctx),
        ),
        double_d_bores: kept(
            &mut defining,
            "double_d_bores",
            profiled_bores::discover(ctx),
        ),
        edge_open_circular_pockets: kept(
            &mut defining,
            "edge_open_circular_pockets",
            edge_open_circular::discover(ctx),
        ),
        edge_open_prismatic_recesses: kept(
            &mut defining,
            "edge_open_prismatic_recesses",
            edge_open_prismatic::discover(ctx),
        ),
        blends: kept(&mut defining, "blends", blends::discover(ctx)),
        sheet_metal_bodies: kept(
            &mut defining,
            "sheet_metal_bodies",
            sheet_metal::discover(ctx),
        ),
        slot_patterns: Vec::new(),
        slots,
        pocket_patterns: Vec::new(),
        pockets,
        channels: kept(&mut defining, "channels", channels::discover(ctx)),
        repeating_radial_profiles: kept(
            &mut defining,
            "repeating_radial_profiles",
            repeating_profiles::discover(ctx),
        ),
        freeform_surfaces,
        polygonal_bosses: kept(
            &mut defining,
            "polygonal_bosses",
            polygonal_bosses::discover(ctx, &Default::default()),
        ),
        polygonal_stock: kept(
            &mut defining,
            "polygonal_stock",
            polygonal_bosses::discover_stock(ctx, &Default::default()),
        ),
        section_passages: kept(&mut defining, "section_passages", passages),
        prismatic_pockets: kept(
            &mut defining,
            "prismatic_pockets",
            prismatic_pockets::discover(ctx),
        ),
        oriented_slot_patterns: Vec::new(),
        oriented_slots,
        pads: kept(
            &mut defining,
            "pads",
            pads::discover(ctx, &Default::default())
                .unwrap_or_else(|e| panic!("rectangular pads refused: {e}")),
        ),
        step_levels,
        risers,
        defining: defining.defining,
    };
    // The thin-wall rule reads the risers this inventory carries, in its order.
    let mut evidence = evidence;
    evidence.risers = physical.defining["risers"].clone();
    let dispositions = reconcile::reconcile(&physical, &evidence)?;
    Ok(Inventory {
        physical,
        evidence,
        dispositions,
        constituent: defining.constituent,
    })
}

/// Defining and constituent faces by family field name, then record.
#[derive(Default)]
struct Kept {
    defining: Defining,
    constituent: Defining,
}

/// The records of a family's occurrences, their defining and constituent faces kept under the
/// family's name.
fn kept<R>(kept: &mut Kept, family: &'static str, found: Vec<Occurrence<R>>) -> Vec<R> {
    kept.defining
        .insert(family, found.iter().map(|o| o.defining.clone()).collect());
    kept.constituent.insert(
        family,
        found
            .iter()
            .map(|o| {
                let faces: BTreeSet<usize> = o.defining.iter().chain(&o.context).copied().collect();
                faces.into_iter().collect()
            })
            .collect(),
    );
    records(found)
}

/// The records of a family's occurrences, in order.
pub fn records<R>(found: Vec<Occurrence<R>>) -> Vec<R> {
    found.into_iter().map(|o| o.record).collect()
}
