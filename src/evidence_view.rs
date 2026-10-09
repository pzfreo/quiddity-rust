//! The recognition evidence view (`quiddity.evidence.build_recognition_evidence` and
//! `_project_recognition_evidence`): one recognition run's accepted features and rejected
//! candidates, projected onto the part's faces.
//!
//! Per accepted feature: its family and record, its *defining* faces (those that establish it),
//! its *constituent* faces (those that physically belong to it: a hole's bore and floor, a
//! counterbore's faces), its instance groups (a pattern's members' constituent faces, or a
//! circular face pattern's repeated groups), its *host* faces (for holes and bosses, the planar
//! faces normal to the axis just outside it that it opens from or stands on: Python's
//! `_axial_host_nodes`) and, for a hole pattern, its members. Per projected candidate (every
//! rejected candidate and, transitively, the candidates it is related to): its family, the
//! aggregate's outcome and reason, its defining and constituent faces and its related
//! candidates.
//!
//! Python issues opaque run-local references (`FaceRef`, `FeatureRef`, `CandidateRef`); the
//! port uses plain indices, as elsewhere: a face is its index in `part.faces` (the order every
//! capture numbers faces in), a feature its position in [`RecognitionEvidence::features`] (so a
//! pattern's members are positions there, not copies of their records), and a candidate its
//! position in [`RecognitionEvidence::candidates`]. A feature's `index` is its position among
//! its family's accepted records ([`RecognitionEvidence::record`] reads it), a candidate's its
//! position among its family's physical candidates (before reconciliation).
//!
//! Features come family by family in Python's registry order (`PHYSICAL_DEFINITIONS`), leaving
//! out the recess source families as Python does, then the hole patterns; candidates in that
//! family order, then by index. The planar outer-profile evidence is read per face on demand
//! ([`RecognitionEvidence::planar_outer_profile`]), as Python's view reads it.
//!
//! Not ported, each stated in [`GAPS`]: the section recesses (and their refusals), which Python
//! lists after the physical families, and the step levels, a physical family the port does not
//! have. Nor are the view's association coverage, its bounded report (`RecognitionReport`) or
//! the framed variant (`build_framed_recognition_evidence`).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::Serialize;
use serde_json::Value;

use crate::features::reconcile::{Disposition, Family, Outcome, ReasonCode, ReconcileError};
use crate::features::{
    self, Context, Features, Inventory, angled_steps, bosses, circular_face_patterns, countersinks,
    edge_open_circular, holes, interior_voids, policy, polygonal_bosses, prismatic_pockets,
    section_passages, thin_walls,
};
use crate::kernel::brep::Part;
use crate::kernel::geom::{self, Surface};
use crate::{PlanarOuterProfileEvidence, RefusedPlanarOuterProfile, planar_outer_profile};

/// The physical families in Python's registry order (`PHYSICAL_DEFINITIONS`), by Python value.
pub const PHYSICAL_FAMILIES: [&str; 40] = [
    "countersinks",
    "holes",
    "double_d_bores",
    "bosses",
    "polygonal_bosses",
    "polygonal_stock",
    "channels",
    "slots",
    "rectangular_blind_slots",
    "round_bottom_blind_slots",
    "grooves",
    "flats",
    "pockets",
    "prismatic_pockets",
    "edge_open_circular_pockets",
    "edge_open_prismatic_recesses",
    "section_recesses",
    "pads",
    "repeating_radial_profiles",
    "circular_face_patterns",
    "turned_steps",
    "step_levels",
    "risers",
    "chamfers",
    "oriented_chamfers",
    "angled_steps",
    "paired_ramp_steps",
    "gusset_ribs",
    "through_steps",
    "oblique_through_steps",
    "circular_blind_steps",
    "passages",
    "oriented_slots",
    "blends",
    "fillets",
    "plates",
    "thin_wall_bodies",
    "interior_voids",
    "freeform_surfaces",
    "sheet_metal_bodies",
];

/// The recess source families (`RECESS_SOURCE_FAMILIES`): Python's view leaves them out of its
/// features (the section recesses aggregate them), though their candidates are projected.
pub const RECESS_SOURCE_FAMILIES: [&str; 8] = [
    "pockets",
    "channels",
    "prismatic_pockets",
    "passages",
    "edge_open_prismatic_recesses",
    "edge_open_circular_pockets",
    "rectangular_blind_slots",
    "round_bottom_blind_slots",
];

/// A family Python's view lists that this one does not, and why.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Gap {
    pub family: &'static str,
    pub reason: &'static str,
}

/// The view's stated gaps: features Python lists that are absent here (an absence, not a guess).
pub const GAPS: [Gap; 2] = [
    Gap {
        family: "section_recesses",
        reason: "the aggregate's section-recess projection (and its refusals) is not ported here; \
                 it awaits q-section-recess-family",
    },
    Gap {
        family: "step_levels",
        reason: "the step-level family (`levels.STEP_LEVELS`) is not ported",
    },
];

/// One accepted feature (a physical occurrence or a hole pattern).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EvidenceFeature {
    /// Python's family value (`passages` for the port's section passages).
    pub family: &'static str,
    /// Its position among the family's accepted records.
    pub index: usize,
    /// Faces, each list sorted.
    pub defining: Vec<usize>,
    pub constituent: Vec<usize>,
    /// Instance groups in order (each sorted), or none.
    pub groups: Vec<Vec<usize>>,
    pub hosts: Vec<usize>,
    /// A hole pattern's members, as positions in the view's features, in record order.
    pub members: Vec<usize>,
}

/// One projected candidate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EvidenceCandidate {
    pub family: &'static str,
    /// Its position among the family's physical candidates.
    pub index: usize,
    pub outcome: Outcome,
    /// The rule's reason; `None` is Python's default acceptance (`default.accepted`).
    pub reason: Option<ReasonCode>,
    pub defining: Vec<usize>,
    pub constituent: Vec<usize>,
    /// Its direct reconciliation links, as positions in the view's candidates.
    pub related: Vec<usize>,
}

impl EvidenceCandidate {
    /// Python's reason value.
    pub fn reason_value(&self) -> &'static str {
        self.reason.map_or("default.accepted", ReasonCode::value)
    }
}

/// The view's refusal: the reconciliation's, or evidence that does not describe one run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceViewError(pub String);

impl fmt::Display for EvidenceViewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for EvidenceViewError {}

impl From<ReconcileError> for EvidenceViewError {
    fn from(error: ReconcileError) -> Self {
        EvidenceViewError(error.0)
    }
}

/// One recognition run's evidence on *part* (`RecognitionEvidence`).
#[derive(Clone, Debug)]
pub struct RecognitionEvidence<'a> {
    part: &'a Part,
    /// The accepted inventory ([`features::recognise`]'s answer for the same run).
    pub result: Features,
    pub features: Vec<EvidenceFeature>,
    pub candidates: Vec<EvidenceCandidate>,
    /// The rejected candidates, as positions in `candidates`, in order.
    pub rejected_candidates: Vec<usize>,
}

impl RecognitionEvidence<'_> {
    /// The record of the feature at *position* in `features`, as its JSON.
    pub fn record(&self, position: usize) -> Value {
        let feature = &self.features[position];
        let records = serde_json::to_value(&self.result)
            .map(|v| v[field(feature.family)].clone())
            .expect("records serialise");
        records[feature.index].clone()
    }

    /// One face's planar outer profile (`RecognitionEvidence.planar_outer_profile`).
    pub fn planar_outer_profile(
        &self,
        face: usize,
    ) -> Result<PlanarOuterProfileEvidence, RefusedPlanarOuterProfile> {
        planar_outer_profile(self.part, face)
    }
}

/// The [`Features`] field (and [`features::Defining`] key) of a Python family.
fn field(family: &str) -> &str {
    match family {
        "passages" => "section_passages",
        other => other,
    }
}

fn sorted(faces: &[usize]) -> Vec<usize> {
    let set: BTreeSet<usize> = faces.iter().copied().collect();
    set.into_iter().collect()
}

/// Recognise *part* once and project the outcome onto its faces (`build_recognition_evidence`),
/// or the reconciliation's refusal.
pub fn build_recognition_evidence(
    part: &Part,
) -> Result<RecognitionEvidence<'_>, EvidenceViewError> {
    project(part, features::inventory(part)?)
}

/// Each physical family's candidates' defining and constituent faces (sorted), by Python family.
struct Faces {
    defining: BTreeMap<&'static str, Vec<Vec<usize>>>,
    constituent: BTreeMap<&'static str, Vec<Vec<usize>>>,
    /// The circular face patterns' repeated groups (each sorted, in order).
    groups: Vec<Vec<Vec<usize>>>,
}

/// Each candidate's defining and constituent faces (sorted), in order.
type Pairs = Vec<(Vec<usize>, Vec<usize>)>;

/// The candidates' faces. Where Python publishes wider constituent evidence, the port's
/// occurrences hold it as their consulted faces, so those families are found again here on a
/// fresh run context and each occurrence checked against the inventory's defining faces;
/// pockets' constituent faces are the reconciliation's evidence, and section passages' come
/// from their ring proposals (`proposal.constituent or proposal.nodes`). Every other family's
/// constituent faces are its defining faces, as Python's.
fn faces(part: &Part, inventory: &Inventory) -> Result<Faces, EvidenceViewError> {
    let mut defining = BTreeMap::new();
    for family in PHYSICAL_FAMILIES {
        let found: Vec<Vec<usize>> = match family {
            "section_recesses" | "step_levels" => Vec::new(),
            other => inventory
                .physical
                .defining
                .get(field(other))
                .ok_or_else(|| EvidenceViewError(format!("{other} has no defining evidence")))?
                .clone(),
        };
        defining.insert(family, found.iter().map(|f| sorted(f)).collect::<Vec<_>>());
    }
    let ctx = Context::new(part);
    let mut wider: BTreeMap<&'static str, Pairs> = BTreeMap::new();
    fn pairs<R>(found: Vec<features::evidence::Occurrence<R>>) -> Pairs {
        found
            .into_iter()
            .map(|o| {
                let both: Vec<usize> = o.defining.iter().chain(&o.context).copied().collect();
                (sorted(&o.defining), sorted(&both))
            })
            .collect()
    }
    let seats = countersinks::discover(&ctx);
    wider.insert("holes", pairs(holes::discover(&ctx, &seats)));
    wider.insert("bosses", pairs(bosses::discover(&ctx)));
    wider.insert("angled_steps", pairs(angled_steps::discover(&ctx)));
    wider.insert(
        "polygonal_bosses",
        pairs(polygonal_bosses::discover(&ctx, &Default::default())),
    );
    wider.insert("interior_voids", pairs(interior_voids::discover(&ctx)));
    wider.insert("thin_wall_bodies", pairs(thin_walls::discover(&ctx)));
    wider.insert(
        "prismatic_pockets",
        pairs(prismatic_pockets::discover(&ctx)),
    );
    wider.insert(
        "edge_open_circular_pockets",
        pairs(edge_open_circular::discover(&ctx)),
    );
    let patterns = circular_face_patterns::discover_with_groups(&ctx);
    let groups: Vec<Vec<Vec<usize>>> = patterns
        .iter()
        .map(|(_, groups)| groups.iter().map(|g| sorted(g)).collect())
        .collect();
    wider.insert(
        "circular_face_patterns",
        patterns
            .into_iter()
            .map(|(o, _)| o)
            .map(|o| {
                let faces = sorted(&o.defining);
                (faces.clone(), faces)
            })
            .collect(),
    );
    let pockets: Pairs = defining["pockets"]
        .iter()
        .cloned()
        .zip(
            inventory
                .evidence
                .pocket_constituent
                .iter()
                .map(|f| sorted(f)),
        )
        .collect();
    wider.insert("pockets", pockets);
    let proposals = if defining["passages"].is_empty() {
        Vec::new()
    } else {
        section_passages::section_ring_proposals(&ctx)
            .map_err(|e| EvidenceViewError(e.to_string()))?
    };
    let passages = defining["passages"]
        .iter()
        .map(|walls| {
            let proposal = proposals
                .iter()
                .find(|p| sorted(&p.nodes) == *walls)
                .ok_or_else(|| {
                    EvidenceViewError("section passage has no ring proposal".to_owned())
                })?;
            let constituent: Vec<usize> = if proposal.constituent.is_empty() {
                walls.clone()
            } else {
                proposal.constituent.iter().copied().collect()
            };
            Ok((walls.clone(), constituent))
        })
        .collect::<Result<Vec<_>, EvidenceViewError>>()?;
    wider.insert("passages", passages);

    let mut constituent = BTreeMap::new();
    for (&family, faces) in &defining {
        let Some(found) = wider.get(family) else {
            constituent.insert(family, faces.clone());
            continue;
        };
        if found.len() != faces.len() || found.iter().zip(faces).any(|((d, _), f)| d != f) {
            return Err(EvidenceViewError(format!(
                "{family} constituent evidence does not match its candidates"
            )));
        }
        constituent.insert(family, found.iter().map(|(_, c)| c.clone()).collect());
    }
    Ok(Faces {
        defining,
        constituent,
        groups,
    })
}

/// Project one completed inventory (`_project_recognition_evidence`).
fn project(
    part: &Part,
    inventory: Inventory,
) -> Result<RecognitionEvidence<'_>, EvidenceViewError> {
    let faces = faces(part, &inventory)?;
    let family_of = |family: Family| -> &'static str {
        PHYSICAL_FAMILIES
            .into_iter()
            .find(|f| *f == family.value())
            .expect("a reconciled family is physical")
    };
    let mut rejected: BTreeSet<(&'static str, usize)> = BTreeSet::new();
    let mut decided: BTreeMap<(&'static str, usize), &Disposition> = BTreeMap::new();
    for d in &inventory.dispositions {
        let key = (family_of(d.candidate.family), d.candidate.index);
        decided.insert(key, d);
        if d.outcome == Outcome::Rejected {
            rejected.insert(key);
        }
    }

    let mut out = Vec::new();
    for family in PHYSICAL_FAMILIES {
        if RECESS_SOURCE_FAMILIES.contains(&family) || GAPS.iter().any(|g| g.family == family) {
            continue;
        }
        let mut accepted = 0;
        for (i, defining) in faces.defining[family].iter().enumerate() {
            if rejected.contains(&(family, i)) {
                continue;
            }
            out.push(EvidenceFeature {
                family,
                index: accepted,
                defining: defining.clone(),
                constituent: faces.constituent[family][i].clone(),
                groups: match family {
                    "circular_face_patterns" => faces.groups[i].clone(),
                    _ => Vec::new(),
                },
                hosts: Vec::new(),
                members: Vec::new(),
            });
            accepted += 1;
        }
    }
    let accepted = inventory.clone().accepted();

    // Each hole pattern's members are accepted holes, found by their records: Python finds them
    // by identity; the port's patterns hold copies, so a member must equal exactly one hole.
    let hole_features: Vec<usize> = (0..out.len())
        .filter(|&p| out[p].family == "holes")
        .collect();
    if hole_features.len() != accepted.holes.len() {
        return Err(EvidenceViewError(
            "accepted holes do not match their evidence".to_owned(),
        ));
    }
    for (index, pattern) in accepted.hole_patterns.iter().enumerate() {
        let members: Vec<usize> = pattern_holes(pattern)
            .iter()
            .map(|member| {
                let mut equal = accepted
                    .holes
                    .iter()
                    .enumerate()
                    .filter(|(_, h)| *h == member);
                match (equal.next(), equal.next()) {
                    (Some((at, _)), None) => Ok(hole_features[at]),
                    _ => Err(EvidenceViewError(
                        "hole pattern member does not have one accepted evidence identity"
                            .to_owned(),
                    )),
                }
            })
            .collect::<Result<_, _>>()?;
        if members.iter().collect::<BTreeSet<_>>().len() != members.len() {
            return Err(EvidenceViewError(
                "hole pattern repeats an accepted evidence member".to_owned(),
            ));
        }
        let union = |pick: fn(&EvidenceFeature) -> &Vec<usize>| -> Vec<usize> {
            let all: Vec<usize> = members
                .iter()
                .flat_map(|&m| pick(&out[m]).clone())
                .collect();
            sorted(&all)
        };
        let feature = EvidenceFeature {
            family: "hole_patterns",
            index,
            defining: union(|f| &f.defining),
            constituent: union(|f| &f.constituent),
            groups: members
                .iter()
                .map(|&m| out[m].constituent.clone())
                .collect(),
            hosts: Vec::new(),
            members,
        };
        out.push(feature);
    }

    for feature in &mut out {
        feature.hosts = match feature.family {
            "holes" => {
                let hole = &accepted.holes[feature.index];
                let through = hole.bottom == holes::Bottom::Through;
                axial_hosts(
                    part,
                    &feature.constituent,
                    hole.axis,
                    hole.location,
                    |offsets| {
                        let lo = offsets.iter().copied().fold(f64::INFINITY, f64::min);
                        let hi = offsets.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                        if through { vec![lo, hi] } else { vec![lo] }
                    },
                )?
            }
            "bosses" => {
                let boss = &accepted.bosses[feature.index];
                let target = -boss.height;
                axial_hosts(
                    part,
                    &feature.constituent,
                    boss.axis,
                    boss.location,
                    |offsets| {
                        // The first offset nearest the target, as Python's `min` by key picks.
                        let mut best = offsets[0];
                        for &o in &offsets[1..] {
                            if (o - target).abs() < (best - target).abs() {
                                best = o;
                            }
                        }
                        vec![best]
                    },
                )?
            }
            _ => Vec::new(),
        };
    }

    // Candidates: every rejected one and, transitively, those it is related to; one without a
    // rule's decision is Python's default acceptance (no related candidates).
    let related_of = |key: &(&'static str, usize)| -> Vec<(&'static str, usize)> {
        decided.get(key).map_or_else(Vec::new, |d| {
            d.related
                .iter()
                .map(|r| (family_of(r.family), r.index))
                .collect()
        })
    };
    let mut chosen: BTreeSet<(&'static str, usize)> = rejected.clone();
    let mut pending: Vec<(&'static str, usize)> = chosen.iter().copied().collect();
    while let Some(key) = pending.pop() {
        for related in related_of(&key) {
            if chosen.insert(related) {
                pending.push(related);
            }
        }
    }
    let position = |family: &str| PHYSICAL_FAMILIES.iter().position(|f| *f == family);
    let mut ordered: Vec<(&'static str, usize)> = chosen.into_iter().collect();
    ordered.sort_by_key(|&(family, index)| (position(family), index));
    let at: BTreeMap<(&'static str, usize), usize> =
        ordered.iter().enumerate().map(|(i, k)| (*k, i)).collect();
    let candidates: Vec<EvidenceCandidate> = ordered
        .iter()
        .map(|key| {
            let (family, index) = *key;
            let disposition = decided.get(key);
            let mut related: Vec<usize> = related_of(key).iter().map(|r| at[r]).collect();
            related.sort_unstable();
            let faces_of = |map: &BTreeMap<&'static str, Vec<Vec<usize>>>| {
                map[family].get(index).cloned().ok_or_else(|| {
                    EvidenceViewError(format!("{family} candidate {index} has no evidence"))
                })
            };
            Ok(EvidenceCandidate {
                family,
                index,
                outcome: disposition.map_or(Outcome::Accepted, |d| d.outcome),
                reason: disposition.map(|d| d.reason),
                defining: faces_of(&faces.defining)?,
                constituent: faces_of(&faces.constituent)?,
                related,
            })
        })
        .collect::<Result<_, EvidenceViewError>>()?;
    let rejected_candidates = (0..candidates.len())
        .filter(|&i| candidates[i].outcome == Outcome::Rejected)
        .collect();
    Ok(RecognitionEvidence {
        part,
        result: accepted,
        features: out,
        candidates,
        rejected_candidates,
    })
}

fn pattern_holes(pattern: &features::hole_patterns::HolePattern) -> &[holes::HoleRecord] {
    use features::hole_patterns::HolePattern as P;
    match pattern {
        P::RectGrid { holes, .. }
        | P::RectangularHoleSet { holes, .. }
        | P::BoltCircle { holes, .. }
        | P::LinearArray { holes, .. } => holes,
    }
}

/// The planar faces normal to *axis* immediately outside *constituent* whose offsets along it
/// from *location* are among the *targets* chosen from all such offsets (`_axial_host_nodes`).
/// Offsets are read at each face's centroid, as Python's `Face.center()` reads a plane.
fn axial_hosts(
    part: &Part,
    constituent: &[usize],
    axis: geom::V3,
    location: geom::V3,
    targets: impl Fn(&[f64]) -> Vec<f64>,
) -> Result<Vec<usize>, EvidenceViewError> {
    let inside: BTreeSet<usize> = constituent.iter().copied().collect();
    let boundary: BTreeSet<usize> = constituent
        .iter()
        .flat_map(|&f| part.neighbours(f))
        .filter(|n| !inside.contains(n))
        .collect();
    let mut hosts = Vec::new();
    for face in boundary {
        let Surface::Plane { frame } = &part.faces[face].surface else {
            continue;
        };
        if geom::dot(frame.z, axis).abs() >= 1.0 - 1e-6 {
            let centroid = part
                .face_moments(face)
                .ok_or_else(|| EvidenceViewError(format!("face {face} has no centroid")))?
                .centroid;
            hosts.push((face, geom::dot(geom::sub(centroid, location), axis)));
        }
    }
    if hosts.is_empty() {
        return Ok(Vec::new());
    }
    let offsets: Vec<f64> = hosts.iter().map(|(_, o)| *o).collect();
    let chosen = targets(&offsets);
    let scale = offsets.iter().fold(0.0_f64, |m, o| m.max(o.abs()));
    let tolerance = policy::length_tol(scale, 1e-9);
    Ok(hosts
        .into_iter()
        .filter(|(_, o)| chosen.iter().any(|t| (o - t).abs() <= tolerance))
        .map(|(f, _)| f)
        .collect())
}
