//! Accuracy, not parity: recognition must not depend on where the part sits. Every corpus part is
//! re-read under rigid motions that keep principal axes principal (a translation and right-angle
//! rotations), and every family must find the same features on the same faces. Python is not
//! consulted; differences it shares are still failures here. Explained exceptions live in
//! `tests/fixtures/known_invariance.json`.

mod common;

use std::collections::BTreeMap;

use quiddity::Part;
use quiddity::features::{
    Context, Occurrence, angled_steps, bosses, chamfers, circular_blind_steps,
    circular_face_patterns, countersinks, fillets, flats, holes, interior_voids,
    oblique_through_steps, oriented_chamfers, paired_ramp_steps, thin_walls,
};
use quiddity::kernel::step::{IDENTITY, Placement, read_step_file_placed};

/// A non-round translation (so rounding grids are not trivially aligned) and five rotations with
/// exact ±1/0 entries, so the placement itself adds no round-off.
const T: [f64; 3] = [123.456, -78.9, 41.3];
/// A named motion: rotation rows and translation.
type Motion = (&'static str, [[f64; 3]; 3], [f64; 3]);

const MOTIONS: [Motion; 6] = [
    (
        "translate",
        [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        T,
    ),
    (
        "rot_z90",
        [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        [0.0; 3],
    ),
    (
        "rot_x90",
        [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]],
        [0.0; 3],
    ),
    (
        "rot_y180",
        [[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]],
        [0.0; 3],
    ),
    (
        "cycle_xyz",
        [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        [0.0; 3],
    ),
    (
        "rot_zx_moved",
        [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        T,
    ),
];

fn placement(r: &[[f64; 3]; 3], t: &[f64; 3]) -> Placement {
    [0, 1, 2].map(|i| [r[i][0], r[i][1], r[i][2], t[i]])
}

/// Each family's occurrences as a sorted list of sorted defining-face sets: what was found, and
/// where, independent of the record's coordinates.
fn signature(part: &Part) -> BTreeMap<&'static str, Vec<Vec<usize>>> {
    fn faces<R>(found: Vec<Occurrence<R>>) -> Vec<Vec<usize>> {
        let mut out: Vec<Vec<usize>> = found
            .into_iter()
            .map(|o| {
                let mut f = o.defining;
                f.sort_unstable();
                f
            })
            .collect();
        out.sort();
        out
    }
    let ctx = Context::new(part);
    let seats = countersinks::discover(&ctx);
    let holes = holes::discover(&ctx, &seats);
    BTreeMap::from([
        (
            "fillets",
            faces(fillets::discover(&ctx, &Default::default())),
        ),
        (
            "chamfers",
            faces(chamfers::discover(&ctx, &Default::default())),
        ),
        ("bosses", faces(bosses::discover(&ctx))),
        ("angled_steps", faces(angled_steps::discover(&ctx))),
        ("flats", faces(flats::discover(&ctx))),
        (
            "paired_ramp_steps",
            faces(paired_ramp_steps::discover(&ctx)),
        ),
        (
            "oriented_chamfers",
            faces(oriented_chamfers::discover(&ctx, &Default::default())),
        ),
        (
            "circular_face_patterns",
            faces(circular_face_patterns::discover(&ctx)),
        ),
        ("thin_walls", faces(thin_walls::discover(&ctx))),
        ("interior_voids", faces(interior_voids::discover(&ctx))),
        (
            "oblique_through_steps",
            faces(oblique_through_steps::discover(&ctx)),
        ),
        (
            "circular_blind_steps",
            faces(circular_blind_steps::discover(&ctx)),
        ),
        ("countersinks", faces(seats)),
        ("holes", faces(holes)),
    ])
}

#[test]
fn recognition_is_invariant_under_rigid_motion() {
    let Some(dir) = common::corpus_dir() else {
        assert!(
            std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
            "QUIDDITY_CORPUS_REQUIRED is set but the corpus was not found"
        );
        eprintln!("corpus not found; set QUIDDITY_CORPUS");
        return;
    };
    let known = common::load("known_invariance.json");
    let known = known.as_array().unwrap();
    common::check_verdicts("known_invariance.json", known);
    let known: Vec<(String, String, String)> = known
        .iter()
        .map(|k| {
            let s = |f: &str| k[f].as_str().unwrap().to_string();
            (s("file"), s("motion"), s("family"))
        })
        .collect();
    let mut problems = Vec::new();
    let mut seen = Vec::new();
    for entry in common::load("corpus.json")["files"].as_array().unwrap() {
        let name = entry["file"].as_str().unwrap();
        let path = dir.join(name);
        let Ok(base) = read_step_file_placed(&path, &IDENTITY) else {
            continue; // the corpus test reports read failures
        };
        let expected = signature(&base);
        for (motion, r, t) in &MOTIONS {
            let moved = read_step_file_placed(&path, &placement(r, t)).unwrap();
            assert_eq!(moved.faces.len(), base.faces.len(), "{name} {motion}");
            for (family, got) in signature(&moved) {
                if got == expected[family] {
                    continue;
                }
                let key = (name.to_string(), motion.to_string(), family.to_string());
                if known.contains(&key) {
                    seen.push(key);
                    continue;
                }
                problems.push(format!(
                    "{name} {motion} {family}: {} occurrences, {} unmoved; differing: {:?}",
                    got.len(),
                    expected[family].len(),
                    symmetric_difference(&expected[family], &got),
                ));
            }
        }
    }
    for key in &known {
        if !seen.contains(key) {
            problems.push(format!("{key:?} is listed but now invariant: remove it"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

fn symmetric_difference(a: &[Vec<usize>], b: &[Vec<usize>]) -> Vec<String> {
    let only = |x: &[Vec<usize>], y: &[Vec<usize>], tag: &str| {
        x.iter()
            .filter(|f| !y.contains(f))
            .map(|f| format!("{tag}{f:?}"))
            .collect::<Vec<_>>()
    };
    let mut out = only(a, b, "-");
    out.extend(only(b, a, "+"));
    out
}
