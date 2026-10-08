//! Accuracy, not parity: recognition must not depend on where the part sits. Every corpus part is
//! re-read under rigid motions that keep principal axes principal (a translation and right-angle
//! rotations), and every family must find the same features on the same faces. Python is not
//! consulted; differences it shares are still failures here. Explained exceptions live in
//! `tests/fixtures/known_invariance.json`, each with the most occurrences it may differ by
//! (`max_differing`), so a listed difference that grows still fails.

mod common;

use std::collections::BTreeMap;

use quiddity::Part;
use quiddity::features::{self, levels};
use quiddity::frames;
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

/// Each family's occurrences (`features::recognise`'s defining faces, under its field names), and
/// face levels' and risers' faces, as a sorted list of sorted face sets: what was found, and
/// where, independent of the record's coordinates.
fn signature(part: &Part) -> BTreeMap<&'static str, Vec<Vec<usize>>> {
    fn sorted(found: impl IntoIterator<Item = Vec<usize>>) -> Vec<Vec<usize>> {
        let mut out: Vec<Vec<usize>> = found
            .into_iter()
            .map(|mut f| {
                f.sort_unstable();
                f
            })
            .collect();
        out.sort();
        out
    }
    let mut out: BTreeMap<_, _> = features::recognise(part)
        .defining
        .into_iter()
        .map(|(family, found)| (family, sorted(found)))
        .collect();
    let found = levels::face_levels_with_faces(part, &Default::default());
    out.insert("face_levels", sorted(found.into_iter().map(|(_, f)| f)));
    let found = levels::risers_with_faces(part, &Default::default());
    out.insert("risers", sorted(found.into_iter().map(|(_, f)| f)));
    out
}

/// Families read along world Z by specification (`quiddity.levels`: the levels of a part's
/// horizontal planes, and the risers between them), so compared only under motions that keep the
/// Z axis vertical (translate, rot_z90, rot_y180): under the others the part's horizontal faces
/// are other faces, in Python as here.
const WORLD_Z: [&str; 2] = ["face_levels", "risers"];

/// The listed exceptions for these motions, keyed by (file, motion, family), with the most
/// occurrences each may differ by.
fn known_entries(motions: &[&str]) -> Vec<((String, String, String), usize)> {
    let known = common::load("known_invariance.json");
    let known = known.as_array().unwrap();
    common::check_verdicts("known_invariance.json", known);
    known
        .iter()
        .filter(|k| motions.contains(&k["motion"].as_str().unwrap()))
        .map(|k| {
            let s = |f: &str| k[f].as_str().unwrap().to_string();
            let most = k["max_differing"]
                .as_u64()
                .unwrap_or_else(|| panic!("an entry gives max_differing: {k}"));
            ((s("file"), s("motion"), s("family")), most as usize)
        })
        .collect()
}

fn corpus_files() -> Vec<String> {
    common::load("corpus.json")["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["file"].as_str().unwrap().to_string())
        .collect()
}

/// A family whose occurrences differ under a motion: how many differ, and which.
struct Difference {
    family: &'static str,
    count: usize,
    text: String,
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
    let names: Vec<&str> = MOTIONS.iter().map(|m| m.0).collect();
    let known = known_entries(&names);
    let files = corpus_files();
    // Every part unmoved, then every part under every motion, each spread over every core; the
    // differences come back in corpus order, motion by motion.
    let unmoved = common::parallel::map(&files, |name| {
        let base = read_step_file_placed(&dir.join(name), &IDENTITY).ok()?;
        Some((base.faces.len(), signature(&base)))
    });
    let cases: Vec<(usize, &Motion)> = (0..files.len())
        .filter(|&i| unmoved[i].is_some()) // the corpus test reports read failures
        .flat_map(|i| MOTIONS.iter().map(move |m| (i, m)))
        .collect();
    let found = common::parallel::map(&cases, |&(i, (motion, r, t))| {
        let name = &files[i];
        let (faces, expected) = unmoved[i].as_ref().unwrap();
        let moved = read_step_file_placed(&dir.join(name), &placement(r, t)).unwrap();
        assert_eq!(moved.faces.len(), *faces, "{name} {motion}");
        differences(expected, &signature(&moved), |family| {
            WORLD_Z.contains(&family) && r[2][2].abs() != 1.0
        })
    });
    let found = cases
        .iter()
        .zip(found)
        .map(|(&(i, (motion, _, _)), d)| (&files[i], *motion, d));
    let problems = unexplained(&known, found);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The families of *got* whose occurrences are not *expected*'s, except those *skip* names.
fn differences(
    expected: &BTreeMap<&'static str, Vec<Vec<usize>>>,
    got: &BTreeMap<&'static str, Vec<Vec<usize>>>,
    skip: impl Fn(&str) -> bool,
) -> Vec<Difference> {
    let mut out = Vec::new();
    for (&family, got) in got {
        if skip(family) {
            continue;
        }
        if *got != expected[family] {
            let differing = symmetric_difference(&expected[family], got);
            out.push(Difference {
                family,
                count: differing.len(),
                text: format!(
                    "{} occurrences, {} unmoved; differing: {differing:?}",
                    got.len(),
                    expected[family].len(),
                ),
            });
        }
    }
    out
}

/// The differences not explained by a listed exception, a listed exception exceeded, and every
/// listed exception no longer seen.
fn unexplained<'a>(
    known: &[((String, String, String), usize)],
    found: impl Iterator<Item = (&'a String, &'a str, Vec<Difference>)>,
) -> Vec<String> {
    let mut problems = Vec::new();
    let mut seen = Vec::new();
    for (name, motion, differences) in found {
        for d in differences {
            let key = (name.clone(), motion.to_string(), d.family.to_string());
            match known.iter().find(|k| k.0 == key) {
                Some((_, most)) => {
                    if d.count > *most {
                        problems.push(format!(
                            "{name} {motion} {} is listed with at most {most} differing, now {}: {}",
                            d.family, d.count, d.text
                        ));
                    }
                    seen.push(key);
                }
                None => problems.push(format!(
                    "{name} {motion} {}: {} differing, {}",
                    d.family, d.count, d.text
                )),
            }
        }
    }
    for (key, _) in known {
        if !seen.contains(key) {
            problems.push(format!("{key:?} is listed but now invariant: remove it"));
        }
    }
    problems
}

/// Review M8's generic rotation (37° about (1, 2, 3), then the translation `T`), under its name in
/// `known_invariance.json`. No principal axis stays principal, so caller-space recognition (which
/// reads world axes) is not expected to survive it; recognition in the part's own frame
/// (`quiddity::frames`) is.
const GENERIC: &str = "generic_framed";

/// The rotation by *degrees* about *axis* (Rodrigues), then *translation*.
fn rotation(axis: [f64; 3], degrees: f64, translation: [f64; 3]) -> Placement {
    let n = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
    let [x, y, z] = axis.map(|c| c / n);
    let (s, c) = degrees.to_radians().sin_cos();
    let t = 1.0 - c;
    [
        [
            t * x * x + c,
            t * x * y - s * z,
            t * x * z + s * y,
            translation[0],
        ],
        [
            t * x * y + s * z,
            t * y * y + c,
            t * y * z - s * x,
            translation[1],
        ],
        [
            t * x * z - s * y,
            t * y * z + s * x,
            t * z * z + c,
            translation[2],
        ],
    ]
}

/// Every corpus part recognised in its own frame (`frames::prepare_framed`), unmoved and under
/// the generic rotation: each family, face levels and risers included (in the frame they are read
/// along the frame's z), must find the same features on the same faces. Where a part's frame is
/// not full the two working frames may differ by the gauge's freedom (a sign or interchange, a
/// roll about the axis), and the listed exceptions say so.
///
/// Also prints, without failing, how many family results caller-space recognition changes under
/// the same rotation (review M8's measure).
#[test]
fn framed_recognition_is_invariant_under_a_generic_rotation() {
    let Some(dir) = common::corpus_dir() else {
        assert!(
            std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
            "QUIDDITY_CORPUS_REQUIRED is set but the corpus was not found"
        );
        eprintln!("corpus not found; set QUIDDITY_CORPUS");
        return;
    };
    let known = known_entries(&[GENERIC]);
    let files = corpus_files();
    let generic = rotation([1.0, 2.0, 3.0], 37.0, T);
    let found = common::parallel::map(&files, |name| {
        let path = dir.join(name);
        // Caller space: the part as read, and as read under the rotation.
        let plain = read_step_file_placed(&path, &IDENTITY).ok().map(|unmoved| {
            let moved = read_step_file_placed(&path, &generic).unwrap();
            differences(&signature(&unmoved), &signature(&moved), |_| false)
        });
        // In the part's frame. A refused frame (no material, an unmeasured face) is a
        // difference only when the two placements disagree on it.
        let framed = |placement: &Placement| {
            frames::prepare_framed_file(&path, placement)
                .map(|f| (f.frame.gauge, signature(&f.part)))
                .map_err(|e| e.to_string())
        };
        let differing = match (framed(&IDENTITY), framed(&generic)) {
            (Ok((_, unmoved)), Ok((_, moved))) => differences(&unmoved, &moved, |_| false),
            (Err(a), Err(b)) if a == b => Vec::new(),
            (a, b) => vec![Difference {
                family: "frame",
                count: 1,
                text: format!("unmoved {:?}, moved {:?}", a.map(|f| f.0), b.map(|f| f.0)),
            }],
        };
        (plain, differing)
    });
    let (mut results, mut occurrences) = (0, 0);
    for (name, (plain, _)) in files.iter().zip(&found) {
        for d in plain.iter().flatten() {
            results += 1;
            occurrences += d.count;
            eprintln!("caller space, {name} {}: {}", d.family, d.text);
        }
    }
    eprintln!(
        "caller-space recognition under the generic rotation: {results} family results change \
         ({occurrences} occurrences differ) over {} parts",
        files.len()
    );
    let framed = files
        .iter()
        .zip(found)
        .map(|(name, (_, d))| (name, GENERIC, d));
    let problems = unexplained(&known, framed);
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
