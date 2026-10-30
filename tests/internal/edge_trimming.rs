//! For #134: treatments preserve supporting features; real dimension/footprint edits do not.
use super::common;
use quiddity::features::{Context, bosses, plates};
use quiddity::kernel::geom::{self, Surface};

fn part(name: &str) -> quiddity::Part {
    quiddity::read_step_file(&common::fixtures().join(name)).unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-6, "{a} != {b}");
}

#[test]
fn gramil_end_chamfer_preserves_boss_height_and_plate_but_shortening_does_not() {
    for (name, height, lo, hi) in [
        ("old", 37.6, 33.6, 37.6),
        ("new", 37.6, 33.6, 37.6),
        ("shorter", 35.6, 31.6, 35.6),
    ] {
        let p = part(&format!("revisions/gramil_shaft.{name}.step.gz"));
        let ctx = Context::new(&p);
        let b = bosses::discover_verified(&ctx).unwrap();
        assert_eq!(b.len(), 1);
        near(b[0].record.height, height);
        let plates = plates::discover_verified(&ctx, &Default::default()).unwrap();
        assert_eq!(plates.len(), 1);
        near(*plates[0].record.lo, lo);
        near(*plates[0].record.hi, hi);
        near(plates[0].record.v, 0.190838914);
    }
}

#[test]
fn boss_rounds_include_their_axial_extent_even_below_the_fillet_reporting_floor() {
    for name in ["boss_end_round.step.gz", "boss_end_small_round.step.gz"] {
        let p = part(name);
        for motion in [
            [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]],
            [[0., 0., -1., 17.], [1., 0., 0., -23.], [0., -1., 0., 9.]],
        ] {
            let p = p.transformed(&motion).unwrap();
            let ctx = Context::new(&p);
            let b = bosses::discover_verified(&ctx).unwrap();
            assert_eq!(b.len(), 1);
            near(b[0].record.height, 20.);
            let end = motion.map(|r| r[2] * 24. + r[3]);
            near(geom::dist(b[0].record.location, end), 0.);
            assert!(
                b[0].context
                    .iter()
                    .any(|&f| matches!(p.faces[f].surface, Surface::Torus { .. }))
            );
        }
    }
    let p = part("boss_end_sphere.step.gz");
    let b = bosses::discover_verified(&Context::new(&p)).unwrap();
    near(b[0].record.height, 25.);
    near(b[0].record.location[2], 29.);
    // Two chamfers on the original analytic disc, #134: 6.5 mm of cylinder + 2 × 0.25.
    let p = part("turned_body_disc.step");
    near(bosses::discover(&Context::new(&p))[0].record.height, 7.);
}

#[test]
fn projected_round_strips_restore_plate_coverage_and_centres() {
    let p = part("revisions/q134_plate.old.step.gz");
    let rounded = part("plate_edge_round.step.gz");
    let get = |p: &quiddity::Part| {
        plates::discover_verified(&Context::new(p), &Default::default())
            .unwrap()
            .into_iter()
            .find(|o| o.record.axis == 'z')
            .unwrap()
    };
    let (a, b) = (get(&p), get(&rounded));
    near(*a.record.lo, *b.record.lo);
    near(*a.record.hi, *b.record.hi);
    near(a.record.u, b.record.u);
    near(a.record.v, b.record.v);
    assert!(
        b.context
            .iter()
            .any(|&f| matches!(rounded.faces[f].surface, Surface::Cylinder { .. }))
    );
    // Orthogonal projection of the R1.5 quarter torus is the annulus R5..R3.5.
    let p = part("boss_end_round.step.gz");
    let torus = p
        .faces
        .iter()
        .position(|f| matches!(f.surface, Surface::Torus { .. }))
        .unwrap();
    let (a, c) = p.face_projected_moments(torus, [0., 0., 1.]).unwrap();
    near(a, std::f64::consts::PI * (25. - 3.5_f64.powi(2)));
    near(c[0], 0.);
    near(c[1], 0.);
}

#[test]
fn a_real_plate_thinning_or_removal_remains_visible() {
    for (name, thickness) in [("old", 4.), ("thinned", 2.)] {
        let p = part(&format!("revisions/q134_plate.{name}.step.gz"));
        let found = plates::discover_verified(&Context::new(&p), &Default::default()).unwrap();
        let plate = found.iter().find(|o| o.record.axis == 'z').unwrap();
        near(plate.record.thickness(), thickness);
    }
    let p = part("revisions/q134_plate.removed.step.gz");
    assert!(plates::discover(&Context::new(&p), &Default::default()).is_empty());
}
