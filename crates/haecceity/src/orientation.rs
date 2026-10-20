//! Conservative, reported repair of an entirely inward closed solid boundary.
//!
//! A negative flux is only a candidate: mixed face senses can also give a negative volume.
//! Ray parity is independent of the senses and supplies the material side. Every face must
//! have three regular interior witnesses whose normal points into material, at two offsets.
//! Any outward witness, unresolved face, open topology or insufficient evidence leaves the
//! entire solid unchanged. Coherently inward boundary wires reverse with the normals;
//! outward wires survive a normals-only repair. This does not sew or heal mixed shells.

use crate::brep::{Part, holds};
use crate::classify::{Classifier, ON_TOLERANCE, State};
use crate::geom;

impl Part {
    /// Repair a closed boundary only when its signed volume is negative and every face is
    /// established to face into material. STEP import applies this before returning the part;
    /// callers building in-memory geometry can apply it explicitly before any other queries.
    /// Coherently inward boundary wires are reversed too, preserving face validity and
    /// convexity; already outward wires are retained. Each repaired solid is named in
    /// [`Self::input_repairs`]. Mixed or unproved boundaries
    /// retain their original face senses, signed mass and classification behaviour.
    pub fn repair_inverted_shells(mut self) -> Self {
        for solid in 0..self.solids.len() {
            if !self.solid_is_valid(solid)
                || !self
                    .solid_mass(solid)
                    .is_some_and(|(v, _)| v.is_finite() && v < 0.0)
            {
                continue;
            }
            let classifier = Classifier::for_solid(&self, solid);
            if self.solids[solid]
                .faces
                .iter()
                .all(|&f| inward_face(&self, &classifier, f))
                && let Some(reverse_wires) = inward_wires(&self, solid)
            {
                self.record_inverted_shell_repair(solid, reverse_wires);
            }
        }
        self
    }
}

/// Whether the wires follow the inward normals too, requiring reversal with the faces.
/// A normals-only inversion leaves the wires outward and needs no wire change. Reuse the
/// integrator's local boundary-side test: a parameter polygon's winding is not evidence
/// around a pole or a periodic seam. Mixed or unproved wire senses refuse the repair.
fn inward_wires(part: &Part, solid: usize) -> Option<bool> {
    let mut sense = None;
    for &face in &part.solids[solid].faces {
        let f = &part.faces[face];
        for (i, lp) in f.loops.iter().enumerate() {
            // A vertex or a seam bounds no neighbourhood on the face by itself.
            let boundary = lp.edges.iter().any(|&(e, _)| {
                f.loops
                    .iter()
                    .flat_map(|l| &l.edges)
                    .filter(|&&(other, _)| other == e)
                    .count()
                    == 1
            });
            if !boundary {
                continue;
            }
            let left = (part.loop_direction(face, i, None)? > 0.0) ^ f.reversed;
            if sense.is_some_and(|s| s != left) {
                return None;
            }
            sense = Some(left);
        }
    }
    sense
}

fn inward_face(part: &Part, classifier: &Classifier<'_>, face: usize) -> bool {
    let Some(domain) = part.domain(face) else {
        return false;
    };
    let (u0, u1) = domain.u_range();
    let (v0, v1) = domain.v_range();
    let h = (part.face_bounds(face).diagonal() * 1e-5).max(64.0 * ON_TOLERANCE);
    let mut witnesses = 0;
    for a in [0.2113, 0.4331, 0.677, 0.819, 0.5] {
        for b in [0.413, 0.673, 0.293, 0.839, 0.5] {
            let (u, v) = (u0 + a * (u1 - u0), v0 + b * (v1 - v0));
            if !domain.contains(u, v) || !holds(domain, u, v, (1e-6 * (u1 - u0), 1e-6 * (v1 - v0)))
            {
                continue;
            }
            let Some(n) = part.face_normal(face, u, v) else {
                continue;
            };
            let p = part.faces[face].surface.value(u, v);
            let mut inward = false;
            for distance in [h, h / 4.0, h / 16.0] {
                if distance < 8.0 * ON_TOLERANCE {
                    break;
                }
                let sides = |d| {
                    (
                        classifier.classify(geom::add(p, geom::scale(n, d))),
                        classifier.classify(geom::sub(p, geom::scale(n, d))),
                    )
                };
                let (far, near) = (sides(distance), sides(distance / 2.0));
                if far == (State::Out, State::In) || near == (State::Out, State::In) {
                    return false;
                }
                if far == (State::In, State::Out) && near == far {
                    inward = true;
                    break;
                }
            }
            if inward {
                witnesses += 1;
                if witnesses == 3 {
                    return true;
                }
                // Spread the witnesses across the face rather than keeping three at one u.
                break;
            }
        }
    }
    false
}
