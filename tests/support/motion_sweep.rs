//! Shared poses and occurrence census for the invariance suite and read-only sweep example.
use super::motion_breakdown::Occurrence;
use quiddity::kernel::step::Placement;
use quiddity::{Part, correspondence, features};

/// The rotation by *degrees* about *axis* (Rodrigues), then *translation*.
pub fn rotation(axis: [f64; 3], degrees: f64, translation: [f64; 3]) -> Placement {
    let n = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
    let [x, y, z] = axis.map(|c| c / n);
    let (s, c) = degrees.to_radians().sin_cos();
    let t = 1.0 - c;
    [
        [
            t * x * x + c,
            t * x * y - s * z,
            t * x * z + s * y,
            translation[0],
        ],
        [
            t * x * y + s * z,
            t * y * y + c,
            t * y * z - s * x,
            translation[1],
        ],
        [
            t * x * z - s * y,
            t * y * z + s * x,
            t * z * z + c,
            translation[2],
        ],
    ]
}

/// Compose placements: apply `b` first, then `a`, including both translations.
pub fn compose(a: Placement, b: Placement) -> Placement {
    [0, 1, 2].map(|i| {
        [0, 1, 2, 3].map(|j| {
            (0..3).map(|k| a[i][k] * b[k][j]).sum::<f64>() + if j == 3 { a[i][3] } else { 0. }
        })
    })
}

/// The reference sweep's census bindings, including its distinct-step compatibility projection.
pub const SWEEP_FAMILIES: [(&str, &str); 28] = [
    ("hole", "holes"),
    ("hole_pattern", "hole_patterns"),
    ("boss", "bosses"),
    ("step", "turned_steps"),
    ("groove", "grooves"),
    ("flat", "flats"),
    ("slot", "slots"),
    ("oriented_slot", "oriented_slots"),
    ("rectangular_blind_slot", "rectangular_blind_slots"),
    ("round_bottom_blind_slot", "round_bottom_blind_slots"),
    ("channel", "channels"),
    ("pocket", "pockets"),
    ("prismatic_pocket", "prismatic_pockets"),
    ("edge_open_circular_pocket", "edge_open_circular_pockets"),
    ("edge_open_prismatic_recess", "edge_open_prismatic_recesses"),
    ("passage", "passages"),
    ("chamfer", "chamfers"),
    ("oriented_chamfer", "oriented_chamfers"),
    ("angled_step", "angled_steps"),
    ("gusset_rib", "gusset_ribs"),
    ("paired_ramp_step", "paired_ramp_steps"),
    ("through_step", "through_steps"),
    ("oblique_through_step", "oblique_through_steps"),
    ("circular_blind_step", "circular_blind_steps"),
    ("blend", "blends"),
    ("fillet", "fillets"),
    ("countersink", "countersinks"),
    ("plate", "plates"),
];

pub fn occurrences(part: &Part) -> Vec<Occurrence> {
    use features::reconcile::ReasonCode;
    let inventory = features::inventory(part).unwrap();
    let compatible: Vec<_> = inventory
        .dispositions
        .iter()
        .filter(|d| d.reason == ReasonCode::TurnedStepGrooveCompatible)
        .map(|d| {
            let mut f = d.candidate.defining.clone();
            f.sort_unstable();
            f
        })
        .collect();
    let accepted = inventory.accepted();
    let prints = correspondence::fingerprint(part, &accepted);
    SWEEP_FAMILIES
        .iter()
        .flat_map(|&(family, field)| {
            let compatible = &compatible;
            prints
                .features
                .iter()
                .filter(move |f| f.family == field)
                .filter(move |f| field != "turned_steps" || !compatible.contains(&f.faces))
                .map(move |f| Occurrence {
                    family: family.into(),
                    faces: f.faces.clone(),
                })
        })
        .collect()
}
