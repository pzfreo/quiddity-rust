//! Accuracy, not parity: recognition must not depend on where the part sits. Every corpus part is
//! re-read under rigid motions that keep principal axes principal (a translation and right-angle
//! rotations), and every family must find the same features on the same faces. Python is not
//! consulted; differences it shares are still failures here. Explained exceptions live in
//! `tests/fixtures/known_invariance.json`, each with the most occurrences it may differ by
//! (`max_differing`), so a listed difference that grows still fails.

mod common;
#[macro_use]
#[path = "support/slices.rs"]
mod slices;

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

/// Each family's occurrences (`features::recognise`'s defining faces, under its field names, step
/// levels and risers among them), and face levels' faces, as a sorted list of sorted face sets:
/// what was found, and where, independent of the record's coordinates.
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
    out
}

/// Families read along world Z by specification (`quiddity.levels`: the levels of a part's
/// horizontal planes, the step levels among them, and the risers between them), so compared only
/// under motions that keep the Z axis vertical (translate, rot_z90, rot_y180): under the others
/// the part's horizontal faces are other faces, in Python as here.
const WORLD_Z: [&str; 3] = ["face_levels", "risers", "step_levels"];

/// The listed exceptions for these motions whose file is *owned* (the slice's,
/// `tests/support/slices.rs`), keyed by (file, motion, family), with the most occurrences each
/// may differ by.
fn known_entries(
    motions: &[&str],
    owned: impl Fn(&str) -> bool,
) -> Vec<((String, String, String), usize)> {
    let known = common::load("known_invariance.json");
    let known = known.as_array().unwrap();
    common::check_verdicts("known_invariance.json", known);
    known
        .iter()
        .filter(|k| {
            motions.contains(&k["motion"].as_str().unwrap()) && owned(k["file"].as_str().unwrap())
        })
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

/// Every corpus part under every motion: one test per slice of the corpus
/// (`tests/support/slices.rs`).
fn invariant_under_rigid_motion(k: usize, n: usize) {
    let Some(dir) = common::corpus_dir() else {
        assert!(
            std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
            "QUIDDITY_CORPUS_REQUIRED is set but the corpus was not found"
        );
        eprintln!("corpus not found; set QUIDDITY_CORPUS");
        return;
    };
    let names: Vec<&str> = MOTIONS.iter().map(|m| m.0).collect();
    let all = corpus_files();
    let known = known_entries(&names, |file| slices::owns(&all, k, n, file));
    let files = slices::slice(&all, k, n);
    // Every part unmoved and under every motion, all spread over every core at once (so a costly
    // part's readings run side by side); the differences come back in corpus order, motion by
    // motion.
    let readings: Vec<(usize, Option<&Motion>)> = (0..files.len())
        .flat_map(|i| {
            std::iter::once(None)
                .chain(MOTIONS.iter().map(Some))
                .map(move |m| (i, m))
        })
        .collect();
    let mut read = common::parallel::map(&readings, |&(i, motion)| {
        let at = motion.map_or(IDENTITY, |(_, r, t)| placement(r, t));
        // (The error as text: the kernel's error is not `Send`.)
        read_step_file_placed(&dir.join(&files[i]), &at)
            .map(|p| (p.faces.len(), signature(&p)))
            .map_err(|e| e.to_string())
    })
    .into_iter();
    let mut found = Vec::new();
    for name in &files {
        let unmoved = read.next().unwrap();
        let moved: Vec<_> = read.by_ref().take(MOTIONS.len()).collect();
        // The corpus test reports read failures.
        let Ok((faces, expected)) = unmoved else {
            continue;
        };
        for ((motion, r, _), moved) in MOTIONS.iter().zip(moved) {
            let (moved_faces, got) = moved.unwrap();
            assert_eq!(moved_faces, faces, "{name} {motion}");
            let d = differences(&expected, &got, |family| {
                WORLD_Z.contains(&family) && r[2][2].abs() != 1.0
            });
            found.push((name, *motion, d));
        }
    }
    let problems = unexplained(&known, found.into_iter());
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

sliced!(
    recognition_is_invariant_under_rigid_motion,
    super::invariant_under_rigid_motion,
    [
        0 => slice_0,
        1 => slice_1,
        2 => slice_2,
        3 => slice_3,
        4 => slice_4,
        5 => slice_5,
        6 => slice_6,
        7 => slice_7,
    ]
);

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
/// the generic rotation: each family, face levels, step levels and risers included (in the frame
/// they are read along the frame's z), must find the same features on the same faces. Where a
/// part's frame is not full the two working frames may differ by the gauge's freedom (a sign or
/// interchange, a roll about the axis), and the listed exceptions say so.
///
/// Also prints, without failing, how many family results caller-space recognition changes under
/// the same rotation (review M8's measure).
///
/// One test per slice of the corpus (`tests/support/slices.rs`); the measure printed is the
/// slice's.
fn framed_invariant_under_a_generic_rotation(k: usize, n: usize) {
    let Some(dir) = common::corpus_dir() else {
        assert!(
            std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
            "QUIDDITY_CORPUS_REQUIRED is set but the corpus was not found"
        );
        eprintln!("corpus not found; set QUIDDITY_CORPUS");
        return;
    };
    let all = corpus_files();
    let known = known_entries(&[GENERIC], |file| slices::owns(&all, k, n, file));
    let files = slices::slice(&all, k, n);
    let generic = rotation([1.0, 2.0, 3.0], 37.0, T);
    // Each part's four readings (as read, then in its frame; unmoved, then rotated), all spread
    // over every core at once, so a costly part's readings run side by side.
    let readings: Vec<(usize, bool, &Placement)> = (0..files.len())
        .flat_map(|i| {
            [
                (false, &IDENTITY),
                (false, &generic),
                (true, &IDENTITY),
                (true, &generic),
            ]
            .map(|(framed, at)| (i, framed, at))
        })
        .collect();
    let mut read = common::parallel::map(&readings, |&(i, framed, at)| {
        let path = dir.join(&files[i]);
        if framed {
            frames::prepare_framed_file(&path, at)
                .map(|f| (Some(f.frame.gauge), signature(&f.part)))
                .map_err(|e| e.to_string())
        } else {
            read_step_file_placed(&path, at)
                .map(|p| (None, signature(&p)))
                .map_err(|e| e.to_string())
        }
    })
    .into_iter();
    let found: Vec<_> = files
        .iter()
        .map(|_| {
            let mut next = || read.next().unwrap();
            let (plain_unmoved, plain_moved) = (next(), next());
            // Caller space: the part as read, and as read under the rotation.
            let plain = plain_unmoved
                .ok()
                .map(|(_, unmoved)| differences(&unmoved, &plain_moved.unwrap().1, |_| false));
            // In the part's frame. A refused frame (no material, an unmeasured face) is a
            // difference only when the two placements disagree on it.
            let differing = match (next(), next()) {
                (Ok((_, unmoved)), Ok((_, moved))) => differences(&unmoved, &moved, |_| false),
                (Err(a), Err(b)) if a == b => Vec::new(),
                (a, b) => vec![Difference {
                    family: "frame",
                    count: 1,
                    text: format!(
                        "unmoved {:?}, moved {:?}",
                        a.map(|f| f.0.unwrap()),
                        b.map(|f| f.0.unwrap())
                    ),
                }],
            };
            (plain, differing)
        })
        .collect();
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

sliced!(
    framed_recognition_is_invariant_under_a_generic_rotation,
    super::framed_invariant_under_a_generic_rotation,
    [
        0 => slice_0,
        1 => slice_1,
        2 => slice_2,
        3 => slice_3,
        4 => slice_4,
        5 => slice_5,
        6 => slice_6,
        7 => slice_7,
    ]
);

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

/// Repeating radial profiles, which no corpus part has: every captured part Python finds one on,
/// under each motion and, in its own frame, under the generic rotation, must give the same
/// opposed faces with the same repeat and edge counts.
#[test]
fn repeating_radial_profiles_are_invariant_on_captured_parts() {
    let calls = common::load("captured/calls.json");
    let mut files: Vec<&str> = calls["calls"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| {
            c["function"] == "recognise_repeating_radial_profiles"
                && c["result"].as_array().is_some_and(|r| !r.is_empty())
        })
        .map(|c| c["file"].as_str().unwrap())
        .collect();
    files.sort_unstable();
    files.dedup();
    assert!(!files.is_empty());
    let read = |part: &Part| {
        let found = features::recognise(part);
        let counts = found
            .repeating_radial_profiles
            .iter()
            .map(|r| (r.repeat_count, r.edge_count));
        let mut both: Vec<(Vec<usize>, (usize, usize))> =
            found.defining["repeating_radial_profiles"]
                .iter()
                .map(|f| {
                    let mut f = f.clone();
                    f.sort_unstable();
                    f
                })
                .zip(counts)
                .collect();
        both.sort();
        both
    };
    let generic = rotation([1.0, 2.0, 3.0], 37.0, T);
    for name in files {
        let path = common::fixtures().join("captured").join(name);
        let part = read_step_file_placed(&path, &IDENTITY).unwrap();
        let unmoved = read(&part);
        assert!(!unmoved.is_empty(), "{name}: no profile found");
        // The evidence path publishes them on a solid and refuses an open shell, as Python's
        // writer does (`test_open_shell_and_malformed_sampling_fail_closed`).
        let verified =
            features::repeating_profiles::discover_verified(&features::Context::new(&part));
        if part.solids.is_empty() {
            assert_eq!(
                verified,
                Err(features::EvidenceError::NoValidSolid),
                "{name}"
            );
        } else {
            assert_eq!(verified.map(|f| f.len()), Ok(unmoved.len()), "{name}");
        }
        for (motion, r, t) in &MOTIONS {
            let moved = read(&read_step_file_placed(&path, &placement(r, t)).unwrap());
            assert_eq!(moved, unmoved, "{name} {motion}");
        }
        // A refused frame (the open shell has no material) must be refused both ways.
        let framed = |p: &Placement| {
            frames::prepare_framed_file(&path, p)
                .map(|f| read(&f.part))
                .map_err(|e| e.to_string())
        };
        let unmoved = framed(&IDENTITY);
        assert!(
            unmoved.as_ref().map_or(true, |f| !f.is_empty()),
            "{name}: none framed"
        );
        assert_eq!(framed(&generic), unmoved, "{name} {GENERIC}");
    }
}
