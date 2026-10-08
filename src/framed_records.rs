//! Records recognised in the part's own frame, reported in the file's coordinates (the
//! maintainer's decision on review M8: recognition canonicalises the frame first, so results do
//! not depend on how the part was placed).
//!
//! [`map_features`] carries every record of a [`Features`] through a rigid motion: points by the
//! whole motion, directions, normals and axes by its rotation, sizes, counts, faces and other
//! scalars unchanged. Many records also name a principal axis by letter (`'x'`, `"y"`) and give
//! coordinates measured along such axes (a plate's `lo` and `hi`, a slot's `w_center`, a
//! through step's two-dimensional section). Where the rotation is a signed permutation, as it is
//! for a part whose frame is aligned with the file's axes, those are carried exactly: the letter
//! renamed, the coordinate moved and, along a reversed axis, negated (an interval's ends swapped,
//! a sign flipped). Where it is not, no file axis corresponds, so the value stays as found in
//! the frame and its field path is listed in [`Mapped::local`]: a letter names the frame's axis,
//! and a coordinate is measured along it from the frame's origin. A value is never left
//! frame-relative without that label.
//!
//! Every record is destructured in full, with no `..`: a field or family added later fails to
//! compile here until its mapping is decided.
//!
//! Values are mapped, not re-rounded: a coordinate the recogniser rounded in the frame carries
//! that rounding plus the frame's origin, so it can differ from a caller-space recognition by up
//! to the field's published grid. Orders and signs a recogniser chooses by coordinates (records
//! within a family, a direction's canonical sense, which end of a through hole is its entry, a
//! section's starting vertex and direction) are the frame's and are kept, so they do not depend
//! on placement either; they can differ from caller-space recognition's, which chooses them by
//! the file's axes (`tests/fixtures/known_framed_document.json` lists each such difference).

use std::collections::BTreeMap;

use crate::features::Features;
use crate::features::angled_steps::AngledStep;
use crate::features::blends::{Blend, BlendPath, CircularBlendPath, StraightBlendPath};
use crate::features::body::BodyKey;
use crate::features::bosses::BossRecord;
use crate::features::chamfers::Chamfer;
use crate::features::circular_blind_steps::CircularBlindStep;
use crate::features::circular_face_patterns::CircularFacePattern;
use crate::features::countersinks::CounterSink;
use crate::features::edge_open_circular::{
    EdgeOpenCircularPocket, OpenCircularSection, OpenCircularSectionSegment,
};
use crate::features::edge_open_prismatic::{
    EdgeOpenPrismaticRecess, OpenPolygonalSection, OpenSectionOpening,
};
use crate::features::fillets::Fillet;
use crate::features::flats::Flat;
use crate::features::freeform_surfaces::{
    BSplineSurfaceSupport, FreeformSurface, SurfaceContinuityLink,
};
use crate::features::grooves::Groove;
use crate::features::gussets::{GussetRib, GussetRibPattern};
use crate::features::hole_patterns::HolePattern;
use crate::features::holes::{CounterBore, HoleRecord};
use crate::features::interior_voids::InteriorVoid;
use crate::features::oblique_through_steps::ObliqueThroughStep;
use crate::features::oriented_chamfers::OrientedChamfer;
use crate::features::paired_ramp_steps::PairedRampStep;
use crate::features::passages::{
    PassageEnds, PassageFrame, PassageSection, PassageSectionVertex, SectionPassage,
};
use crate::features::pattern_geometry::plane_uv;
use crate::features::plates::Plate;
use crate::features::polygonal_bosses::PolygonalPrism;
use crate::features::profiled_bores::DoubleDBore;
use crate::features::recess_patterns::{PocketPattern, SlotPattern};
use crate::features::recess_records::{Channel, Pocket, Slot};
use crate::features::rectangular_blind_slots::RectangularBlindSlot;
use crate::features::repeating_profiles::RepeatingRadialProfile;
use crate::features::round_bottom_slots::RoundBottomBlindSlot;
use crate::features::sheet_metal::{
    FlatOverlapWitness, FlatPatternPlan, FormedSheetFeature, SheetBend, SheetEdgeTreatment,
    SheetFlange, SheetMetalBody, UnfoldedBendStrip, UnfoldedFlangeFace,
};
use crate::features::thin_walls::{ShellHistoryHint, ThinWallBody, UnpairedWallFace, WallFacePair};
use crate::features::through_steps::ThroughStep;
use crate::features::turned::TurnedProfileKey;
use crate::features::turned_steps::TurnedStep;
use crate::frames::PartFrame;
use crate::kernel::geom::{V3, cross};
use crate::kernel::py;
use crate::kernel::step::Placement;

/// A rigid motion, `p ↦ rotation · p + translation`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rigid {
    pub rotation: [[f64; 3]; 3],
    pub translation: V3,
}

impl Rigid {
    /// The motion taking the frame's local coordinates to the caller's ([`PartFrame::to_world`]).
    pub fn from_frame(frame: &PartFrame) -> Rigid {
        let (x, y, z) = (frame.x, frame.y, frame.z);
        Rigid {
            rotation: [0, 1, 2].map(|i| [x[i], y[i], z[i]]),
            translation: frame.origin,
        }
    }

    /// The motion a reader's placement applies.
    pub fn from_placement(p: &Placement) -> Rigid {
        Rigid {
            rotation: p.map(|row| [row[0], row[1], row[2]]),
            translation: p.map(|row| row[3]),
        }
    }

    pub fn inverse(&self) -> Rigid {
        let r = self.rotation;
        let rotation = [0, 1, 2].map(|i| [r[0][i], r[1][i], r[2][i]]);
        let translation = [0, 1, 2].map(|i| -py::dot(&rotation[i], &self.translation));
        Rigid {
            rotation,
            translation,
        }
    }

    pub fn point(&self, p: V3) -> V3 {
        [0, 1, 2].map(|i| py::dot(&self.rotation[i], &p) + self.translation[i])
    }

    pub fn direction(&self, d: V3) -> V3 {
        [0, 1, 2].map(|i| py::dot(&self.rotation[i], &d))
    }

    /// Each source axis's target axis and sign, when the rotation is a signed permutation
    /// (every entry exactly 0 or ±1).
    pub fn axes(&self) -> Option<[(usize, f64); 3]> {
        let r = self.rotation;
        let mut out = [(0, 0.0); 3];
        let mut used = [false; 3];
        for (i, slot) in out.iter_mut().enumerate() {
            let rows: Vec<usize> = (0..3).filter(|&w| r[w][i] != 0.0).collect();
            let &[w] = rows.as_slice() else {
                return None;
            };
            if r[w][i].abs() != 1.0 || used[w] {
                return None;
            }
            used[w] = true;
            *slot = (w, r[w][i]);
        }
        Some(out)
    }
}

/// The field paths of each record left in the frame's local coordinates, by record ID
/// (`<family>/<index>`, as in the fingerprints and the recognition document). Paths are as the
/// fingerprint tables write them: `a.b` a nested record's field, `a[]` each item of a list.
pub type LocalFields = BTreeMap<String, Vec<String>>;

/// Records carried through a motion, and the fields that could not be.
pub struct Mapped {
    pub features: Features,
    pub local: LocalFields,
}

/// *features*, recognised on the framed working part, in the file's coordinates.
pub fn to_file(features: Features, frame: &PartFrame) -> Mapped {
    map_features(features, &Rigid::from_frame(frame))
}

const LETTERS: [char; 3] = ['x', 'y', 'z'];

/// A record's axis letter as an index.
fn index(letter: char) -> usize {
    LETTERS
        .iter()
        .position(|&l| l == letter)
        .unwrap_or_else(|| panic!("{letter:?} is not an axis letter"))
}

fn index_str(letter: &str) -> usize {
    let mut chars = letter.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => index(c),
        _ => panic!("{letter:?} is not an axis letter"),
    }
}

/// The two axes other than *axis*, in increasing order.
fn others(axis: usize) -> [usize; 2] {
    match axis {
        0 => [1, 2],
        1 => [0, 2],
        _ => [0, 1],
    }
}

/// One record's mapping: the motion, and the paths it left local.
struct Out<'a> {
    motion: &'a Rigid,
    axes: Option<[(usize, f64); 3]>,
    /// Prepended to every path (a pattern's members: `holes[].`).
    prefix: &'static str,
    local: Vec<String>,
}

impl<'a> Out<'a> {
    fn new(motion: &'a Rigid, prefix: &'static str) -> Out<'a> {
        Out {
            motion,
            axes: motion.axes(),
            prefix,
            local: Vec::new(),
        }
    }

    fn mark(&mut self, path: &str) {
        self.local.push(format!("{}{path}", self.prefix));
    }

    fn point(&self, p: V3) -> V3 {
        self.motion.point(p)
    }

    fn direction(&self, d: V3) -> V3 {
        self.motion.direction(d)
    }

    fn points(&self, ps: Vec<V3>) -> Vec<V3> {
        ps.into_iter().map(|p| self.point(p)).collect()
    }

    fn translation(&self, w: usize) -> f64 {
        self.motion.translation[w]
    }

    /// Source axis *axis*'s target axis and sign, or `None` (the path marked) when the rotation
    /// is not a signed permutation.
    fn target(&mut self, axis: usize, path: &str) -> Option<(usize, f64)> {
        match self.axes {
            Some(axes) => Some(axes[axis]),
            None => {
                self.mark(path);
                None
            }
        }
    }

    fn letter(&mut self, letter: char, path: &str) -> char {
        match self.target(index(letter), path) {
            Some((w, _)) => LETTERS[w],
            None => letter,
        }
    }

    fn letter_string(&mut self, letter: String, path: &str) -> String {
        match self.target(index_str(&letter), path) {
            Some((w, _)) => LETTERS[w].to_string(),
            None => letter,
        }
    }

    /// A coordinate along source axis *axis*.
    fn coordinate(&mut self, axis: usize, v: f64, path: &str) -> f64 {
        match self.target(axis, path) {
            Some((w, s)) => self.translation(w) + s * v,
            None => v,
        }
    }

    /// An ordered interval along source axis *axis*.
    fn interval(&mut self, axis: usize, (lo, hi): (f64, f64), path: &str) -> (f64, f64) {
        match self.target(axis, path) {
            Some((w, s)) => {
                let (a, b) = (self.translation(w) + s * lo, self.translation(w) + s * hi);
                if s > 0.0 { (a, b) } else { (b, a) }
            }
            None => (lo, hi),
        }
    }

    fn interval2(&mut self, axis: usize, [lo, hi]: [f64; 2], path: &str) -> [f64; 2] {
        let (lo, hi) = self.interval(axis, (lo, hi), path);
        [lo, hi]
    }

    /// A sign (±1) along source axis *axis*.
    fn sign(&mut self, axis: usize, sign: i32, path: &str) -> i32 {
        match self.target(axis, path) {
            Some((_, s)) if s < 0.0 => -sign,
            _ => sign,
        }
    }

    /// A point in the two axes other than source axis *axis* (absolute coordinates, in
    /// increasing axis order), in the target's two axes other than *axis*'s image.
    fn across(&mut self, axis: usize, p: [f64; 2], path: &str) -> [f64; 2] {
        let [a, b] = others(axis);
        let Some(axes) = self.axes else {
            self.mark(path);
            return p;
        };
        let ((wa, sa), (wb, sb)) = (axes[a], axes[b]);
        let (va, vb) = (
            self.translation(wa) + sa * p[0],
            self.translation(wb) + sb * p[1],
        );
        if wa < wb { [va, vb] } else { [vb, va] }
    }

    /// Whether [`Out::across`] reverses the section's handedness (1 or −1; 1 when it is not
    /// mapped).
    fn across_turn(&self, axis: usize) -> f64 {
        let [a, b] = others(axis);
        match self.axes {
            Some(axes) => {
                let ((wa, sa), (wb, sb)) = (axes[a], axes[b]);
                sa * sb * if wa < wb { 1.0 } else { -1.0 }
            }
            None => 1.0,
        }
    }

    /// A box in source axes: `min` then `max` per axis, as [`BodyKey`]'s first six entries
    /// (`interleaved` false) or a turned profile's `[min x, max x, min y, ...]` (`true`).
    fn bounds(&mut self, b: &[f64], interleaved: bool, path: &str) -> Vec<f64> {
        let Some(axes) = self.axes else {
            self.mark(path);
            return b.to_vec();
        };
        let at = |axis: usize, high: bool| {
            if interleaved {
                2 * axis + usize::from(high)
            } else {
                3 * usize::from(high) + axis
            }
        };
        let mut out = b.to_vec();
        for (i, &(w, s)) in axes.iter().enumerate() {
            let (lo, hi) = (b[at(i, false)], b[at(i, true)]);
            let t = self.translation(w);
            let (lo, hi) = if s > 0.0 {
                (t + lo, t + hi)
            } else {
                (t - hi, t - lo)
            };
            out[at(w, false)] = lo;
            out[at(w, true)] = hi;
        }
        out
    }

    /// A solid's key: its box, then its volume and area.
    fn body_key(&mut self, key: Option<BodyKey>, path: &str) -> Option<BodyKey> {
        key.map(|k| {
            let mut out = self.bounds(&k[..6], false, path);
            out.extend_from_slice(&k[6..]);
            out
        })
    }

    /// A pattern's orientation, in degrees in its plane's basis ([`plane_uv`] of *normal*), and
    /// reduced modulo *period*: the same direction in the target's basis of the turned normal.
    fn pattern_angle(&self, normal: V3, degrees: f64, period: f64) -> f64 {
        let (u, v) = plane_uv(normal);
        let (s, c) = degrees.to_radians().sin_cos();
        let direction = self.direction([0, 1, 2].map(|i| c * u[i] + s * v[i]));
        let (u, v) = plane_uv(self.direction(normal));
        let turned = py::dot(&direction, &v)
            .atan2(py::dot(&direction, &u))
            .to_degrees();
        // Round-off is cleared before the reduction, so 0 does not come back as 180.
        let reduced = py::round_to(py::modulo(py::round_to(turned, 9), period), 2);
        if reduced >= period {
            reduced - period
        } else {
            reduced
        }
    }

    fn done(mut self) -> Vec<String> {
        self.local.sort();
        self.local.dedup();
        self.local
    }
}

/// *features*, carried through *motion*: see the module documentation.
pub fn map_features(features: Features, motion: &Rigid) -> Mapped {
    let Features {
        fillets,
        chamfers,
        bosses,
        angled_steps,
        flats,
        paired_ramp_steps,
        oriented_chamfers,
        circular_face_patterns,
        oblique_through_steps,
        circular_blind_steps,
        holes,
        countersinks,
        hole_patterns,
        gusset_ribs,
        gusset_rib_patterns,
        thin_wall_bodies,
        interior_voids,
        through_steps,
        turned_steps,
        grooves,
        plates,
        round_bottom_blind_slots,
        rectangular_blind_slots,
        double_d_bores,
        edge_open_circular_pockets,
        edge_open_prismatic_recesses,
        blends,
        sheet_metal_bodies,
        slots,
        pockets,
        channels,
        slot_patterns,
        pocket_patterns,
        repeating_radial_profiles,
        freeform_surfaces,
        polygonal_bosses,
        polygonal_stock,
        section_passages,
        defining,
    } = features;
    let mut local = LocalFields::new();
    let mut family = |name: &str, n: usize, paths: Vec<String>| {
        if !paths.is_empty() {
            local.insert(format!("{name}/{n}"), paths);
        }
    };
    macro_rules! each {
        ($name:literal, $records:expr, $f:expr) => {
            $records
                .into_iter()
                .enumerate()
                .map(|(n, r)| {
                    let mut out = Out::new(motion, "");
                    let r = $f(&mut out, r);
                    family($name, n, out.done());
                    r
                })
                .collect()
        };
    }
    let features = Features {
        fillets: each!("fillets", fillets, fillet),
        chamfers: each!("chamfers", chamfers, chamfer),
        bosses: each!("bosses", bosses, boss),
        angled_steps: each!("angled_steps", angled_steps, angled_step),
        flats: each!("flats", flats, flat),
        paired_ramp_steps: each!("paired_ramp_steps", paired_ramp_steps, paired_ramp_step),
        oriented_chamfers: each!("oriented_chamfers", oriented_chamfers, oriented_chamfer),
        circular_face_patterns: each!(
            "circular_face_patterns",
            circular_face_patterns,
            circular_face_pattern
        ),
        oblique_through_steps: each!(
            "oblique_through_steps",
            oblique_through_steps,
            oblique_through_step
        ),
        circular_blind_steps: each!(
            "circular_blind_steps",
            circular_blind_steps,
            circular_blind_step
        ),
        holes: each!("holes", holes, hole),
        countersinks: each!("countersinks", countersinks, countersink),
        hole_patterns: each!("hole_patterns", hole_patterns, hole_pattern),
        gusset_ribs: each!("gusset_ribs", gusset_ribs, gusset_rib),
        gusset_rib_patterns: each!(
            "gusset_rib_patterns",
            gusset_rib_patterns,
            gusset_rib_pattern
        ),
        thin_wall_bodies: each!("thin_wall_bodies", thin_wall_bodies, thin_wall_body),
        interior_voids: each!("interior_voids", interior_voids, interior_void),
        through_steps: each!("through_steps", through_steps, through_step),
        turned_steps: each!("turned_steps", turned_steps, turned_step),
        grooves: each!("grooves", grooves, groove),
        plates: each!("plates", plates, plate),
        round_bottom_blind_slots: each!(
            "round_bottom_blind_slots",
            round_bottom_blind_slots,
            round_bottom_blind_slot
        ),
        rectangular_blind_slots: each!(
            "rectangular_blind_slots",
            rectangular_blind_slots,
            rectangular_blind_slot
        ),
        double_d_bores: each!("double_d_bores", double_d_bores, double_d_bore),
        edge_open_circular_pockets: each!(
            "edge_open_circular_pockets",
            edge_open_circular_pockets,
            edge_open_circular_pocket
        ),
        edge_open_prismatic_recesses: each!(
            "edge_open_prismatic_recesses",
            edge_open_prismatic_recesses,
            edge_open_prismatic_recess
        ),
        blends: each!("blends", blends, blend),
        sheet_metal_bodies: each!("sheet_metal_bodies", sheet_metal_bodies, sheet_metal_body),
        slots: each!("slots", slots, slot),
        pockets: each!("pockets", pockets, pocket),
        channels: each!("channels", channels, channel),
        slot_patterns: each!("slot_patterns", slot_patterns, slot_pattern),
        pocket_patterns: each!("pocket_patterns", pocket_patterns, pocket_pattern),
        repeating_radial_profiles: each!(
            "repeating_radial_profiles",
            repeating_radial_profiles,
            repeating_radial_profile
        ),
        freeform_surfaces: each!("freeform_surfaces", freeform_surfaces, freeform_surface),
        polygonal_bosses: each!("polygonal_bosses", polygonal_bosses, polygonal_prism),
        polygonal_stock: each!("polygonal_stock", polygonal_stock, polygonal_prism),
        section_passages: each!("section_passages", section_passages, section_passage),
        // The working part's faces are the caller's, under the same indices.
        defining,
    };
    Mapped { features, local }
}

fn fillet(out: &mut Out, r: Fillet) -> Fillet {
    let Fillet {
        axis,
        radius,
        at,
        turned,
        side,
    } = r;
    Fillet {
        axis: out.letter(axis, "axis"),
        radius,
        at: out.point(at),
        turned,
        side,
    }
}

fn chamfer(out: &mut Out, r: Chamfer) -> Chamfer {
    let Chamfer {
        axis,
        leg1,
        leg2,
        angle,
        at,
        turned,
        corner,
    } = r;
    Chamfer {
        axis: out.letter(axis, "axis"),
        leg1,
        leg2,
        angle,
        at: out.point(at),
        turned,
        corner: corner.map(|c| out.point(c)),
    }
}

fn boss(out: &mut Out, r: BossRecord) -> BossRecord {
    let BossRecord {
        axis,
        location,
        diameter,
        height,
    } = r;
    BossRecord {
        axis: out.direction(axis),
        location: out.point(location),
        diameter,
        height,
    }
}

fn angled_step(out: &mut Out, r: AngledStep) -> AngledStep {
    let AngledStep {
        axis,
        leg1,
        leg2,
        angle,
        length,
        at,
        corner,
    } = r;
    AngledStep {
        axis: out.letter(axis, "axis"),
        leg1,
        leg2,
        angle,
        length,
        at: out.point(at),
        corner: corner.map(|c| out.point(c)),
    }
}

/// `axis_direction` is the stock cylinder's direction (canonical along `axis` in the frame),
/// `stock_span` is measured along it from the origin, and `axis_line` is where the axis line
/// crosses the plane through the origin, in the other two axes.
fn flat(out: &mut Out, r: Flat) -> Flat {
    let Flat {
        axis,
        across,
        at,
        axis_line,
        stock_span,
        axis_direction,
    } = r;
    let i = index(axis);
    let [a, b] = others(i);
    let d = axis_direction;
    let mut foot = [0.0; 3];
    foot[a] = axis_line.0;
    foot[b] = axis_line.1;
    foot[i] = -(foot[a] * d[a] + foot[b] * d[b]) / d[i];
    let target = out.target(i, "axis");
    let direction = out.direction(d);
    let shift = py::dot(&out.motion.translation, &direction);
    let stock_span = (stock_span.0 + shift, stock_span.1 + shift);
    let (axis, axis_line) = match target {
        Some((w, _)) => {
            let q = out.point(foot);
            let along = py::dot(&q, &direction);
            let foot = [0, 1, 2].map(|k| q[k] - along * direction[k]);
            let [wa, wb] = others(w);
            (LETTERS[w], (foot[wa], foot[wb]))
        }
        None => {
            out.mark("axis_line");
            (axis, axis_line)
        }
    };
    Flat {
        axis,
        across,
        at: out.point(at),
        axis_line,
        stock_span,
        axis_direction: direction,
    }
}

/// `half_widths` is low side first along the axis the two ramps oppose across, which the record
/// does not name: it is kept when both axes across the run keep their sense, swapped when both
/// reverse, and left local otherwise.
fn paired_ramp_step(out: &mut Out, r: PairedRampStep) -> PairedRampStep {
    let PairedRampStep {
        axis,
        angle,
        length,
        at,
        opening_direction,
        half_width,
        half_widths,
    } = r;
    let i = index(axis);
    let half_widths = half_widths.map(|(low, high)| {
        let [a, b] = others(i);
        match out.axes.map(|axes| (axes[a].1, axes[b].1)) {
            Some((sa, sb)) if sa > 0.0 && sb > 0.0 => (low, high),
            Some((sa, sb)) if sa < 0.0 && sb < 0.0 => (high, low),
            _ => {
                out.mark("half_widths");
                (low, high)
            }
        }
    });
    PairedRampStep {
        axis: out.letter(axis, "axis"),
        angle,
        length,
        at: out.point(at),
        opening_direction: out.direction(opening_direction),
        half_width,
        half_widths,
    }
}

fn oriented_chamfer(out: &mut Out, r: OrientedChamfer) -> OrientedChamfer {
    let OrientedChamfer {
        run,
        length,
        at,
        corner,
        leg1,
        leg2,
        leg1_direction,
        leg2_direction,
        support_spans,
        angle,
        body_key,
    } = r;
    OrientedChamfer {
        run: out.direction(run),
        length,
        at: out.point(at),
        corner: out.point(corner),
        leg1,
        leg2,
        leg1_direction: out.direction(leg1_direction),
        leg2_direction: out.direction(leg2_direction),
        // Measured along `run` from the chamfer's mid-station: they turn with it.
        support_spans,
        angle,
        body_key: out.body_key(body_key, "body_key"),
    }
}

fn circular_face_pattern(out: &mut Out, r: CircularFacePattern) -> CircularFacePattern {
    let CircularFacePattern {
        axis_origin,
        axis_direction,
        count,
        pitch_degrees,
        seed_index,
        fit_error,
    } = r;
    CircularFacePattern {
        axis_origin: out.point(axis_origin),
        axis_direction: out.direction(axis_direction),
        count,
        pitch_degrees,
        seed_index,
        fit_error,
    }
}

fn oblique_through_step(out: &mut Out, r: ObliqueThroughStep) -> ObliqueThroughStep {
    let ObliqueThroughStep {
        run,
        length,
        at,
        depth_direction,
        depth,
        across_direction,
        wall_outline,
        body_key,
    } = r;
    ObliqueThroughStep {
        run: out.direction(run),
        length,
        at: out.point(at),
        depth_direction: out.direction(depth_direction),
        depth,
        across_direction: out.direction(across_direction),
        // Along `run` and across the wall from the wall's start: they turn with the record.
        wall_outline,
        body_key: out.body_key(body_key, "body_key"),
    }
}

fn circular_blind_step(out: &mut Out, r: CircularBlindStep) -> CircularBlindStep {
    let CircularBlindStep {
        axis,
        radius,
        length,
        centreline,
        section,
    } = r;
    let i = index(axis);
    CircularBlindStep {
        axis: out.letter(axis, "axis"),
        radius,
        length,
        centreline: centreline.map(|p| out.point(p)),
        section: section.map(|p| out.across(i, p, "section")),
    }
}

fn hole(out: &mut Out, r: HoleRecord) -> HoleRecord {
    let HoleRecord {
        axis,
        location,
        diameter,
        depth,
        bottom,
        cbore,
        spotface,
        csink,
    } = r;
    HoleRecord {
        axis: out.direction(axis),
        location: out.point(location),
        diameter,
        depth,
        bottom,
        cbore: cbore.map(counterbore),
        spotface: spotface.map(counterbore),
        csink: csink.map(|c| countersink(out, c)),
    }
}

fn countersink(out: &mut Out, r: CounterSink) -> CounterSink {
    let CounterSink {
        axis,
        location,
        major_diameter,
        drill_diameter,
        included_angle,
        depth,
    } = r;
    CounterSink {
        axis: out.direction(axis),
        location: out.point(location),
        major_diameter,
        drill_diameter,
        included_angle,
        depth,
    }
}

fn holes(out: &mut Out, holes: Vec<HoleRecord>) -> Vec<HoleRecord> {
    holes.into_iter().map(|h| hole(out, h)).collect()
}

/// The members' common drilling axis, which the grid's plane basis is taken from.
fn drilling_axis(holes: &[HoleRecord]) -> V3 {
    holes.first().expect("a hole pattern has members").axis
}

fn hole_pattern(out: &mut Out, r: HolePattern) -> HolePattern {
    match r {
        HolePattern::RectGrid {
            holes: members,
            rows,
            cols,
            row_pitch,
            col_pitch,
            angle,
            center,
        } => HolePattern::RectGrid {
            angle: out.pattern_angle(drilling_axis(&members), angle, 180.0),
            holes: holes(out, members),
            rows,
            cols,
            row_pitch,
            col_pitch,
            center: out.point(center),
        },
        HolePattern::RectangularHoleSet {
            holes: members,
            center,
            width,
            height,
            angle,
        } => HolePattern::RectangularHoleSet {
            // A square set's angle is reduced modulo 90 degrees.
            angle: out.pattern_angle(
                drilling_axis(&members),
                angle,
                if width == height { 90.0 } else { 180.0 },
            ),
            holes: holes(out, members),
            center: out.point(center),
            width,
            height,
        },
        HolePattern::BoltCircle {
            holes: members,
            center,
            diameter,
        } => HolePattern::BoltCircle {
            holes: holes(out, members),
            center: out.point(center),
            diameter,
        },
        HolePattern::LinearArray {
            holes: members,
            pitch,
            direction,
        } => HolePattern::LinearArray {
            holes: holes(out, members),
            pitch,
            direction: out.direction(direction),
        },
    }
}

/// `supports` are the two support planes across the thickness axis (letter and coordinate) in
/// axis order; `legs` and `directions` (the side of each support the rib stands on) follow them.
fn gusset_rib(out: &mut Out, r: GussetRib) -> GussetRib {
    let GussetRib {
        thickness_axis,
        thickness_bounds,
        supports: ((la, ca), (lb, cb)),
        legs,
        directions,
        body_key,
    } = r;
    let t = index(thickness_axis);
    let thickness_bounds = out.interval(t, thickness_bounds, "thickness_bounds");
    let (ia, ib) = (index(la), index(lb));
    let first = (
        (
            out.letter(la, "supports"),
            out.coordinate(ia, ca, "supports"),
        ),
        legs.0,
        out.sign(ia, directions.0, "directions"),
    );
    let second = (
        (
            out.letter(lb, "supports"),
            out.coordinate(ib, cb, "supports"),
        ),
        legs.1,
        out.sign(ib, directions.1, "directions"),
    );
    let (first, second) = if out.axes.is_none() {
        out.mark("legs");
        (first, second)
    } else if first.0.0 > second.0.0 {
        (second, first)
    } else {
        (first, second)
    };
    GussetRib {
        thickness_axis: out.letter(thickness_axis, "thickness_axis"),
        thickness_bounds,
        supports: (first.0, second.0),
        legs: (first.1, second.1),
        directions: (first.2, second.2),
        body_key: out.body_key(body_key, "body_key"),
    }
}

fn gusset_rib_pattern(out: &mut Out, r: GussetRibPattern) -> GussetRibPattern {
    let ribs = |out: &mut Out, ribs: Vec<GussetRib>| {
        let mut members = Out::new(out.motion, "ribs[].");
        let ribs = ribs
            .into_iter()
            .map(|r| gusset_rib(&mut members, r))
            .collect();
        out.local.extend(members.done());
        ribs
    };
    match r {
        GussetRibPattern::Array {
            ribs: members,
            axis,
            pitch,
        } => GussetRibPattern::Array {
            ribs: ribs(out, members),
            axis: out.letter(axis, "axis"),
            pitch,
        },
        GussetRibPattern::MirrorPair {
            ribs: members,
            mirror_plane: (letter, at),
        } => GussetRibPattern::MirrorPair {
            ribs: ribs(out, members),
            mirror_plane: (
                out.letter(letter, "mirror_plane"),
                out.coordinate(index(letter), at, "mirror_plane"),
            ),
        },
    }
}

fn thin_wall_body(out: &mut Out, r: ThinWallBody) -> ThinWallBody {
    let ThinWallBody {
        body_index,
        body_key,
        thickness,
        face_pairs,
        unpaired_faces,
        rim_regions,
        paired_area_fraction,
        history_hint,
        unpaired_face_classes,
    } = r;
    ThinWallBody {
        body_index,
        body_key: out.body_key(body_key, "body_key"),
        thickness,
        face_pairs: wall_pairs(face_pairs),
        unpaired_faces,
        rim_regions,
        paired_area_fraction,
        history_hint: shell_history_hint(history_hint),
        unpaired_face_classes: unpaired_face_classes
            .into_iter()
            .map(unpaired_wall_face)
            .collect(),
    }
}

fn interior_void(out: &mut Out, r: InteriorVoid) -> InteriorVoid {
    let InteriorVoid {
        body_index,
        body_key,
        void_faces,
        openings,
        estimated_volume,
        volume_method,
        grid_pitch,
        enclosed_samples,
        air_samples,
    } = r;
    InteriorVoid {
        body_index,
        body_key: out.body_key(body_key, "body_key"),
        void_faces,
        openings,
        estimated_volume,
        volume_method,
        grid_pitch,
        enclosed_samples,
        air_samples,
    }
}

/// `section` is boundary endpoint, concave corner, boundary endpoint (the lesser direction in the
/// frame first), and `endpoint_scopes` follow its two endpoints.
fn through_step(out: &mut Out, r: ThroughStep) -> ThroughStep {
    let ThroughStep {
        axis,
        length,
        at,
        section,
        body_key,
        endpoint_scopes,
    } = r;
    let i = index(axis);
    let section = section.map(|p| out.across(i, p, "section"));
    if out.axes.is_none() {
        out.mark("endpoint_scopes");
    }
    ThroughStep {
        axis: out.letter(axis, "axis"),
        length,
        at: out.point(at),
        section,
        body_key: out.body_key(body_key, "body_key"),
        endpoint_scopes,
    }
}

fn turned_step(out: &mut Out, r: TurnedStep) -> TurnedStep {
    let TurnedStep {
        axis,
        lo,
        hi,
        diameter,
        profile,
    } = r;
    let (lo, hi) = out.interval(index(axis), (lo, hi), "lo");
    if out.axes.is_none() {
        out.mark("hi");
    }
    TurnedStep {
        axis: out.letter(axis, "axis"),
        lo,
        hi,
        diameter,
        profile: profile.map(|p| turned_profile(out, p)),
    }
}

/// `axis_origin` is a point of the turning axis with its own coordinate zeroed; `body_bounds`
/// the body's box, interleaved.
fn turned_profile(out: &mut Out, p: TurnedProfileKey) -> TurnedProfileKey {
    let TurnedProfileKey {
        axis,
        axis_origin,
        body_bounds,
        body_key,
    } = p;
    let axis_origin = match out.target(index(axis), "profile.axis_origin") {
        Some((w, _)) => {
            let mut q = out.point(axis_origin);
            q[w] = 0.0;
            q
        }
        None => axis_origin,
    };
    let bounds = out.bounds(&body_bounds, true, "profile.body_bounds");
    TurnedProfileKey {
        axis: out.letter(axis, "profile.axis"),
        axis_origin,
        body_bounds: bounds.try_into().expect("six bounds"),
        body_key: out.body_key(body_key, "profile.body_key"),
    }
}

fn groove(out: &mut Out, r: Groove) -> Groove {
    let Groove {
        axis,
        width,
        diameter,
        at,
        profile,
    } = r;
    Groove {
        axis: out.letter(axis, "axis"),
        width,
        diameter,
        at: out.point(at),
        profile: profile.map(|p| turned_profile(out, p)),
    }
}

/// `lo` and `hi` are along the axis, `u` and `v` the faces' area-weighted centre in the other
/// two axes.
fn plate(out: &mut Out, r: Plate) -> Plate {
    let Plate {
        axis,
        lo,
        hi,
        u,
        v,
        body_key,
    } = r;
    let i = index(axis);
    let (lo, hi) = out.interval(i, (lo, hi), "lo");
    let [u, v] = out.across(i, [u, v], "u");
    if out.axes.is_none() {
        out.mark("hi");
        out.mark("v");
    }
    Plate {
        axis: out.letter(axis, "axis"),
        lo,
        hi,
        u,
        v,
        body_key: out.body_key(body_key, "body_key"),
    }
}

fn round_bottom_blind_slot(out: &mut Out, r: RoundBottomBlindSlot) -> RoundBottomBlindSlot {
    let RoundBottomBlindSlot {
        axis,
        open_sign,
        length,
        width_axis,
        depth_axis,
        depth_sign,
        radius,
        flat_width,
        at,
    } = r;
    RoundBottomBlindSlot {
        open_sign: out.sign(index(axis), open_sign, "open_sign"),
        axis: out.letter(axis, "axis"),
        length,
        width_axis: out.letter(width_axis, "width_axis"),
        depth_sign: out.sign(index(depth_axis), depth_sign, "depth_sign"),
        depth_axis: out.letter(depth_axis, "depth_axis"),
        radius,
        flat_width,
        at: out.point(at),
    }
}

fn rectangular_blind_slot(out: &mut Out, r: RectangularBlindSlot) -> RectangularBlindSlot {
    let RectangularBlindSlot {
        axis,
        open_sign,
        length,
        width_axis,
        depth_axis,
        depth_sign,
        width,
        depth,
        at,
    } = r;
    RectangularBlindSlot {
        open_sign: out.sign(index(axis), open_sign, "open_sign"),
        axis: out.letter(axis, "axis"),
        length,
        width_axis: out.letter(width_axis, "width_axis"),
        depth_sign: out.sign(index(depth_axis), depth_sign, "depth_sign"),
        depth_axis: out.letter(depth_axis, "depth_axis"),
        width,
        depth,
        at: out.point(at),
    }
}

fn double_d_bore(out: &mut Out, r: DoubleDBore) -> DoubleDBore {
    let DoubleDBore {
        axis,
        location,
        major_diameter,
        across_flats,
        depth,
        through,
        flat_direction,
    } = r;
    DoubleDBore {
        axis: out.direction(axis),
        location: out.point(location),
        major_diameter,
        across_flats,
        depth,
        through,
        flat_direction: out.direction(flat_direction),
    }
}

/// The section's points are in the two axes across the run; an arc's sweep is signed by the
/// section's handedness there.
fn edge_open_circular_pocket(out: &mut Out, r: EdgeOpenCircularPocket) -> EdgeOpenCircularPocket {
    let EdgeOpenCircularPocket {
        axis,
        run_interval,
        open_sign,
        section: OpenCircularSection { segments, opening },
    } = r;
    let i = index(axis);
    let turn = out.across_turn(i);
    let segments = segments
        .into_iter()
        .map(|s| {
            let OpenCircularSectionSegment {
                kind,
                start,
                end,
                center,
                radius,
                sweep,
            } = s;
            OpenCircularSectionSegment {
                kind,
                start: out.across(i, start, "section"),
                end: out.across(i, end, "section"),
                center: center.map(|c| out.across(i, c, "section")),
                radius,
                sweep: sweep.map(|w| turn * w),
            }
        })
        .collect();
    EdgeOpenCircularPocket {
        axis: out.letter(axis, "axis"),
        run_interval: out.interval2(i, run_interval, "run_interval"),
        open_sign: out.sign(i, open_sign, "open_sign"),
        section: OpenCircularSection {
            segments,
            opening: opening.map(|p| out.across(i, p, "section")),
        },
    }
}

/// The wall chain runs in its lesser direction in the frame, and the opening is from its last
/// corner to its first.
fn edge_open_prismatic_recess(
    out: &mut Out,
    r: EdgeOpenPrismaticRecess,
) -> EdgeOpenPrismaticRecess {
    let EdgeOpenPrismaticRecess {
        axis,
        run_interval,
        open_sign,
        section:
            OpenPolygonalSection {
                wall_chain,
                opening: OpenSectionOpening { start, end },
            },
    } = r;
    let i = index(axis);
    let wall_chain = wall_chain
        .into_iter()
        .map(|p| out.across(i, p, "section"))
        .collect();
    let (start, end) = (
        out.across(i, start, "section"),
        out.across(i, end, "section"),
    );
    EdgeOpenPrismaticRecess {
        axis: out.letter(axis, "axis"),
        run_interval: out.interval2(i, run_interval, "run_interval"),
        open_sign: out.sign(i, open_sign, "open_sign"),
        section: OpenPolygonalSection {
            wall_chain,
            opening: OpenSectionOpening { start, end },
        },
    }
}

fn blend(out: &mut Out, r: Blend) -> Blend {
    let Blend { radius, side, path } = r;
    let path = match path {
        BlendPath::Straight(StraightBlendPath { at, direction }) => {
            BlendPath::Straight(StraightBlendPath {
                at: out.point(at),
                direction: out.direction(direction),
            })
        }
        BlendPath::Circular(CircularBlendPath {
            center,
            normal,
            radius,
        }) => BlendPath::Circular(CircularBlendPath {
            center: out.point(center),
            normal: out.direction(normal),
            radius,
        }),
    };
    Blend { radius, side, path }
}

/// The flat pattern is laid out in the base flange's plane, in a basis taken from its normal and
/// a principal axis (`horizontal`), about the flange's origin: its points turn into the basis
/// the turned normal gives.
fn sheet_metal_body(out: &mut Out, r: SheetMetalBody) -> SheetMetalBody {
    let SheetMetalBody {
        body_index,
        body_key,
        thickness,
        first_side_faces,
        second_side_faces,
        cut_edge_faces,
        formed_features,
        flanges,
        bends,
        flat_pattern,
        paired_area_fraction,
        flat_pattern_status,
        edge_treatments,
    } = r;
    let flat_pattern = flat_pattern.map(|plan| {
        let base = flanges
            .iter()
            .find(|f| f.index == plan.base_flange)
            .expect("the flat pattern's base flange is a flange");
        flat_plan(out, plan, base.normal)
    });
    let flanges = flanges
        .into_iter()
        .map(|f| {
            let SheetFlange {
                index,
                reference_faces,
                mate_faces,
                origin,
                normal,
                area,
            } = f;
            SheetFlange {
                index,
                reference_faces,
                mate_faces,
                origin: out.point(origin),
                normal: out.direction(normal),
                area,
            }
        })
        .collect();
    let bends = bends
        .into_iter()
        .map(|b| {
            let SheetBend {
                index,
                face_pairs,
                first_flange,
                second_flange,
                axis_origin,
                axis_direction,
                angle_degrees,
                inner_radius,
                reference_skin,
                neutral_radius,
                bend_allowance,
            } = b;
            SheetBend {
                index,
                face_pairs: wall_pairs(face_pairs),
                first_flange,
                second_flange,
                axis_origin: out.point(axis_origin),
                axis_direction: out.direction(axis_direction),
                angle_degrees,
                inner_radius,
                reference_skin,
                neutral_radius,
                bend_allowance,
            }
        })
        .collect();
    SheetMetalBody {
        body_index,
        body_key: out.body_key(body_key, "body_key"),
        thickness,
        first_side_faces,
        second_side_faces,
        cut_edge_faces,
        formed_features: formed_features.into_iter().map(formed_feature).collect(),
        flanges,
        bends,
        flat_pattern,
        paired_area_fraction,
        flat_pattern_status,
        edge_treatments: edge_treatments.into_iter().map(edge_treatment).collect(),
    }
}

/// `sheet_metal`'s flat basis of a flange normal.
fn flat_basis(normal: V3) -> Option<(V3, V3)> {
    let horizontal = if normal[2].abs() < 0.9 {
        [0.0, 0.0, 1.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let x = py::unit(cross(horizontal, normal))?;
    Some((x, cross(normal, x)))
}

fn flat_plan(out: &mut Out, plan: FlatPatternPlan, normal: V3) -> FlatPatternPlan {
    let FlatPatternPlan {
        k_factor,
        base_flange,
        tree_bends,
        flat_faces,
        bend_strips,
        tessellation_tolerance,
        valid_blank,
        overlap_witnesses,
    } = plan;
    let (from, to) = (flat_basis(normal), flat_basis(out.direction(normal)));
    let (Some((xs, ys)), Some((xt, yt))) = (from, to) else {
        panic!("a laid-out flange normal has a flat basis");
    };
    let turn = |p: [f64; 2]| {
        let d = out.direction([0, 1, 2].map(|i| p[0] * xs[i] + p[1] * ys[i]));
        [py::dot(&d, &xt), py::dot(&d, &yt)]
    };
    FlatPatternPlan {
        k_factor,
        base_flange,
        tree_bends,
        flat_faces: flat_faces
            .into_iter()
            .map(|f| {
                let UnfoldedFlangeFace {
                    source_face,
                    flange,
                    vertices,
                    triangles,
                } = f;
                UnfoldedFlangeFace {
                    source_face,
                    flange,
                    vertices: vertices.into_iter().map(turn).collect(),
                    triangles,
                }
            })
            .collect(),
        bend_strips: bend_strips
            .into_iter()
            .map(|s| {
                let UnfoldedBendStrip {
                    bend,
                    source_face_pair,
                    corners,
                } = s;
                UnfoldedBendStrip {
                    bend,
                    source_face_pair: wall_pair(source_face_pair),
                    corners: corners.map(turn),
                }
            })
            .collect(),
        tessellation_tolerance,
        valid_blank,
        overlap_witnesses: overlap_witnesses.into_iter().map(overlap_witness).collect(),
    }
}

/// A recess's three axes: `w_center` along the width axis, `lo` and `hi` along the long axis,
/// `d_lo` and `d_hi` (and `open_sign`) along the third, the depth axis.
struct RecessAxes {
    width: usize,
    long: usize,
    depth: usize,
}

impl RecessAxes {
    fn of(width_axis: char, long_axis: char) -> RecessAxes {
        let (width, long) = (index(width_axis), index(long_axis));
        RecessAxes {
            width,
            long,
            depth: 3 - width - long,
        }
    }
}

/// The recess's placement fields, carried: `(w_center, (lo, hi), (d_lo, d_hi))`.
fn recess_placement(
    out: &mut Out,
    axes: &RecessAxes,
    w_center: f64,
    run: (f64, f64),
    depth: (f64, f64),
) -> (f64, (f64, f64), (f64, f64)) {
    let w_center = out.coordinate(axes.width, w_center, "w_center");
    let run = out.interval(axes.long, run, "lo");
    let depth = out.interval(axes.depth, depth, "d_lo");
    if out.axes.is_none() {
        out.mark("hi");
        out.mark("d_hi");
    }
    (w_center, run, depth)
}

fn slot(out: &mut Out, r: Slot) -> Slot {
    let Slot {
        width_axis,
        long_axis,
        width,
        length,
        w_center,
        lo,
        hi,
        d_lo,
        d_hi,
        body_key,
        end_radius,
        corner_radius,
    } = r;
    let axes = RecessAxes::of(width_axis, long_axis);
    let (w_center, (lo, hi), (d_lo, d_hi)) =
        recess_placement(out, &axes, w_center, (lo, hi), (d_lo, d_hi));
    Slot {
        width_axis: out.letter(width_axis, "width_axis"),
        long_axis: out.letter(long_axis, "long_axis"),
        width,
        length,
        w_center,
        lo,
        hi,
        d_lo,
        d_hi,
        body_key: out.body_key(body_key, "body_key"),
        end_radius,
        corner_radius,
    }
}

fn pocket(out: &mut Out, r: Pocket) -> Pocket {
    let Pocket {
        width_axis,
        long_axis,
        width,
        length,
        depth,
        w_center,
        lo,
        hi,
        d_lo,
        d_hi,
        open_sign,
        edge_anchored,
        body_key,
        end_radius,
        corner_radius,
    } = r;
    let axes = RecessAxes::of(width_axis, long_axis);
    let (w_center, (lo, hi), (d_lo, d_hi)) =
        recess_placement(out, &axes, w_center, (lo, hi), (d_lo, d_hi));
    Pocket {
        width_axis: out.letter(width_axis, "width_axis"),
        long_axis: out.letter(long_axis, "long_axis"),
        width,
        length,
        depth,
        w_center,
        lo,
        hi,
        d_lo,
        d_hi,
        open_sign: out.sign(axes.depth, open_sign, "open_sign"),
        edge_anchored,
        body_key: out.body_key(body_key, "body_key"),
        end_radius,
        corner_radius,
    }
}

fn channel(out: &mut Out, r: Channel) -> Channel {
    let Channel {
        width_axis,
        long_axis,
        width,
        w_center,
        lo,
        hi,
        d_lo,
        d_hi,
        open_sign,
        body_key,
    } = r;
    let axes = RecessAxes::of(width_axis, long_axis);
    let (w_center, (lo, hi), (d_lo, d_hi)) =
        recess_placement(out, &axes, w_center, (lo, hi), (d_lo, d_hi));
    Channel {
        width_axis: out.letter(width_axis, "width_axis"),
        long_axis: out.letter(long_axis, "long_axis"),
        width,
        w_center,
        lo,
        hi,
        d_lo,
        d_hi,
        open_sign: out.sign(axes.depth, open_sign, "open_sign"),
        body_key: out.body_key(body_key, "body_key"),
    }
}

/// A recess grid's plane is across its members' depth axis (the positive unit vector along it).
fn depth_normal(width_axis: char, long_axis: char) -> V3 {
    let mut normal = [0.0; 3];
    normal[RecessAxes::of(width_axis, long_axis).depth] = 1.0;
    normal
}

fn slot_pattern(out: &mut Out, r: SlotPattern) -> SlotPattern {
    let members = |out: &mut Out, slots: Vec<Slot>| {
        let mut each = Out::new(out.motion, "slots[].");
        let slots = slots.into_iter().map(|s| slot(&mut each, s)).collect();
        out.local.extend(each.done());
        slots
    };
    match r {
        SlotPattern::SlotGrid {
            slots,
            rows,
            cols,
            row_pitch,
            col_pitch,
            angle,
            center,
        } => {
            let first = slots.first().expect("a slot pattern has members");
            let normal = depth_normal(first.width_axis, first.long_axis);
            SlotPattern::SlotGrid {
                angle: out.pattern_angle(normal, angle, 180.0),
                slots: members(out, slots),
                rows,
                cols,
                row_pitch,
                col_pitch,
                center: out.point(center),
            }
        }
        SlotPattern::SlotArray {
            slots,
            pitch,
            direction,
        } => SlotPattern::SlotArray {
            slots: members(out, slots),
            pitch,
            direction: out.direction(direction),
        },
    }
}

fn pocket_pattern(out: &mut Out, r: PocketPattern) -> PocketPattern {
    let members = |out: &mut Out, pockets: Vec<Pocket>| {
        let mut each = Out::new(out.motion, "pockets[].");
        let pockets = pockets.into_iter().map(|p| pocket(&mut each, p)).collect();
        out.local.extend(each.done());
        pockets
    };
    match r {
        PocketPattern::PocketGrid {
            pockets,
            rows,
            cols,
            row_pitch,
            col_pitch,
            angle,
            center,
        } => {
            let first = pockets.first().expect("a pocket pattern has members");
            let normal = depth_normal(first.width_axis, first.long_axis);
            PocketPattern::PocketGrid {
                angle: out.pattern_angle(normal, angle, 180.0),
                pockets: members(out, pockets),
                rows,
                cols,
                row_pitch,
                col_pitch,
                center: out.point(center),
            }
        }
        PocketPattern::PocketArray {
            pockets,
            pitch,
            direction,
        } => PocketPattern::PocketArray {
            pockets: members(out, pockets),
            pitch,
            direction: out.direction(direction),
        },
    }
}

fn repeating_radial_profile(out: &mut Out, r: RepeatingRadialProfile) -> RepeatingRadialProfile {
    let RepeatingRadialProfile {
        axis,
        centre,
        span,
        repeat_count,
        edge_count,
        sector_signature,
    } = r;
    let i = index_str(&axis);
    RepeatingRadialProfile {
        span: out.interval2(i, span, "span"),
        axis: out.letter_string(axis, "axis"),
        centre: out.point(centre),
        repeat_count,
        edge_count,
        // Lengths and polar shapes about the centre, whatever the traversal and phase.
        sector_signature,
    }
}

fn freeform_surface(out: &mut Out, r: FreeformSurface) -> FreeformSurface {
    let FreeformSurface {
        face,
        support_kind,
        support:
            BSplineSurfaceSupport {
                u_degree,
                v_degree,
                poles,
                weights,
                u_knots,
                v_knots,
                u_multiplicities,
                v_multiplicities,
                u_periodic,
                v_periodic,
            },
        construction_kind,
        construction_axis,
        construction_vector,
        continuity_group,
        continuity_links,
        offset_partner,
        offset_distance,
        offset_basis,
    } = r;
    FreeformSurface {
        face,
        support_kind,
        support: BSplineSurfaceSupport {
            u_degree,
            v_degree,
            poles: poles.into_iter().map(|row| out.points(row)).collect(),
            weights,
            u_knots,
            v_knots,
            u_multiplicities,
            v_multiplicities,
            u_periodic,
            v_periodic,
        },
        construction_kind,
        // A parameter direction (`u` or `v`), not a direction in space.
        construction_axis,
        construction_vector: construction_vector.map(|d| out.direction(d)),
        continuity_group,
        continuity_links: continuity_links.into_iter().map(continuity_link).collect(),
        offset_partner,
        offset_distance,
        offset_basis,
    }
}

/// `base` and `top` are along the prism's axis.
fn polygonal_prism(out: &mut Out, r: PolygonalPrism) -> PolygonalPrism {
    let PolygonalPrism {
        axis,
        center,
        side_count,
        across_flats,
        base,
        top,
        flat_directions,
        flat_centres,
    } = r;
    let i = index_str(&axis);
    let (base, top) = out.interval(i, (base, top), "base");
    if out.axes.is_none() {
        out.mark("top");
    }
    PolygonalPrism {
        axis: out.letter_string(axis, "axis"),
        center: out.point(center),
        side_count,
        across_flats,
        base,
        top,
        flat_directions: flat_directions
            .into_iter()
            .map(|d| out.direction(d))
            .collect(),
        flat_centres: out.points(flat_centres),
    }
}

/// The passage frame's origin is where its centre line crosses the plane through the origin
/// across `run`, and `run_interval` is measured along `run` from the origin; the section and the
/// ends' gradients are in the frame's `u` and `v`, which turn with it.
fn section_passage(out: &mut Out, r: SectionPassage) -> SectionPassage {
    let SectionPassage {
        frame: PassageFrame { origin, run, u, v },
        run_interval,
        section,
        ends,
    } = r;
    let run = out.direction(run);
    let q = out.point(origin);
    let along = py::dot(&q, &run);
    let shift = py::dot(&out.motion.translation, &run);
    SectionPassage {
        frame: PassageFrame {
            origin: [0, 1, 2].map(|i| q[i] - along * run[i]),
            run,
            u: out.direction(u),
            v: out.direction(v),
        },
        run_interval: (run_interval.0 + shift, run_interval.1 + shift),
        section: passage_section(section),
        ends: passage_ends(ends),
    }
}

// Nested records with nothing placed in them, destructured so that a placed field added later
// fails to compile until mapped.

fn counterbore(c: CounterBore) -> CounterBore {
    let CounterBore { diameter, depth } = c;
    CounterBore { diameter, depth }
}

fn wall_pair(p: WallFacePair) -> WallFacePair {
    let WallFacePair {
        first_face,
        second_face,
        thickness,
    } = p;
    WallFacePair {
        first_face,
        second_face,
        thickness,
    }
}

fn wall_pairs(pairs: Vec<WallFacePair>) -> Vec<WallFacePair> {
    pairs.into_iter().map(wall_pair).collect()
}

fn unpaired_wall_face(f: UnpairedWallFace) -> UnpairedWallFace {
    let UnpairedWallFace { face, kind } = f;
    UnpairedWallFace { face, kind }
}

fn shell_history_hint(h: ShellHistoryHint) -> ShellHistoryHint {
    let ShellHistoryHint {
        basis,
        direction,
        outer_faces,
        inner_faces,
        opening_rims,
        before_shell_collar_pairs,
        after_shell_cut_faces,
    } = h;
    ShellHistoryHint {
        basis,
        // `inward`: toward the shell's material, not a direction in space.
        direction,
        outer_faces,
        inner_faces,
        opening_rims,
        before_shell_collar_pairs: wall_pairs(before_shell_collar_pairs),
        after_shell_cut_faces,
    }
}

fn formed_feature(f: FormedSheetFeature) -> FormedSheetFeature {
    let FormedSheetFeature {
        faces,
        paired_faces,
    } = f;
    FormedSheetFeature {
        faces,
        paired_faces,
    }
}

fn edge_treatment(t: SheetEdgeTreatment) -> SheetEdgeTreatment {
    let SheetEdgeTreatment { face, kind, radius } = t;
    SheetEdgeTreatment { face, kind, radius }
}

fn overlap_witness(w: FlatOverlapWitness) -> FlatOverlapWitness {
    let FlatOverlapWitness {
        first_kind,
        first_index,
        second_kind,
        second_index,
        area,
    } = w;
    FlatOverlapWitness {
        first_kind,
        first_index,
        second_kind,
        second_index,
        area,
    }
}

fn continuity_link(l: SurfaceContinuityLink) -> SurfaceContinuityLink {
    let SurfaceContinuityLink { other_face, kind } = l;
    SurfaceContinuityLink { other_face, kind }
}

/// In the passage frame's `u` and `v`, about its origin.
fn passage_section(s: PassageSection) -> PassageSection {
    let PassageSection { boundary } = s;
    PassageSection {
        boundary: boundary.into_iter().map(passage_vertex).collect(),
    }
}

fn passage_vertex(v: PassageSectionVertex) -> PassageSectionVertex {
    let PassageSectionVertex { point, bulge } = v;
    PassageSectionVertex { point, bulge }
}

/// Gradients across the section in the passage frame's `u` and `v`.
fn passage_ends(e: PassageEnds) -> PassageEnds {
    let PassageEnds {
        low_capped,
        high_capped,
        low_gradient,
        high_gradient,
    } = e;
    PassageEnds {
        low_capped,
        high_capped,
        low_gradient,
        high_gradient,
    }
}
