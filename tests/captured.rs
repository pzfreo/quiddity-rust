//! Rust snapshots of all legacy recognition call inputs, including record-only calls.
use super::common;
use quiddity::{Part, read_step_file};
use serde_json::{Value, json};
use std::collections::BTreeMap;
fn replay(function: &str) {
    let calls = common::load("captured/calls.json");
    let mut parts: BTreeMap<String, Part> = BTreeMap::new();
    let actual: Vec<Value> = calls["calls"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["function"] == function)
        .map(|c| {
            let result = match c["file"].as_str() {
                Some(file) => {
                    let part = parts.entry(file.into()).or_insert_with(|| {
                        read_step_file(&common::fixtures().join("captured").join(file)).unwrap()
                    });
                    common::recognise(function, part, &c["options"])
                }
                None => common::recognise_records(function, &c["arguments"]),
            };
            json!({"test":c["test"], "file":c["file"], "options":c["options"], "result":result})
        })
        .collect();
    assert!(!actual.is_empty());
    common::snapshot::check(
        &common::fixtures(),
        &format!("calls/{function}"),
        &json!(actual),
    );
}
#[test]
fn rust_snapshot_fillets() {
    replay("recognise_fillets");
}
#[test]
fn rust_snapshot_countersinks() {
    replay("recognise_countersinks");
}
#[test]
fn rust_snapshot_holes() {
    replay("recognise_holes");
}
#[test]
fn rust_snapshot_hole_patterns() {
    replay("recognise_hole_patterns");
}
#[test]
fn rust_snapshot_chamfers() {
    replay("recognise_chamfers");
}
#[test]
fn rust_snapshot_bosses() {
    replay("recognise_bosses");
}
#[test]
fn rust_snapshot_angled_steps() {
    replay("recognise_angled_steps");
}
#[test]
fn rust_snapshot_flats() {
    replay("recognise_flats");
}
#[test]
fn rust_snapshot_paired_ramp_steps() {
    replay("recognise_paired_ramp_steps");
}
#[test]
fn rust_snapshot_oriented_chamfers() {
    replay("recognise_oriented_chamfers");
}
#[test]
fn rust_snapshot_circular_face_patterns() {
    replay("recognise_circular_face_patterns");
}
#[test]
fn rust_snapshot_face_levels() {
    replay("recognise_face_levels");
}
#[test]
fn rust_snapshot_risers() {
    replay("recognise_risers");
}
#[test]
fn rust_snapshot_thin_wall_bodies() {
    replay("recognise_thin_wall_bodies");
}
#[test]
fn rust_snapshot_interior_voids() {
    replay("recognise_interior_voids");
}
#[test]
fn rust_snapshot_through_steps() {
    replay("recognise_through_steps");
}
#[test]
fn rust_snapshot_oblique_through_steps() {
    replay("recognise_oblique_through_steps");
}
#[test]
fn rust_snapshot_circular_blind_steps() {
    replay("recognise_circular_blind_steps");
}
#[test]
fn rust_snapshot_turned_steps() {
    replay("recognise_turned_steps");
}
#[test]
fn rust_snapshot_round_bottom_blind_slots() {
    replay("recognise_round_bottom_blind_slots");
}
#[test]
fn rust_snapshot_rectangular_blind_slots() {
    replay("recognise_rectangular_blind_slots");
}
#[test]
fn rust_snapshot_gusset_ribs() {
    replay("recognise_gusset_ribs");
}
#[test]
fn rust_snapshot_double_d_bores() {
    replay("recognise_double_d_bores");
}
#[test]
fn rust_snapshot_gusset_rib_patterns() {
    replay("recognise_gusset_rib_patterns");
}
#[test]
fn rust_snapshot_grooves() {
    replay("recognise_grooves");
}
#[test]
fn rust_snapshot_plates() {
    replay("recognise_plates");
}
#[test]
fn rust_snapshot_edge_open_circular_pockets() {
    replay("recognise_edge_open_circular_pockets");
}
#[test]
fn rust_snapshot_edge_open_prismatic_recesses() {
    replay("recognise_edge_open_prismatic_recesses");
}
#[test]
fn rust_snapshot_blends() {
    replay("recognise_blends");
}
#[test]
fn rust_snapshot_sheet_metal_bodies() {
    replay("recognise_sheet_metal_bodies");
}
#[test]
fn rust_snapshot_slots() {
    replay("recognise_slots");
}
#[test]
fn rust_snapshot_pockets() {
    replay("recognise_pockets");
}
#[test]
fn rust_snapshot_channels() {
    replay("recognise_channels");
}
#[test]
fn rust_snapshot_slot_patterns() {
    replay("recognise_slot_patterns");
}
#[test]
fn rust_snapshot_pocket_patterns() {
    replay("recognise_pocket_patterns");
}
#[test]
fn rust_snapshot_repeating_radial_profiles() {
    replay("recognise_repeating_radial_profiles");
}
#[test]
fn rust_snapshot_freeform_surfaces() {
    replay("recognise_freeform_surfaces");
}
#[test]
fn rust_snapshot_polygonal_bosses() {
    replay("recognise_polygonal_bosses");
}
#[test]
fn rust_snapshot_polygonal_stock() {
    replay("recognise_polygonal_stock");
}
#[test]
fn rust_snapshot_prismatic_pockets() {
    replay("recognise_prismatic_pockets");
}
#[test]
fn rust_snapshot_oriented_slots() {
    replay("recognise_oriented_slots");
}
#[test]
fn rust_snapshot_oriented_slot_patterns() {
    replay("recognise_oriented_slot_patterns");
}
#[test]
fn rust_snapshot_rectangular_pads() {
    replay("recognise_rectangular_pads");
}
