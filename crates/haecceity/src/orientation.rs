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
    /// retain their original face senses, signed mass and classification behaviour, as does a
    /// solid whose faces have only seam or vertex loops (a full sphere or torus), whose wire
    /// sense cannot be established.
    ///
    /// Most boundaries face outward, and proving that is cheap: `outward_proved` finds a face
    /// with an outward witness by the same ray-parity test, so the full test (every face's
    /// sampled box and the flux integrals over every face) runs only for a boundary where that
    /// proof is not found.
    pub fn repair_inverted_shells(mut self) -> Self {
        for solid in 0..self.solids.len() {
            if !self.solid_is_valid(solid) || outward_proved(&self, solid) {
                continue;
            }
            if !self
                .solid_mass(solid)
                .is_some_and(|(v, _)| v.is_finite() && v < 0.0)
            {
                continue;
            }
            let classifier = Classifier::for_solid(&self, solid);
            if self.solids[solid]
                .faces
                .iter()
                .all(|&f| face_side(&self, &classifier, f) == Side::Inward)
                && let Some(reverse_wires) = inward_wires(&self, solid)
            {
                self.record_inverted_shell_repair(solid, reverse_wires);
            }
        }
        self
    }
}

/// How many of a solid's faces [`outward_proved`] tries before leaving the solid to the full
/// test.
const PROOF_FACES: usize = 8;

/// Whether one of the solid's first [`PROOF_FACES`] faces has an outward witness (a point a
/// little in front of it outside the material, a little behind it inside, at two offsets) by
/// ray parity against boxes that certainly contain each face ([`certain_box`]). A face whose
/// normal leaves the material at one regular point does so everywhere, so the boundary is not
/// entirely inward and the repair cannot apply: it needs every face to be found inward by the
/// same parity test, under the same assumption that parity places a point correctly. A sampled
/// box prunes the full classifier's rays; the certain boxes here prune none of the crossings the
/// kernel's own contact test admits, and need no face's interior sampled to be built. A face
/// found inward, or no witness either way, leaves the solid to the full test.
fn outward_proved(part: &Part, solid: usize) -> bool {
    let faces = &part.solids[solid].faces;
    let mut boxes = vec![crate::geom::Bounds::empty(); part.faces.len()];
    for &f in faces {
        let Some(b) = certain_box(part, f) else {
            return false;
        };
        boxes[f] = b;
    }
    let classifier = Classifier::with_face_boxes(part, faces.clone(), boxes);
    for &f in faces.iter().take(PROOF_FACES) {
        match face_side(part, &classifier, f) {
            Side::Outward => return true,
            Side::Inward => return false,
            Side::Unproved => {}
        }
    }
    false
}

/// A box that certainly holds face *f* and the bands of its edges, without sampling its
/// interior. A planar, cylindrical or conical face lies on lines (any line in the plane, the
/// rulings) that leave it through its boundary, or at a cone's apex, so it lies in the convex
/// hull of its boundary and apex; a sphere or torus face lies in its whole surface's box, and a
/// B-spline face in its control points' (the convex-hull property, for positive weights). The
/// boundary is bounded by its curves': a segment's ends, a conic's whole circle, a B-spline
/// curve's control points. The pad covers edges straying from the surface (and the bands about
/// them that contact admits) far beyond any tolerance a file states. `None` for a surface or
/// curve whose box is not certain.
fn certain_box(part: &Part, f: usize) -> Option<crate::geom::Bounds> {
    use crate::geom::{Bounds, Curve, Surface};
    let around = |c: [f64; 3], r: f64| Bounds {
        min: c.map(|x| x - r),
        max: c.map(|x| x + r),
    };
    let face = &part.faces[f];
    let mut b = Bounds::empty();
    let boundary = |b: &mut Bounds| -> Option<()> {
        for lp in &face.loops {
            if let Some(p) = lp.vertex {
                b.add(p);
            }
            for &(e, _) in &lp.edges {
                let edge = &part.edges[e];
                b.add(edge.start);
                b.add(edge.end);
                match &edge.curve {
                    Curve::Line { .. } => {}
                    Curve::Circle { frame, radius } => b.merge(&around(frame.origin, *radius)),
                    Curve::Ellipse { frame, major, .. } => b.merge(&around(frame.origin, *major)),
                    Curve::Nurbs(c) => {
                        if !c.weights().iter().all(|&w| w > 0.0) {
                            return None;
                        }
                        c.control_points().iter().for_each(|&p| b.add(p));
                    }
                }
            }
        }
        Some(())
    };
    match &face.surface {
        Surface::Plane { .. } | Surface::Cylinder { .. } => boundary(&mut b)?,
        Surface::Cone { .. } => {
            boundary(&mut b)?;
            b.add(face.surface.cone_apex()?);
        }
        Surface::Sphere { frame, radius } => b = around(frame.origin, *radius),
        Surface::Torus {
            frame,
            major,
            minor,
        } => b = around(frame.origin, major + minor),
        Surface::Freeform { surface, .. } => {
            if !surface.weights.iter().flatten().all(|&w| w > 0.0) {
                return None;
            }
            let (min, max) = surface.control_bounds();
            b = Bounds { min, max };
        }
        Surface::Other { .. } => return None,
    }
    let pad = 0.1 + 1e-3 * b.diagonal();
    (pad.is_finite() && b.min.iter().chain(&b.max).all(|x| x.is_finite())).then(|| Bounds {
        min: b.min.map(|x| x - pad),
        max: b.max.map(|x| x + pad),
    })
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

/// Which way a face faces material, by its witnesses.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    /// Three regular interior witnesses face into material, and none out of it.
    Inward,
    /// A witness faces out of material.
    Outward,
    /// Neither established.
    Unproved,
}

fn face_side(part: &Part, classifier: &Classifier<'_>, face: usize) -> Side {
    let Some(domain) = part.domain(face) else {
        return Side::Unproved;
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
                    return Side::Outward;
                }
                if far == (State::In, State::Out) && near == far {
                    inward = true;
                    break;
                }
            }
            if inward {
                witnesses += 1;
                if witnesses == 3 {
                    return Side::Inward;
                }
                // Spread the witnesses across the face rather than keeping three at one u.
                break;
            }
        }
    }
    Side::Unproved
}
