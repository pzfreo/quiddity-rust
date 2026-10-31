//! Draftwright `dims/levels.rs` step-ladder and shoulder ownership, without views or labels.
//! Through-step sections that need their own leg grammar and plate thicknesses remain explicit
//! later-slice owners; they must not become anonymous Cartesian plane locations.
use super::*;
use crate::features::{Features, levels as evidence};
use std::collections::BTreeSet;

const NEAR: f64 = 0.5;
fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < NEAR
}
fn station(v: f64) -> i64 {
    (v * 1000.).round() as i64
}

/// Name the inferred orthogonal directions by their nearest file axes, signed towards those
/// axes. This is a geometric frame convention, not a drawing-view choice. For a strongly rolled
/// frame whose nearest directions would reflect it, retain the recogniser's proper frame.
pub(super) fn planning_frame(f: crate::frames::PartFrame) -> crate::frames::PartFrame {
    let axes = [f.x, f.y, f.z];
    let permutations = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    let p = permutations
        .into_iter()
        .max_by(|a, b| {
            let score = |p: &[usize; 3]| (0..3).map(|k| axes[p[k]][k].abs()).sum::<f64>();
            score(a).total_cmp(&score(b)).then(a.cmp(b))
        })
        .unwrap();
    let rows: [Point; 3] = std::array::from_fn(|k| axes[p[k]].map(|v| v * axes[p[k]][k].signum()));
    let [a, b, c] = rows;
    let det = a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
        + a[2] * (b[0] * c[1] - b[1] * c[0]);
    if det > 0. {
        crate::frames::PartFrame {
            x: a,
            y: b,
            z: c,
            ..f
        }
    } else {
        f
    }
}

struct Through {
    faces: Vec<usize>,
    run: usize,
    section: [Point; 3],
    solid: bool,
}
impl Through {
    fn legs(&self) -> Vec<(usize, f64, f64)> {
        self.section
            .windows(2)
            .filter_map(|w| {
                let changed: Vec<_> = (0..3)
                    .filter(|&k| (w[0][k] - w[1][k]).abs() > 1e-6)
                    .collect();
                (changed.len() == 1).then(|| {
                    let k = changed[0];
                    (k, w[0][k].min(w[1][k]), w[0][k].max(w[1][k]))
                })
            })
            .collect()
    }
}
struct Plate {
    axis: usize,
    lo: f64,
    hi: f64,
    faces: Vec<usize>,
}
fn project(
    risers: &[(evidence::RiserEvidence, Vec<usize>)],
    zs: &[f64],
) -> Vec<(usize, f64, Vec<usize>)> {
    let mut out = vec![];
    for (r, faces) in risers {
        let on = |z: f64| {
            r.body_levels
                .iter()
                .any(|l| (z - l.z).abs() < r.tol && zs.iter().any(|s| (s - l.z).abs() < r.tol))
        };
        let tied = if r.vertical {
            on(r.z_lo)
        } else {
            (r.lo_at_envelope || on(r.z_lo)) && (r.hi_at_envelope || on(r.z_hi))
        };
        if tied {
            for (letter, positions) in [(r.axis, &r.positions), (r.other_axis, &r.other_positions)]
            {
                if let Some(k) = ['x', 'y'].iter().position(|&a| a == letter) {
                    out.extend(positions.iter().map(|p| (k, **p, faces.clone())));
                }
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.cmp(&b.2)));
    out
}
fn owners(
    s: &Through,
    lo: Point,
    hi: Point,
    zs: &[f64],
    shoulders: &[(usize, f64, Vec<usize>)],
    plates: &[&Plate],
) -> bool {
    let mut spans: Vec<_> = zs
        .iter()
        .flat_map(|&z| [(2, lo[2], z), (2, z, hi[2])])
        .collect();
    spans.extend(
        shoulders
            .iter()
            .flat_map(|&(k, at, _)| [(k, lo[k], at), (k, at, hi[k])]),
    );
    let own: Vec<_> = plates
        .iter()
        .filter(|p| p.faces.iter().any(|f| s.faces.contains(f)))
        .collect();
    for p in &own {
        spans.push((p.axis, p.lo, p.hi));
        if near(p.lo, lo[p.axis]) {
            spans.push((p.axis, p.hi, hi[p.axis]));
        }
        if near(p.hi, hi[p.axis]) {
            spans.push((p.axis, lo[p.axis], p.lo));
        }
    }
    s.legs().iter().all(|&(k, a, b)| {
        let fitting = own
            .iter()
            .filter(|p| {
                p.axis == k
                    && ((near(a, p.lo) && near(b, p.hi))
                        || (near(p.lo, lo[k]) && near(a, p.hi) && near(b, hi[k]))
                        || (near(p.hi, hi[k]) && near(a, lo[k]) && near(b, p.lo)))
            })
            .count();
        fitting <= 1
            && spans
                .iter()
                .any(|&(ik, ia, ib)| ik == k && near(ia, a) && near(ib, b))
    })
}
fn own_levels(steps: &[&Through], base: f64) -> Vec<f64> {
    steps
        .iter()
        .filter(|s| s.run != 2)
        .filter_map(|s| {
            let z = s.section[1][2];
            (s.solid
                || s.legs()
                    .iter()
                    .any(|&(k, a, b)| k == 2 && ((a - base).abs() < 1e-6 && (b - z).abs() < 1e-6)))
            .then_some(z)
        })
        .collect()
}
fn own_sites(steps: &[&Through], lo: Point, hi: Point) -> Vec<(usize, f64)> {
    steps
        .iter()
        .flat_map(|s| s.legs())
        .filter_map(|(k, a, b)| {
            if k == 2 {
                None
            } else if near(a, lo[k]) {
                Some((k, b))
            } else if near(b, hi[k]) {
                Some((k, a))
            } else {
                None
            }
        })
        .collect()
}
fn deferred(family: &str, faces: &[usize]) -> Deferred {
    let mut faces = faces.to_vec();
    faces.sort_unstable();
    faces.dedup();
    Deferred {
        id: format!("deferred:{}", super::recognition::face_id(family, &faces)),
        faces,
        family: family.into(),
    }
}

pub(super) fn recognise(
    part: &crate::Part,
    found: &Features,
    planes: &[Plane],
    constituents: &std::collections::BTreeMap<String, Vec<usize>>,
) -> (StepLevels, bool, Vec<Deferred>) {
    let b = part.bounds();
    let (lo, hi) = (b.min, b.max);
    // Every accepted later-slice family keeps an explicit face-anchored deferral. The aggregate
    // may expose several descriptions of one face; preserve these claims rather than guess.
    let in_scope = |family: &str| {
        matches!(
            family,
            "holes" | "countersinks" | "step_levels" | "risers" | "through_steps" | "plates"
        )
    };
    let mut deferreds: Vec<_> = found
        .defining
        .iter()
        .filter(|(family, _)| !in_scope(family))
        .flat_map(|(family, records)| records.iter().map(|faces| deferred(family, faces)))
        .collect();
    for d in &mut deferreds {
        if (d.family.contains("pocket") || d.family.contains("recess"))
            && let Some(faces) = constituents.get(d.id.trim_start_matches("deferred:"))
        {
            d.faces = faces.clone();
        }
    }
    for r in &found.section_recesses {
        let id = deferred("section_recesses", r.evidence().defining_faces()).id;
        if let Some(d) = deferreds.iter_mut().find(|d| d.id == id) {
            d.faces = r.evidence().constituent_faces().to_vec();
        }
    }
    let owned: BTreeSet<_> = deferreds
        .iter()
        .flat_map(|d| d.faces.iter().copied())
        .chain(
            ["holes", "countersinks"]
                .into_iter()
                .flat_map(|family| found.defining[family].iter().flatten().copied()),
        )
        .collect();
    let eligible = |faces: &[usize]| faces.iter().any(|f| !owned.contains(f));
    let plates: Vec<_> = found
        .plates
        .iter()
        .zip(&found.defining["plates"])
        .filter_map(|(p, faces)| {
            Some(Plate {
                axis: ['x', 'y', 'z'].iter().position(|&a| a == p.axis)?,
                lo: *p.lo,
                hi: *p.hi,
                faces: faces.clone(),
            })
        })
        .collect();
    let profiles = !found.turned_steps.is_empty();
    let turned = !found.turned_bodies.is_empty() || {
        let mut bands = found.turned_steps.iter();
        bands.next().is_some_and(|first| {
            let axis = ['x', 'y', 'z'].iter().position(|&a| a == first.axis);
            axis.is_some_and(|k| {
                let od = found
                    .turned_steps
                    .iter()
                    .map(|s| s.diameter)
                    .fold(0., f64::max);
                (0..3)
                    .filter(|&j| j != k)
                    .all(|j| ((hi[j] - lo[j]) - od).abs() < 1e-3 * od.max(1.))
            })
        })
    };
    let stock = !found.polygonal_stock.is_empty();
    let cartesian = !turned && !stock;
    if !cartesian {
        for axis in 0..3 {
            deferreds.push(Deferred {
                id: format!("deferred:envelope:{axis}"),
                faces: vec![],
                family: if stock {
                    "polygonal_stock"
                } else {
                    "turned envelope"
                }
                .into(),
            });
        }
    }
    let active_plates: Vec<_> = if !profiles
        && !turned
        && plates.iter().map(|p| p.axis).collect::<BTreeSet<_>>().len() >= 2
    {
        plates.iter().collect()
    } else {
        vec![]
    };
    let base_tops: Vec<_> = active_plates
        .iter()
        .filter(|p| p.axis == 2 && near(p.lo, lo[2]))
        .map(|p| p.hi)
        .collect();
    let opts = crate::FaceLevelOptions {
        tol: Some(0.5),
        min_area_frac: 0.01,
    };
    let levels = evidence::face_levels_with_faces(part, &opts);
    let mut zs: Vec<_> = levels
        .iter()
        .filter(|(l, faces)| {
            let b = faces
                .first()
                .and_then(|&f| part.faces[f].solid)
                .map_or_else(|| part.bounds(), |s| part.solid_bounds(s));
            let margin = evidence::bounded_end_margin(b.max[2] - b.min[2]);
            !profiles
                && l.z > b.min[2] + margin
                && l.z < b.max[2] - margin
                && eligible(faces)
                && !base_tops.iter().any(|&z| near(z, l.z))
        })
        .map(|(l, _)| l.z)
        .collect();
    zs.sort_by(f64::total_cmp);
    zs.dedup_by_key(|z| station(*z - lo[2]));
    let risers: Vec<_> = evidence::risers_with_faces(part, &evidence::RiserOptions::default())
        .into_iter()
        .filter(|(_, faces)| eligible(faces))
        .collect();
    let through: Vec<_> = found
        .through_steps
        .iter()
        .zip(&found.defining["through_steps"])
        .filter_map(|(s, faces)| {
            let run = ['x', 'y', 'z'].iter().position(|&a| a == s.axis)?;
            let across: Vec<_> = (0..3).filter(|&k| k != run).collect();
            let section = s.section.map(|p| {
                let mut at = *s.at;
                at[across[0]] = p[0];
                at[across[1]] = p[1];
                at
            });
            Some(Through {
                run,
                section,
                solid: s.endpoint_scopes == ["solid", "solid"],
                faces: faces.clone(),
            })
        })
        .collect();
    let shoulders = project(&risers, &zs);
    let (mut lowered, mut rest): (Vec<_>, Vec<_>) = through
        .iter()
        .partition(|s| s.run == 2 || !owners(s, lo, hi, &zs, &shoulders, &active_plates));
    loop {
        let owned_zs = own_levels(&lowered, lo[2]);
        let sites = own_sites(&lowered, lo, hi);
        let remaining: Vec<_> = zs
            .iter()
            .copied()
            .filter(|z| !owned_zs.iter().any(|&o| near(o, *z)))
            .collect();
        let shoulders: Vec<_> = project(&risers, &remaining)
            .into_iter()
            .filter(|(k, p, _)| !sites.iter().any(|(sk, s)| k == sk && near(*p, *s)))
            .collect();
        let kept: Vec<_> = active_plates
            .iter()
            .copied()
            .filter(|p| {
                !lowered.iter().flat_map(|s| s.legs()).any(|(k, a, b)| {
                    k == p.axis && (a - p.lo).abs() < 1e-6 && (b - p.hi).abs() < 1e-6
                })
            })
            .collect();
        let (promoted, still): (Vec<_>, Vec<_>) = rest
            .into_iter()
            .partition(|s| !owners(s, lo, hi, &remaining, &shoulders, &kept));
        rest = still;
        if promoted.is_empty() {
            zs = remaining;
            break;
        }
        lowered.extend(promoted);
    }
    for s in &lowered {
        deferreds.push(deferred("through_step section", &s.faces));
    }
    for p in &active_plates {
        deferreds.push(deferred("plate thickness", &p.faces));
    }
    let height_support = |z: f64| {
        let faces: Vec<_> = levels
            .iter()
            .filter(|(l, faces)| station(l.z) == station(z) && eligible(faces))
            .flat_map(|(_, faces)| faces.iter().copied().filter(|f| !owned.contains(f)))
            .collect();
        let body = faces.first().and_then(|&f| part.faces[f].solid);
        Support {
            feature: super::recognition::face_id("step", &faces),
            faces,
            body,
            at: [0., 0., z],
            plane: true,
        }
    };
    let heights = zs.iter().map(|&z| height_support(z)).collect();
    let sites = own_sites(&lowered, lo, hi);
    let mut shoulders: Vec<Plane> = vec![];
    for (axis, at, faces) in project(&risers, &zs) {
        if sites.iter().any(|&(k, s)| k == axis && near(s, at)) || (at - lo[axis]).abs() < 1e-6 {
            continue;
        }
        if let Some(p) = shoulders
            .iter_mut()
            .find(|p| p.axis == axis && station(p.support.at[axis]) == station(at))
        {
            p.support.faces.extend(faces);
            continue;
        }
        let body = faces.first().and_then(|&f| part.faces[f].solid);
        let mut point = [0.; 3];
        point[axis] = at;
        shoulders.push(Plane {
            support: Support {
                feature: String::new(),
                faces,
                body,
                at: point,
                plane: true,
            },
            axis,
            body: body.unwrap_or(usize::MAX),
        });
    }
    for p in &mut shoulders {
        p.support.faces.sort_unstable();
        p.support.faces.dedup();
        p.support.feature = format!(
            "{}:axis:{}:station:{}",
            super::recognition::face_id("shoulder", &p.support.faces),
            p.axis,
            station(p.support.at[p.axis])
        );
    }
    let steps = StepLevels {
        axis: 2,
        heights,
        shoulders,
    };
    // Interior principal planes that are neither a height nor a tied shoulder do not acquire a
    // raw interval. Report their later-slice ownership even when no named family recognised it.
    for p in planes {
        let at = p.support.at[p.axis];
        if near(at, lo[p.axis])
            || near(at, hi[p.axis])
            || p.support.faces.iter().all(|f| owned.contains(f))
        {
            continue;
        }
        if steps
            .heights
            .iter()
            .any(|s| p.axis == 2 && station(s.at[2]) == station(at))
            || steps
                .shoulders
                .iter()
                .any(|s| s.axis == p.axis && station(s.support.at[p.axis]) == station(at))
        {
            continue;
        }
        let family = if profiles {
            "turned profile"
        } else if p
            .support
            .faces
            .iter()
            .any(|f| active_plates.iter().any(|p| p.faces.contains(f)))
        {
            "plate thickness"
        } else if p
            .support
            .faces
            .iter()
            .any(|f| lowered.iter().any(|s| s.faces.contains(f)))
        {
            "through_step section"
        } else {
            "non-step plane"
        };
        deferreds.push(deferred(family, &p.support.faces));
    }
    deferreds.sort_by(|a, b| a.id.cmp(&b.id));
    deferreds.dedup_by(|a, b| a.id == b.id);
    (steps, cartesian, deferreds)
}

fn envelope(input: &Input, axis: usize, end: usize) -> Support {
    input
        .planes
        .iter()
        .filter(|p| p.axis == axis && (p.support.at[axis] - input.bounds[end][axis]).abs() < 1e-4)
        .min_by(|a, b| a.support.feature.cmp(&b.support.feature))
        .map(|p| p.support.clone())
        .unwrap_or_else(|| Support {
            feature: format!("envelope:{axis}:{end}"),
            faces: vec![],
            body: None,
            at: input.bounds[end],
            plane: true,
        })
}
pub(super) fn requirements(input: &Input) -> Vec<Required> {
    let mut out = vec![];
    if input.cartesian_envelope {
        for axis in 0..3 {
            let (a, b) = (envelope(input, axis, 0), envelope(input, axis, 1));
            if b.at[axis] - a.at[axis] > 1e-6 {
                let mut r = super::rules::location(&a, &b, axis, None, input);
                r.covers = vec![a.feature, b.feature];
                out.push(r);
            }
        }
    }
    let axis = input.steps.axis;
    let mut heights = input.steps.heights.clone();
    heights.sort_by(|a, b| {
        a.at[axis]
            .total_cmp(&b.at[axis])
            .then(a.feature.cmp(&b.feature))
    });
    let base = envelope(input, axis, 0);
    let rises: Vec<_> = heights
        .iter()
        .scan(base.at[axis], |last, s| {
            let d = s.at[axis] - *last;
            *last = s.at[axis];
            Some(d)
        })
        .collect();
    let mean = rises.iter().sum::<f64>() / rises.len().max(1) as f64;
    let repeat =
        heights.len() >= 3 && mean > 0. && rises.iter().all(|r| ((r - mean) / mean).abs() <= 0.10);
    if repeat {
        let mut r = super::rules::location(&base, &heights[0], axis, None, input);
        r.nominal = mean;
        r.count = heights.len()
            + usize::from(
                ((input.bounds[1][axis] - heights.last().unwrap().at[axis] - mean) / mean).abs()
                    <= 0.10,
            );
        r.covers = heights.iter().map(|s| s.feature.clone()).collect();
        out.push(r);
    } else {
        for s in &heights {
            let r = super::rules::location(&base, s, axis, None, input);
            if r.nominal > 1e-6 {
                out.push(r);
            }
        }
    }
    for p in &input.steps.shoulders {
        let r =
            super::rules::location(&envelope(input, p.axis, 0), &p.support, p.axis, None, input);
        if r.nominal > 1e-6 {
            out.push(r);
        }
    }
    out
}
