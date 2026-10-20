//! Issue #53. The truck fixtures are byte-for-byte copies of the master and 0.6.0
//! exports. The independent fixtures were written by hand: exact quadrant circles
//! about +X, and degree-one B-spline meridians swept by SURFACE_OF_REVOLUTION.
//! Shaft meridian (x,r): (0,0), (0,15), (40,15), (40,10), (100,10), (100,0).
//! Cone meridian: (0,0), (0,3), (4,0). Neither uses truck or an OCCT exporter.
//!
//! Two input defects must not be hidden by changing the integral's sign or relaxing
//! the parser. Master's revolution FACE_SURFACEs have same_sense=.F. despite their
//! meridians generating outward normals; its hole B-spline faces have correct senses.
//! OCCT repairs these orientations. Pub's hole file repeats entity #539 (and others),
//! with different types. Its permissive OCCT import is not a valid reference solid.

use std::f64::consts::PI;
use std::path::Path;

use haecceity::geom::{self, Surface};
use haecceity::step::{IDENTITY, Placement, read_step_file_placed};

fn fixture(name: &str, placement: &Placement) -> haecceity::Part {
    read_step_file_placed(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures")
            .join(name),
        placement,
    )
    .unwrap()
}

fn motion() -> Placement {
    let [x, y, z] = geom::unit([1.0, 2.0, 3.0]).unwrap();
    let (s, c) = 37f64.to_radians().sin_cos();
    let k = 1.0 - c;
    [
        [c + x * x * k, x * y * k - z * s, x * z * k + y * s, 123.456],
        [y * x * k + z * s, c + y * y * k, y * z * k - x * s, -78.9],
        [z * x * k - y * s, z * y * k + x * s, c + z * z * k, 41.3],
    ]
}

fn close(got: f64, want: f64) {
    assert!((got - want).abs() <= 1e-9 * want.abs(), "{got} vs {want}");
}

#[test]
fn revolution_shafts_and_cone_have_analytic_mass_and_moments() {
    for (name, volume, area, centroid) in [
        (
            "truck_pub_shaft_only.step.gz",
            15000.0 * PI,
            2850.0 * PI,
            [40.0, 0.0, 0.0],
        ),
        (
            "revolution_shaft.step.gz",
            15000.0 * PI,
            2850.0 * PI,
            [40.0, 0.0, 0.0],
        ),
        (
            "revolution_cone.step.gz",
            12.0 * PI,
            24.0 * PI,
            [1.0, 0.0, 0.0],
        ),
    ] {
        for placement in [IDENTITY, motion()] {
            let part = fixture(name, &placement);
            assert!(part.unresolved_faces().is_empty());
            assert!(part.unresolved_edges().is_empty());
            assert!(part.solid_is_valid(0));
            assert!(part.faces.iter().all(|f| matches!(
                f.surface,
                Surface::Freeform {
                    kind: "REVOLUTION",
                    ..
                }
            )));
            let (v, a) = part.solid_mass(0).unwrap();
            close(v, volume);
            close(a, area);
            let (v, c) = part.volume_centroid().unwrap();
            close(v, volume);
            let want = std::array::from_fn(|i| {
                geom::dot(
                    [placement[i][0], placement[i][1], placement[i][2]],
                    centroid,
                ) + placement[i][3]
            });
            assert!(geom::dist(c, want) < 1e-8, "{name}: {c:?} vs {want:?}");
            let mut sum = [0.0; 3];
            let mut face_area = 0.0;
            for i in 0..part.faces.len() {
                let m = part.face_moments(i).unwrap();
                face_area += m.area;
                sum = geom::add(sum, m.area_vector);
            }
            close(face_area, area);
            assert!(geom::norm(sum) < 1e-9 * area, "{name}: {sum:?}");
        }
    }
}

#[test]
fn mass_keeps_the_prescribed_orientation() {
    let mut part = fixture("revolution_shaft.step.gz", &IDENTITY);
    for face in &mut part.faces {
        face.reversed = !face.reversed;
    }
    let (volume, area) = part.solid_mass(0).unwrap();
    close(volume, -15000.0 * PI);
    close(area, 2850.0 * PI);
}

#[test]
fn truck_master_has_reversed_revolution_senses() {
    for name in [
        "truck_master_shaft_only.step.gz",
        "truck_master_shaft_hole.step.gz",
    ] {
        let mut part = fixture(name, &IDENTITY);
        assert!(part.solid_mass(0).unwrap().0 < 0.0);
        // Diagnostic correction of the source defect, deliberately confined to this
        // test. No production code overrides STEP face senses or takes abs(volume).
        for face in &mut part.faces {
            if matches!(
                face.surface,
                Surface::Freeform {
                    kind: "REVOLUTION",
                    ..
                }
            ) {
                assert!(face.reversed);
                face.reversed = false;
            }
        }
        let (volume, area) = part.solid_mass(0).unwrap();
        if name.contains("only") {
            close(volume, 15000.0 * PI);
            close(area, 2850.0 * PI);
        } else {
            // OCP 7.9, adaptive BRepGProp.VolumeProperties(1e-10): 46135.47088917437.
            // OCCT projects approximate pcurves on these revolutions: even the exact
            // unbored shaft reads 47118.969265311505 rather than 15000π (4.92 mm³ low).
            // Compare its hole result to 2e-4; analytic fixtures above require 1e-9.
            assert!(
                (volume - 46135.47088917437).abs() < 2e-4 * 46135.47088917437,
                "{volume}"
            );
        }
    }
}

#[test]
fn malformed_truck_pub_hole_is_refused() {
    let error = read_step_file_placed(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/broken/truck_pub_shaft_hole.step.gz"),
        &IDENTITY,
    )
    .expect_err("duplicate STEP entity IDs must not be silently overwritten");
    assert!(
        error.to_string().contains("duplicate entity #539"),
        "{error}"
    );
}
