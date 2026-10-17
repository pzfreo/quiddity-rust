//! The recognition fast path must answer exactly as the complete volume query does.
use quiddity::features::Context;
use quiddity::features::volume_probe::{Spans, prism_is_empty, prism_material_fraction};
use quiddity::kernel as haecceity;
use std::path::Path;

#[test]
fn bounded_emptiness_matches_complete_volume_on_planar_and_curved_parts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut empty = 0;
    let mut occupied = 0;
    for name in [
        "filleted_plate_with_holes.step",
        "golden_blind_pockets_and_pocket_patterns.step",
        "filleted_stepped_shaft.step",
    ] {
        let part = haecceity::read_step(&std::fs::read(root.join(name)).unwrap()).unwrap();
        let ctx = Context::new(&part);
        let b = part.bounds();
        let span = [0, 1, 2].map(|k| b.max[k] - b.min[k]);
        let at = |lo: f64, hi: f64| -> Spans {
            [0, 1, 2].map(|k| (b.min[k] + lo * span[k], b.min[k] + hi * span[k]))
        };
        let mut probes = vec![at(0., 1.), at(-0.1, 1.1), at(1.1, 1.2), at(0.1, 0.2)];
        // Positive requested spans can round to collapsed probe bounds at a
        // large translation. Also cover a volume overflowing to infinity.
        probes.extend([
            [(1e16, 1e16 + 2.); 3],
            [(-1e16 - 2., -1e16); 3],
            [(1e100, 2e100); 3],
            [(1e200, 2e200); 3],
        ]);
        for k in 0..3 {
            let mut crossing = at(0.25, 0.75);
            crossing[k] = (b.min[k] - 0.1 * span[k], b.min[k] + 0.1 * span[k]);
            probes.push(crossing);
        }
        // Invalid, degenerate and sub-floor probes must never prove emptiness.
        for width in [-1., 0., 1e-7, 1e-6] {
            let mut thin = at(0.1, 0.2);
            thin[0].1 = thin[0].0 + width;
            probes.push(thin);
        }
        for spans in probes {
            for inset in [0., 1e-6, 0.25] {
                let reference = prism_material_fraction(&ctx, 0, &spans, inset);
                let bounded = prism_is_empty(&ctx, 0, &spans, inset);
                assert_eq!(bounded, reference == Some(0.), "{name}: {spans:?}, {inset}");
                if reference == Some(0.) {
                    empty += 1;
                } else if reference.is_some_and(|v| v > 0.) {
                    occupied += 1;
                }
            }
        }
    }
    assert!(empty > 0 && occupied > 0);
}
