//! #127: CTC-05's non-tangent R8-inch surfaces must reach the document as profiles.
use super::common;
use quiddity::correspondence;
use quiddity::kernel::geom::{self, Surface, V3};
use quiddity::recognition::{self, FaceAccounting};

fn direction(value: &serde_json::Value) -> V3 {
    match value.as_str() {
        Some("x") => [1., 0., 0.],
        Some("y") => [0., 1., 0.],
        Some("z") => [0., 0., 1.],
        _ => serde_json::from_value(value.clone()).unwrap(),
    }
}

#[test]
fn ctc05_four_r203_2_faces_are_convex_profiles_with_intrinsic_axes() {
    let Some(dir) = common::corpus_dir() else {
        assert!(std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none());
        return;
    };
    let original = quiddity::read_step_file(&dir.join("nist/nist_ctc_05_asme1_rd.stp")).unwrap();
    for degrees in [0_f64, 27.4] {
        let (s, c) = degrees.to_radians().sin_cos();
        let part = original
            .transformed(&[[c, 0., s, 12.], [0., 1., 0., -4.], [-s, 0., c, 7.]])
            .unwrap();
        for reading in [
            correspondence::recognise(&part),
            correspondence::caller_space_recognise(&part),
        ] {
            let doc = recognition::document(&reading, part.faces.len());
            for face in [14, 15, 168, 169] {
                let profiles: Vec<_> = doc
                    .records
                    .iter()
                    .filter(|r| r.family == "cylindrical_profiles" && r.faces.contains(&face))
                    .collect();
                assert_eq!(profiles.len(), 1, "face {face}, {degrees} degrees");
                let r = profiles[0];
                assert_eq!(r.record_type, "CylindricalProfile");
                assert_eq!(r.faces, vec![face]);
                assert_eq!(r.parameters["faces"], serde_json::json!([face]));
                assert_eq!(r.parameters["radius"], 203.2);
                assert_eq!(r.parameters["side"], "convex");
                let Surface::Cylinder { frame, .. } = part.faces[face].surface else {
                    panic!()
                };
                let axis = direction(&r.parameters["axis"]);
                assert!(geom::cross(axis, frame.z).iter().all(|x| x.abs() < 1e-10));
                let point: V3 = serde_json::from_value(r.parameters["axis_point"].clone()).unwrap();
                assert!(geom::norm(geom::cross(geom::sub(point, frame.origin), frame.z)) < 0.001);
                let (u0, u1, _, _) = part.uv_bounds(face).unwrap();
                assert!((r.parameters["sweep"].as_f64().unwrap() - (u1 - u0)).abs() < 1e-10);
                let FaceAccounting::Feature { record_ids } =
                    &doc.face_accounting.as_ref().unwrap()[face]
                else {
                    panic!("profile lacks accounting")
                };
                assert!(record_ids.contains(&r.id));
                assert!(
                    doc.records
                        .iter()
                        .filter(|r| r.faces.contains(&face))
                        .all(|r| r.family == "cylindrical_profiles")
                );
                let fp = reading
                    .fingerprints
                    .features
                    .iter()
                    .find(|fp| fp.id == r.id)
                    .unwrap();
                assert_eq!(fp.faces, vec![face]);
                assert_eq!(fp.sizes["radius"], 203.2);
                assert_eq!(fp.traits["side"], "convex");
            }
        }
    }
}
