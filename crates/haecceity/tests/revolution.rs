//! Issue #53. The truck fixtures are byte-for-byte copies of the master and 0.6.0
//! exports. The independent fixtures were written by hand: exact quadrant circles
//! about +X, and degree-one B-spline meridians swept by SURFACE_OF_REVOLUTION.
//! Shaft meridian (x,r): (0,0), (0,15), (40,15), (40,10), (100,10), (100,0).
//! Cone meridian: (0,0), (0,3), (4,0). Neither uses truck or an OCCT exporter.
//! revolution_line_shaft has the same exact quadrant topology but unbounded LINE
//! meridians: the first disk starts on the axis, and the annulus's LINE starts
//! at (40,-20,0) toward +Y, on the far side of the axis from its +Y trimming edges.
//! Exact pcurves specify that sheet for STEP readers; haecceity's freeform
//! integrator still projects the 3D boundary curves.
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

fn close(context: &str, got: f64, want: f64) {
    assert!(
        (got - want).abs() <= 1e-9 * want.abs(),
        "{context}: {got} vs {want}"
    );
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
            "revolution_line_shaft.step.gz",
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
            close(&format!("{name} volume at {placement:?}"), v, volume);
            close(&format!("{name} solid area at {placement:?}"), a, area);
            let (v, c) = part.volume_centroid().unwrap();
            close(&format!("{name} volume at {placement:?}"), v, volume);
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
            close(
                &format!("{name} summed face area at {placement:?}"),
                face_area,
                area,
            );
            assert!(geom::norm(sum) < 1e-9 * area, "{name}: {sum:?}");
        }
    }
}

#[test]
fn mass_keeps_the_prescribed_orientation() {
    for name in ["revolution_shaft.step.gz", "revolution_line_shaft.step.gz"] {
        let mut part = fixture(name, &IDENTITY);
        for face in &mut part.faces {
            face.reversed = !face.reversed;
        }
        let (volume, area) = part.solid_mass(0).unwrap();
        close(&format!("{name} reversed volume"), volume, -15000.0 * PI);
        close(&format!("{name} area"), area, 2850.0 * PI);
    }
}

#[test]
fn radial_line_disk_and_far_side_annulus_have_analytic_signed_flux() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/revolution_line_shaft.step.gz");
    let text =
        haecceity::step::decode_step_file_bytes(&path, std::fs::read(&path).unwrap()).unwrap();
    // The swept curves themselves must be LINEs, not merely the seam edges.
    let (model, _) = step_io::read(&text).unwrap();
    assert_eq!(model.surface_of_revolution_arena.items.len(), 5);
    assert!(
        model
            .surface_of_revolution_arena
            .items
            .iter()
            .all(|s| matches!(s.swept_curve, step_io::generated::model::CurveRef::Line(_)))
    );
    let part = fixture("revolution_line_shaft.step.gz", &IDENTITY);
    for (faces, x, quarter_area, nx) in [
        (0..4, 0.0, 225.0 * PI / 4.0, -1.0),
        (8..12, 40.0, 125.0 * PI / 4.0, 1.0),
    ] {
        for f in faces {
            let [area, flux] = part.face_mass(f).unwrap();
            close(
                &format!("revolution_line_shaft.step.gz face {f} area"),
                area,
                quarter_area,
            );
            assert!((flux - x * nx * quarter_area).abs() < 1e-9 * quarter_area);
            let m = part.face_moments(f).unwrap();
            assert!((m.centroid[0] - x).abs() < 1e-9);
            assert!(geom::dist(m.area_vector, [nx * quarter_area, 0.0, 0.0]) < 1e-9 * quarter_area);
        }
    }
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
            close(&format!("{name} corrected volume"), volume, 15000.0 * PI);
            close(&format!("{name} area"), area, 2850.0 * PI);
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

#[test]
fn revolution_control_net_ulp_perturbations() {
    use haecceity::nurbs::NurbsSurface;
    for (name, volume, area, cx) in [
        (
            "truck_pub_shaft_only.step.gz",
            15000.0 * PI,
            2850.0 * PI,
            40.0,
        ),
        ("revolution_shaft.step.gz", 15000.0 * PI, 2850.0 * PI, 40.0),
        (
            "revolution_line_shaft.step.gz",
            15000.0 * PI,
            2850.0 * PI,
            40.0,
        ),
        ("revolution_cone.step.gz", 12.0 * PI, 24.0 * PI, 1.0),
    ] {
        for placement in [IDENTITY, motion()] {
            let original = fixture(name, &placement);
            for trial in 0..32u64 {
                let mut bits = trial + 1;
                let mut faces = original.faces.clone();
                for face in &mut faces {
                    if let Surface::Freeform { surface, .. } = &mut face.surface {
                        let mut poles = surface.control_points.clone();
                        for p in poles.iter_mut().flatten() {
                            for x in p {
                                bits ^= bits << 13;
                                bits ^= bits >> 7;
                                bits ^= bits << 17;
                                let shift = (bits % 17) as i32 - 8;
                                for _ in 0..shift.abs() {
                                    *x = if shift > 0 {
                                        x.next_up()
                                    } else {
                                        x.next_down()
                                    };
                                }
                            }
                        }
                        *surface = Box::new(
                            NurbsSurface::new(
                                surface.degree_u,
                                surface.degree_v,
                                poles,
                                surface.weights.clone(),
                                surface.knots_u.clone(),
                                surface.knots_v.clone(),
                            )
                            .unwrap(),
                        );
                    }
                }
                let part =
                    haecceity::Part::new(faces, original.edges.clone(), original.solids.clone());
                let context = format!("{name} control-net trial {trial} pose {placement:?}");
                let (v, a) = part.solid_mass(0).unwrap();
                close(&format!("{context} volume"), v, volume);
                close(&format!("{context} area"), a, area);
                let (v, c) = part.volume_centroid().unwrap();
                close(&format!("{context} centroid volume"), v, volume);
                let want = [0, 1, 2].map(|i| placement[i][0] * cx + placement[i][3]);
                assert!(
                    geom::dist(c, want) < 1e-9,
                    "{context} centroid {c:?} vs {want:?}"
                );
            }
        }
    }
}

#[test]
fn radial_line_sheet_choice_tolerates_axis_coordinate_roundoff() {
    let name = "revolution_line_shaft.step.gz";
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name);
    let bytes =
        haecceity::step::decode_step_file_bytes(&path, std::fs::read(&path).unwrap()).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    for shift in [-8i32, -4, -2, -1, 0, 1, 2, 4, 8] {
        let mut changed = 0;
        let text = text
            .lines()
            .map(|line| {
                if line.contains("CARTESIAN_POINT") {
                    let (prefix, coords) = line.rsplit_once(",(").unwrap();
                    let coords = coords.strip_suffix("));").unwrap();
                    let mut p: Vec<f64> = coords
                        .split(',')
                        .map(|x| x.trim().parse().unwrap())
                        .collect();
                    if p.len() == 3 {
                        p[1] += 1.0; // translate the whole geometry, leaving 2D pcurves alone
                        if line.starts_with("#72 =") {
                            // This LINE's origin lies on the axis. Nudge its translated y=1
                            // either way by a few ulps, rather than testing subnormal zero.
                            for _ in 0..shift.abs() {
                                p[1] = if shift > 0 {
                                    p[1].next_up()
                                } else {
                                    p[1].next_down()
                                };
                            }
                            changed += 1;
                        }
                        return format!("{prefix},({:.17E},{:.17E},{:.17E}));", p[0], p[1], p[2]);
                    }
                }
                line.to_owned()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(changed, 1);
        let part = haecceity::read_step(text.as_bytes()).unwrap();
        let context = format!("{name} axis origin {shift} ulps");
        let (v, a) = part
            .solid_mass(0)
            .unwrap_or_else(|| panic!("{context} solid mass unresolved"));
        close(&format!("{context} volume"), v, 15000.0 * PI);
        close(&format!("{context} area"), a, 2850.0 * PI);
        let (_, c) = part.volume_centroid().unwrap();
        assert!(
            geom::dist(c, [40., 1., 0.]) < 1e-9,
            "{context} centroid {c:?}"
        );
    }
}
