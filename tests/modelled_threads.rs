//! Measured thread geometry, claims, publication and adversarial geometry.
mod common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use quiddity::features::modelled_threads::{self, ModelledThread};
use quiddity::features::{self, Context};
use quiddity::kernel::geom::{self, Surface};
use quiddity::{Part, recognise_modelled_threads};

fn fixture(name: &str) -> PathBuf {
    common::fixtures().join("modelled_threads").join(name)
}

fn read(path: &Path) -> Part {
    quiddity::read_step_file(path).unwrap()
}

fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{actual} != {expected} ± {tolerance}"
    );
}

fn measured(
    t: &ModelledThread,
    side: &str,
    major: f64,
    minor: f64,
    pitch: f64,
    start: f64,
    end: f64,
) {
    assert_eq!(t.side, side);
    assert_eq!(t.hand, "right");
    assert_eq!(t.starts, 1);
    close(t.major_diameter, major, 0.002);
    close(t.minor_diameter, minor, 0.002);
    close(t.pitch, pitch, 0.0001);
    close(t.start, start, 0.003);
    close(t.end, end, 0.003);
    close(t.length, end - start, 0.003);
    close(geom::norm(t.axis.direction), 1.0, 1e-12);
    close(geom::dot(t.axis.direction, t.axis.point), 0.0, 1e-10);
    assert!(!t.flank_faces.is_empty() && !t.crest_faces.is_empty() && !t.root_faces.is_empty());
}

#[test]
fn corpus_threads_have_measured_dimensions_and_disjoint_claims() {
    let Some(dir) = common::corpus_dir() else {
        return;
    };
    for (file, expected_count, claimed_freeforms) in [
        ("cadgenbench_inputs/cgb243.step.gz", 4, 8),
        ("cadgenbench/threaded_connector_109.step", 1, 136),
    ] {
        let part = read(&dir.join(file));
        let ctx = Context::new(&part);
        let found = modelled_threads::analyse_modelled_threads(&part);
        assert!(found.refusals.is_empty(), "{file}: {:?}", found.refusals);
        assert_eq!(found.threads.len(), expected_count, "{file}");
        assert_eq!(
            modelled_threads::discover_verified(&ctx).unwrap(),
            found.threads
        );
        let mut claims = BTreeSet::new();
        for occurrence in &found.threads {
            let t = &occurrence.record;
            let roles: Vec<_> = t
                .flank_faces
                .iter()
                .chain(&t.crest_faces)
                .chain(&t.root_faces)
                .chain(&t.runout_faces)
                .copied()
                .collect();
            assert_eq!(roles.len(), occurrence.defining.len());
            assert_eq!(
                roles.into_iter().collect::<BTreeSet<_>>(),
                occurrence.defining.iter().copied().collect()
            );
            assert!(occurrence.defining.windows(2).all(|w| w[0] < w[1]));
            assert!(
                occurrence.defining.iter().all(|&f| claims.insert(f)),
                "shared face on {file}"
            );
        }
        assert_eq!(
            claims
                .iter()
                .filter(|&&f| matches!(
                    part.faces[f].surface,
                    Surface::Freeform {
                        kind: "BSPLINE",
                        ..
                    }
                ))
                .count(),
            claimed_freeforms
        );
        let ts: Vec<_> = found.threads.iter().map(|o| &o.record).collect();
        if file.contains("cgb243") {
            // Face 224 is the root of the measured 1.5 mm-pitch external thread, not
            // a 10.374 mm-wide annular groove between its overlapping crest bands.
            assert!(ts.iter().any(|t| t.root_faces.contains(&224)));
            assert!(quiddity::features::grooves::discover(&ctx).is_empty());
            measured(
                ts[0],
                "internal",
                13.623797625,
                12.0,
                1.5,
                38.242761638,
                47.368488013,
            );
            measured(
                ts[1],
                "internal",
                61.623797625,
                60.0,
                1.5,
                -48.501406281,
                -37.377132605,
            );
            measured(
                ts[2],
                "internal",
                61.623797625,
                60.0,
                1.5,
                40.872867415,
                49.998593745,
            );
            measured(
                ts[3],
                "external",
                75.0,
                73.376202375,
                1.5,
                55.556716214,
                66.868489861,
            );
        } else {
            measured(
                ts[0],
                "external",
                110.0,
                103.12,
                6.35,
                367.614207311,
                419.205792702,
            );
        }
        let aggregate = features::recognise(&part);
        assert_eq!(
            aggregate.modelled_threads,
            recognise_modelled_threads(&part)
        );
        let unclaimed: Vec<_> = quiddity::recognise_freeform_surfaces(&part)
            .into_iter()
            .filter(|r| !claims.contains(&r.face))
            .collect();
        assert_eq!(aggregate.freeform_surfaces, unclaimed);
    }
}

#[test]
fn a_physical_diamond_knurl_and_round_wire_spring_are_not_threads() {
    for name in ["diamond_knurl.step.gz", "round_wire_spring.step.gz"] {
        let part = read(&fixture(name));
        assert!(!part.solids.is_empty());
        assert!(
            part.faces
                .iter()
                .any(|f| matches!(f.surface, Surface::Freeform { .. })),
            "{name} must exercise sampled surfaces"
        );
        let found = modelled_threads::analyse_modelled_threads(&part);
        assert!(found.threads.is_empty(), "{name}: {:?}", found.threads);
    }
    // A prismatic repeated radial boundary (gear / spline form) has no screw advance.
    let gear = read(&common::fixtures().join("golden_repeating_radial_profile.step"));
    assert!(recognise_modelled_threads(&gear).is_empty());
}

#[test]
fn the_surveyed_spiral_vanes_blends_and_cosmetic_threads_remain_unclaimed() {
    let Some(dir) = common::corpus_dir() else {
        return;
    };
    for file in [
        "cadgenbench_inputs/cgb203.step", // spiral impeller vanes, varying radius
        "cadgenbench_inputs/cgb202.step.gz", // rib / cylinder intersections
        "cadgenbench_inputs/cgb207.step", // short near-helical blends
        "cadgenbench_inputs/cgb217.step.gz",
        "cadgenbench_inputs/cgb241.step",
        "cadgenbench_inputs/cgb242.step.gz",
        "cadgenbench_inputs/cgb245.step",
        "gramel/GRM-03_thumbwheel_drive_screw.step", // authored cosmetic thread / knurl
        "gramel/GRM-05_depth_lock_bolt.step",
        "gramel/string_post.step",
    ] {
        let part = read(&dir.join(file));
        let found = modelled_threads::analyse_modelled_threads(&part);
        assert!(found.threads.is_empty(), "{file}: {:?}", found.threads);
        assert!(
            found.refusals.is_empty(),
            "a blend is not even a thread candidate: {file}: {:?}",
            found.refusals
        );
    }
}

#[test]
fn the_document_publishes_measurements_claims_and_fingerprints_without_a_designation() {
    let Some(dir) = common::corpus_dir() else {
        return;
    };
    let (out, _) = quiddity::serve::recognise_file(
        dir.join("cadgenbench/threaded_connector_109.step")
            .to_str()
            .unwrap(),
    )
    .unwrap();
    let doc = &out.document;
    assert_eq!(doc.schema_version, "quiddity-rust/recognition/4");
    let records: Vec<_> = doc
        .records
        .iter()
        .filter(|r| r.family == "modelled_threads")
        .collect();
    assert_eq!(records.len(), 1);
    let r = records[0];
    assert_eq!(r.record_type, "ModelledThread");
    assert!(r.local.is_empty());
    assert!(r.parameters.get("designation").is_none());
    let t = &out.recognition.features.modelled_threads[0];
    let expected = &out.recognition.features.defining["modelled_threads"][0];
    assert_eq!(&r.faces, expected);
    assert_eq!(
        r.parameters,
        serde_json::to_value(t)
            .unwrap()
            .as_object()
            .unwrap()
            .clone()
    );
    let fp = out
        .recognition
        .fingerprints
        .features
        .iter()
        .find(|f| f.id == r.id)
        .unwrap();
    assert_eq!(&fp.faces, expected);
    assert!(fp.axis.is_some());
    assert!(fp.sizes.contains_key("pitch"));
}

#[test]
fn thread_axis_and_axial_endpoints_follow_a_rigid_motion() {
    let Some(dir) = common::corpus_dir() else {
        return;
    };
    let path = dir.join("cadgenbench/threaded_connector_109.step");
    let original = read(&path);
    let angle = 0.41_f64;
    let placement = [
        [angle.cos(), 0.0, angle.sin(), 123.0],
        [0.0, 1.0, 0.0, -47.0],
        [-angle.sin(), 0.0, angle.cos(), 91.0],
    ];
    let moved = quiddity::kernel::step::read_step_file_placed(&path, &placement).unwrap();
    let motion = quiddity::framed_records::Rigid::from_placement(&placement);
    let mapped = quiddity::framed_records::map_features(features::recognise(&original), &motion);
    let actual = recognise_modelled_threads(&moved);
    assert_eq!(actual.len(), 1);
    assert!(common::same(
        &serde_json::to_value(&mapped.features.modelled_threads).unwrap(),
        &serde_json::to_value(&actual).unwrap()
    ));
    let t = &actual[0];
    let endpoint = geom::add(t.axis.point, geom::scale(t.axis.direction, t.start));
    let old = &mapped.features.modelled_threads[0];
    close(
        geom::dist(
            endpoint,
            geom::add(old.axis.point, geom::scale(old.axis.direction, old.start)),
        ),
        0.0,
        0.003,
    );
}

#[test]
fn a_left_hand_two_start_thread_measures_pitch_instead_of_lead() {
    let part = read(&fixture("left_two_start.step.gz"));
    let found = modelled_threads::analyse_modelled_threads(&part);
    assert!(found.refusals.is_empty(), "{:?}", found.refusals);
    assert_eq!(found.threads.len(), 1);
    let t = &found.threads[0].record;
    assert_eq!((t.side, t.hand, t.starts), ("external", "left", 2));
    close(t.major_diameter, 20.0, 0.002);
    close(t.minor_diameter, 19.0, 0.002);
    close(t.pitch, 1.0, 0.0001);
    close(t.length, 8.0, 0.0001);
}

#[test]
fn varying_pitch_and_a_shallow_helical_groove_report_refusals_and_claim_nothing() {
    for (name, reason) in [
        ("varying_pitch_sweep.step.gz", "inconsistent_pitch"),
        ("helical_groove.step.gz", "groove_proportions"),
    ] {
        let part = read(&fixture(name));
        let analysis = modelled_threads::analyse_modelled_threads(&part);
        assert!(analysis.threads.is_empty());
        assert_eq!(
            analysis.refusals.len(),
            1,
            "{name}: {:?}",
            analysis.refusals
        );
        assert_eq!(analysis.refusals[0].reason, reason);
        let recognised = quiddity::correspondence::recognise(&part);
        assert_eq!(
            recognised.features.freeform_surfaces,
            quiddity::recognise_freeform_surfaces(&part)
        );
        let doc = quiddity::recognition::document(&recognised, part.faces.len());
        assert_eq!(doc.schema_version, quiddity::recognition::SCHEMA_VERSION);
        let refusals: Vec<_> = doc
            .records
            .iter()
            .filter(|r| r.family == "modelled_thread_refusals")
            .collect();
        assert_eq!(refusals.len(), 1);
        assert_eq!(refusals[0].record_type, "ModelledThreadRefusal");
        assert!(
            !recognised
                .fingerprints
                .features
                .iter()
                .any(|f| f.id == refusals[0].id)
        );
    }
}
