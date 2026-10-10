//! The tilted mixed line/arc pocket's floor swept into a probe (section-recess test part
//! 45e9d1ade38c84f9, floor 14, run (0.515038, -0.250611, 0.819713)): the volume probe answers
//! it, measured against itself and against the part, where its rays used to be left unresolved.
//!
//! Every unresolved line passed an edge or vertex within the coordinate floor and met an odd
//! number of the faces there, beyond the nanometre nudges' reach. The file's vertices lie
//! 3.0e-8 mm off its lines and circles: the swept sides are planes through the vertices while
//! the caps place an edge by its line (one face of the two claims a line passing 2e-8 from
//! it); a cylindrical side's band past an edge (its vertices' stray) reaches 3e-8 further than
//! the cap's; and the pocket's first probe, inset 1e-6, puts breaks 1e-6 apart and so Gauss
//! points 6e-7 from the floor's corner, where all three faces put the crossing outside their
//! trims. The lines are now nudged by up to the coordinate floor (`volume::intervals`).

use haecceity::classify::Classifier;
use haecceity::geom::Surface;
use haecceity::rays::RayCaster;
use haecceity::sweep::extrude_face;
use haecceity::volume::{Probe, common_volume, probe_volume};
use haecceity::{Part, read_step_file};

fn part() -> Part {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../tests/fixtures/captured/section_recess_geometry/parts/45e9d1ade38c84f9.step.gz",
    );
    read_step_file(&path).unwrap()
}

/// Floor 14 moved *offset* and swept *length* along its normal.
fn prism(part: &Part, offset: f64, length: f64) -> Part {
    let Surface::Plane { frame } = part.faces[14].surface else {
        panic!("floor 14 is planar");
    };
    let n = frame.z;
    extrude_face(part, 14, n.map(|x| x * offset), n.map(|x| x * length)).unwrap()
}

#[test]
fn the_swept_floor_measured_against_itself_is_full() {
    let part = part();
    for length in [1.0, 4.9] {
        let prism = prism(&part, 0.0, length);
        let probe = Probe::Solid(RayCaster::for_solid(&prism, 0));
        let whole = probe_volume(&probe).unwrap();
        let got = common_volume(&Classifier::for_solid(&prism, 0), &probe);
        assert_eq!(got, Some(whole), "length {length}");
    }
}

#[test]
fn the_swept_floor_measured_against_the_part_agrees_with_opencascade() {
    // OpenCascade's `BRepAlgoAPI_Common` of the part with `BRepPrimAPI_MakePrism` of the same
    // face moved and swept the same way, measured by `BRepGProp` (OCP 7.9.3): the pocket's first
    // probe (the floor swept to the mouth, inset 1e-6 at both ends) holds no material, and the
    // floor swept from 1 mm below to 1 mm above holds the half below, 94.11442701019767 mm³.
    let part = part();
    let solid = Classifier::for_solid(&part, 0);
    let pocket = prism(&part, 1e-6, 4.9 - 2e-6);
    let probe = Probe::Solid(RayCaster::for_solid(&pocket, 0));
    let whole = probe_volume(&probe).unwrap();
    assert!((whole - 461.1605).abs() < 1e-3, "{whole}");
    assert_eq!(common_volume(&solid, &probe), Some(0.0));

    // Measured, not decided exactly: the probe's oblique cylindrical sides bend the length
    // across each slice where no break falls, and the adaptive rule stops short of exact
    // (6.7e-6 of the answer here).
    let straddle = prism(&part, -1.0, 2.0);
    let probe = Probe::Solid(RayCaster::for_solid(&straddle, 0));
    let got = common_volume(&solid, &probe).unwrap();
    let want = 94.11442701019767;
    assert!((got - want).abs() <= 1e-5 * want, "{got} vs {want}");
}
