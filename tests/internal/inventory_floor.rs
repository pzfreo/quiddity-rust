//! Recognition inventories at 5fb1706: motion stability alone cannot detect a lost feature.
use super::common;
use quiddity::{correspondence, features, read_step_file};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// #159 preserves the historical per-torus-patch inventory even when several
/// interrupted patches now share one callout. Cylinder rounds (including their
/// spherical caps) still count once. No baseline or loss allowance is lowered.
pub(super) fn fillet_patch_count(part: &quiddity::Part, defining: &[Vec<usize>]) -> usize {
    defining
        .iter()
        .map(|faces| {
            faces
                .iter()
                .filter(|&&f| {
                    matches!(
                        part.faces[f].surface,
                        quiddity::kernel::geom::Surface::Torus { .. }
                    )
                })
                .count()
                .max(1)
        })
        .sum()
}

fn counts(part: &quiddity::Part, f: &features::Features) -> BTreeMap<String, usize> {
    let mut counts: BTreeMap<String, usize> = serde_json::to_value(f)
        .unwrap()
        .as_object()
        .unwrap()
        .iter()
        .filter_map(|(k, v)| v.as_array().map(|v| (k.clone(), v.len())))
        .collect();
    counts.insert(
        "section_recess_patterns".into(),
        f.section_recess_patterns.len(),
    );
    if let Some(defining) = f.defining.get("fillets") {
        counts.insert("fillets".into(), fillet_patch_count(part, defining));
    }
    counts
}

fn census() -> BTreeMap<String, Value> {
    let root = common::fixtures();
    let mut paths = BTreeMap::new();
    for entry in std::fs::read_dir(&root).unwrap() {
        let p = entry.unwrap().path();
        let file = p.file_name().unwrap().to_str().unwrap();
        if file.starts_with("golden_") && file.ends_with(".step") {
            paths.insert(format!("golden/{file}"), p);
        }
    }
    if let Some(dir) = common::corpus_dir() {
        for entry in common::load("corpus.json")["files"].as_array().unwrap() {
            let file = entry["file"].as_str().unwrap();
            paths.insert(format!("corpus/{file}"), dir.join(file));
        }
    }
    for file in [
        "nist_ctc_02_asme1_ap242-e2.stp.gz",
        "nist_ftc_07_asme1_ap242-e2.stp.gz",
    ] {
        paths.insert(format!("fixture/{file}"), root.join(file));
    }
    let calls: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("captured/calls.json")).unwrap())
            .unwrap();
    for call in calls["calls"].as_array().unwrap() {
        if let Some(file) = call["file"].as_str() {
            paths.insert(format!("captured/{file}"), root.join("captured").join(file));
        }
    }
    let paths: Vec<_> = paths.into_iter().collect();
    common::parallel::map(&paths, |(key, path)| {
        let part = read_step_file(path).unwrap();
        let inventory = features::inventory(&part).unwrap();
        let mut raw = inventory.physical.clone();
        // Derived families must be counted over the raw members as well as accepted members.
        raw.hole_patterns = features::hole_patterns::recognise_hole_patterns(&raw.holes);
        raw.gusset_rib_patterns =
            features::gussets::recognise_gusset_rib_patterns(&raw.gusset_ribs);
        raw.slot_patterns = features::recess_patterns::recognise_slot_patterns(&raw.slots);
        raw.pocket_patterns = features::recess_patterns::recognise_pocket_patterns(&raw.pockets);
        raw.oriented_slot_patterns =
            features::oriented_slots::recognise_oriented_slot_patterns(&raw.oriented_slots);
        let mut accepted = inventory.accepted();
        // Include the public aggregate's unified recess projection, not just its sources.
        // The public projection API performs its own inventory; this is test-only work.
        if let Ok(projection) = features::section_recess_family::section_recess_projection(&part) {
            accepted.section_recesses = projection.section_recesses;
            accepted.section_recess_patterns = projection.patterns;
        }
        let mut result =
            json!({"physical": counts(&part, &raw), "accepted": counts(&part, &accepted)});
        if key.starts_with("corpus/") || key.starts_with("fixture/") {
            let found = correspondence::recognise(&part);
            let working_counts = counts(&part, &found.features);
            let published: BTreeMap<_, _> = found.features.families().into_iter().collect();
            for (old, plain) in [
                ("oriented_slots", "slots"),
                ("oriented_chamfers", "chamfers"),
                ("oriented_slot_patterns", "slot_patterns"),
            ] {
                assert!(!published.contains_key(old), "{key}: old published family");
                assert_eq!(
                    published[plain].len(),
                    working_counts[plain] + working_counts[old],
                    "{key}: merged {plain} inventory"
                );
            }
            result["framed"] = json!(working_counts);
        }
        (key.clone(), result)
    })
    .into_iter()
    .collect()
}

#[test]
fn inventories_do_not_silently_shrink() {
    let actual = census();
    // An explicit output path permits the same census to run at the reference revision.
    if let Some(path) = std::env::var_os("QUIDDITY_INVENTORY_OUTPUT") {
        std::fs::write(path, serde_json::to_string_pretty(&actual).unwrap()).unwrap();
    }
    let path = common::fixtures().join("inventory_floor.json");
    if !path.exists() && std::env::var_os("QUIDDITY_INVENTORY_OUTPUT").is_some() {
        return;
    }
    // For #123, additive turned-body floors retain the historical feature baselines.
    let floor: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = Vec::new();
    for (file, expected) in floor["files"].as_object().unwrap() {
        let Some(got) = actual.get(file) else {
            // The corpus is optional in developer runs; mandatory CI exports it.
            assert!(
                file.starts_with("corpus/") && common::corpus_dir().is_none(),
                "missing {file}"
            );
            continue;
        };
        for (mode, families) in expected.as_object().unwrap() {
            for (family, baseline) in families.as_object().unwrap() {
                let baseline = baseline.as_u64().unwrap();
                let now = got[mode][family].as_u64().unwrap_or(0);
                if now >= baseline {
                    continue;
                }
                let allowance =
                    floor["losses"].as_array().unwrap().iter().find(|e| {
                        e["file"] == *file && e["mode"] == *mode && e["family"] == *family
                    });
                let allowed = allowance.is_some_and(|e| {
                    !e["reason"].as_str().unwrap_or("").is_empty()
                        && !e["evidence"].as_str().unwrap_or("").is_empty()
                        && baseline - now <= e["max_lost"].as_u64().unwrap()
                });
                if !allowed {
                    failures.push(format!("{file} {mode}/{family}: {baseline} -> {now}; an evidence-backed loss entry is required"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
