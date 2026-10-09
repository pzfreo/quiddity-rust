//! Ray crossings on corpus lines where the kernel once missed one: the interior-void grid's
//! lines through cgb243's B-spline combs and past cgb217's B-spline cracks. Each line's
//! crossing is OpenCascade's (`IntCurvesFace_ShapeIntersector` on the same line), and every
//! line must cross the solid an even number of times.

mod common;

use haecceity::rays::RayCaster;
use haecceity::read_step_file;

/// A line along an axis from *start*, the length cast, and the crossings (distance along the
/// line, face) it must find.
struct Line {
    axis: usize,
    start: [f64; 3],
    length: f64,
    crossings: &'static [(f64, usize)],
}

fn check(file: &str, lines: &[Line]) {
    let Some(dir) = common::corpus_dir() else {
        assert!(std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none());
        return;
    };
    let part = read_step_file(&dir.join(file)).unwrap();
    let rays = RayCaster::for_solid(&part, 0);
    for line in lines {
        let along = [0, 1, 2].map(|a| if a == line.axis { 1.0 } else { 0.0 });
        let hits = rays.hits(line.start, along, line.length).unwrap();
        for &(t, face) in line.crossings {
            assert!(
                hits.iter()
                    .any(|h| h.face == face && (h.t - t).abs() < 1e-3),
                "{file}: the line from {:?} misses face {face} at {t}: {hits:?}",
                line.start
            );
        }
        // The interior-void grid's count: hits closer than its coincidence are one crossing.
        let mut ts: Vec<f64> = Vec::new();
        for h in &hits {
            if ts.last().is_none_or(|&last| h.t - last > 1e-6) {
                ts.push(h.t);
            }
        }
        assert_eq!(ts.len() % 2, 0, "{file}: {:?} {hits:?}", line.start);
    }
}

/// Crossings inside B-spline comb faces, 0.03 to 0.42 mm from their edges, that the trim test
/// missed when it re-inverted the surface at the crossing instead of taking the parameters the
/// intersection solved for (the inversion settled on a boundary 1.3 to 1.5 mm away). The last
/// line also meets the comb of faces 233 and 234 seven times where OpenCascade does not
/// (its count is odd there).
#[test]
fn cgb243_comb_crossings() {
    let (x, y) = (57.522041606459666, -52.07839475623629);
    let length = 105.92641355081115;
    let lines = [
        Line {
            axis: 0,
            start: [-68.1357627038163, -29.32313272638889, -24.03904999278629],
            length: 137.0812410657556,
            crossings: &[(125.4968, 225), (126.2877, 223)],
        },
        Line {
            axis: 1,
            start: [x, -53.20850048784631, -24.03904999278629],
            length,
            crossings: &[(24.2494, 225)],
        },
        Line {
            axis: 1,
            start: [x, -53.20850048784631, -19.885072990793695],
            length,
            crossings: &[(21.2129, 225)],
        },
        Line {
            axis: 2,
            start: [-11.018578926418137, 28.83254530150743, y],
            length,
            crossings: &[(94.583, 239), (95.6636, 238)],
        },
        Line {
            axis: 2,
            start: [-6.864601924425543, 28.83254530150743, y],
            length,
            crossings: &[(95.3001, 238)],
        },
        Line {
            axis: 2,
            start: [3.5203405805559385, 24.67856829951483, y],
            length,
            crossings: &[(95.1512, 238)],
        },
        Line {
            axis: 2,
            start: [x, -31.400121227385185, y],
            length,
            crossings: &[(31.2762, 225)],
        },
        Line {
            axis: 2,
            start: [x, -29.32313272638889, y],
            length,
            crossings: &[(28.4771, 225)],
        },
        Line {
            axis: 2,
            start: [-25.55749843339221, -29.32313272638889, y],
            length,
            crossings: &[
                (5.0696, 233),
                (6.5696, 233),
                (8.0696, 233),
                (9.5696, 233),
                (9.7599, 234),
                (11.0696, 233),
                (12.5696, 233),
            ],
        },
    ];
    check("cadgenbench_inputs/cgb243.step.gz", &lines);
}

/// Crossings on cylinders just outside their trim, in the crack a B-spline neighbour leaves:
/// face 51's edge with face 43 strays 2.0 µm from the cylinder and 12.8 µm from face 43, and
/// the line crosses the cylinder 2.85 µm outside the trim (OpenCascade finds it); face 358's
/// edge with face 47 strays 2.6 µm from the cylinder and 19.2 µm from face 47, whose surface
/// the second line never meets, and the line crosses the cylinder 6.0 µm outside the trim
/// (OpenCascade misses it, and its count is odd).
#[test]
fn cgb217_crack_crossings() {
    let lines = [
        Line {
            axis: 1,
            start: [15.540277715778075, -65.55942849728709, 18.692870175876823],
            length: 132.61036984128498,
            crossings: &[(88.4062, 51)],
        },
        Line {
            axis: 0,
            start: [-68.37722194941009, 7.997886024050686, 20.764907204646907],
            length: 136.75444389882514,
            crossings: &[(34.8145, 358)],
        },
    ];
    check("cadgenbench_inputs/cgb217.step.gz", &lines);
}
