use super::*;
use crate::features::{hole_patterns::HolePattern, holes::Bottom};
use crate::kernel::geom::Surface;
use std::collections::BTreeSet;

pub(super) fn face_id(role: &str, faces: &[usize]) -> String {
    let mut faces = faces.to_vec();
    faces.sort_unstable();
    faces.dedup();
    format!(
        "{role}:{}",
        faces
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(".")
    )
}

impl Input {
    /// Recognise once in the inferred part frame, preserving caller face numbers.
    /// Refuses absent/invalid frames instead of silently interpreting world axes as part axes.
    /// Datum faces use caller face indices. Their planes are measured in the inferred frame.
    pub fn from_recognition(
        part: &crate::Part,
        datums: Vec<Datum>,
        policy: Policy,
    ) -> Result<Self, String> {
        let frame = crate::frames::infer_part_frame(part)
            .map_err(|r| format!("dimensioning frame: {}", r.as_str()))?;
        let local = part
            .transformed(&frame.normalisation())
            .map_err(|e| format!("dimensioning frame transform: {e:?}"))?;
        let found = crate::features::recognise(&local);
        let bounds = local.bounds();
        let bounds = [bounds.min, bounds.max];
        let holes: Vec<_> = found
            .holes
            .iter()
            .zip(found.defining.get("holes").map_or(&[][..], Vec::as_slice))
            .map(|(h, faces)| {
                let mut details = Vec::new();
                for (role, r) in [
                    ("counterbore".to_owned(), h.cbore.as_ref()),
                    ("spotface".to_owned(), h.spotface.as_ref()),
                ]
                .into_iter()
                .chain(
                    h.outer_steps
                        .iter()
                        .enumerate()
                        .map(|(i, r)| (format!("outer_steps.{i}"), Some(r))),
                ) {
                    if let Some(r) = r {
                        details.push(Size {
                            role: format!("{role}.diameter"),
                            nominal: r.diameter,
                            kind: SizeKind::Diameter,
                        });
                        details.push(Size {
                            role: format!("{role}.depth"),
                            nominal: r.depth,
                            kind: SizeKind::Length,
                        });
                    }
                }
                if let Some(c) = &h.csink {
                    details.push(Size {
                        role: "countersink.diameter".into(),
                        nominal: c.major_diameter,
                        kind: SizeKind::Diameter,
                    });
                    details.push(Size {
                        role: "countersink.angle".into(),
                        nominal: c.included_angle,
                        kind: SizeKind::Angle,
                    });
                }
                Hole {
                    support: Support {
                        feature: face_id("hole", faces),
                        faces: faces.clone(),
                        body: faces.first().and_then(|&f| local.faces[f].solid),
                        at: *h.location,
                        plane: false,
                    },
                    direction: h.axis,
                    diameter: h.diameter,
                    depth: (h.bottom != Bottom::Through).then_some(h.depth),
                    traversal: (h.bottom == Bottom::Through).then_some(h.depth),
                    bottom: h.bottom,
                    details,
                }
            })
            .collect();
        let patterns = found
            .hole_patterns
            .iter()
            .filter_map(|p| {
                let (members, arrangement) = match p {
                    HolePattern::RectGrid { holes, .. }
                    | HolePattern::RectangularHoleSet { holes, .. } => (holes, Arrangement::Grid),
                    HolePattern::LinearArray { holes, .. } => (holes, Arrangement::Line),
                    HolePattern::BoltCircle {
                        holes,
                        center,
                        diameter,
                    } => (
                        holes,
                        Arrangement::BoltCircle {
                            centre: *center,
                            diameter: *diameter,
                        },
                    ),
                };
                let members = members
                    .iter()
                    .map(|m| {
                        found
                            .holes
                            .iter()
                            .position(|h| h == m)
                            .map(|i| holes[i].support.feature.clone())
                    })
                    .collect::<Option<Vec<_>>>()?;
                Some(Pattern {
                    members,
                    arrangement,
                })
            })
            .collect();
        // A hole/recess/boss owns its local floor or cap, not the generic step ladder.
        // Envelope planes always survive. Through-step and plate supports remain ladder input.
        let excluded: BTreeSet<_> = found
            .defining
            .iter()
            .filter(|(family, _)| {
                matches!(
                    **family,
                    "holes"
                        | "bosses"
                        | "slots"
                        | "oriented_slots"
                        | "section_recesses"
                        | "prismatic_pockets"
                        | "pads"
                        | "cylindrical_pockets"
                        | "turned_steps"
                        | "turned_bodies"
                )
            })
            .flat_map(|(_, records)| records.iter().flatten().copied())
            .collect();
        // Use recognition's support-plane proposal and area/end bands on each axis,
        // rather than treating every small planar facet as a manufacturing step.
        let mut level_faces = BTreeSet::new();
        for axis in 0..3 {
            let axes = [(axis + 1) % 3, (axis + 2) % 3, axis];
            let rotation = axes.map(|k| {
                let mut row = [0.; 4];
                row[k] = 1.;
                row
            });
            let rotated = local
                .transformed(&rotation)
                .map_err(|e| format!("dimensioning level frame: {e:?}"))?;
            let opts = crate::FaceLevelOptions {
                tol: Some(0.01),
                min_area_frac: 0.01,
            };
            for (level, faces) in crate::features::levels::face_levels_with_faces(&rotated, &opts) {
                let bounds = faces
                    .first()
                    .and_then(|&f| rotated.faces[f].solid)
                    .map_or_else(|| rotated.bounds(), |body| rotated.solid_bounds(body));
                let margin =
                    crate::features::levels::bounded_end_margin(bounds.max[2] - bounds.min[2]);
                if level.z > bounds.min[2] + margin && level.z < bounds.max[2] - margin {
                    level_faces.extend(faces);
                }
            }
        }
        let mut planes: Vec<Plane> = Vec::new();
        for (i, face) in local.faces.iter().enumerate() {
            let Surface::Plane { frame: plane } = &face.surface else {
                continue;
            };
            let Some(axis) = super::rules::principal(plane.z) else {
                continue;
            };
            let at = plane.origin[axis];
            let outer = bounds.iter().any(|p| (at - p[axis]).abs() < 1e-4);
            let datum = datums.iter().any(|d| d.faces.contains(&i));
            if !outer && !datum && (excluded.contains(&i) || !level_faces.contains(&i)) {
                continue;
            }
            let body = face.solid.unwrap_or(usize::MAX);
            if let Some(p) = planes.iter_mut().find(|p| {
                p.body == body && p.axis == axis && (p.support.at[axis] - at).abs() < 1e-6
            }) {
                p.support.faces.push(i);
            } else {
                let mut point = [0.; 3];
                point[axis] = at;
                planes.push(Plane {
                    support: Support {
                        feature: String::new(),
                        faces: vec![i],
                        body: face.solid,
                        at: point,
                        plane: true,
                    },
                    axis,
                    body,
                });
            }
        }
        for p in &mut planes {
            p.support.feature = face_id("plane", &p.support.faces);
        }
        let round_axes = found.bosses.iter().map(|b| (b.location, b.axis)).collect();
        Ok(Self {
            frame,
            bounds,
            holes,
            patterns,
            planes,
            datums,
            round_axes,
            specified: vec![],
            policy,
        })
    }
}
