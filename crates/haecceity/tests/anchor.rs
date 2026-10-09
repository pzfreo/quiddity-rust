//! A point proved on each face's trimmed region ([`Part::face_anchor`], upstream need U6 of
//! specify-core-rust), over every face of the corpus: every point returned has parameters the
//! face's domain contains and lies on the surface; the faces refused are counted and listed.

mod common;

use haecceity::Part;
use haecceity::read_step_file;

/// How far an anchor may lie from the surface's point at its parameters' inversion.
const ON_SURFACE: f64 = 1e-7;

/// One file: (faces, anchored at the centre, anchored from the mesh, refusals, problems).
type Outcome = (usize, usize, usize, Vec<String>, Vec<String>);

fn check(part: &Part, file: &str) -> Outcome {
    let mut out: Outcome = (0, 0, 0, Vec::new(), Vec::new());
    for face in 0..part.faces.len() {
        out.0 += 1;
        let surface = &part.faces[face].surface;
        match part.face_anchor(face) {
            Ok(anchor) => {
                let (u, v) = anchor.uv;
                if !part.domain(face).is_some_and(|d| d.contains(u, v)) {
                    out.4
                        .push(format!("{file} face {face}: {:?} is outside", anchor.uv));
                }
                let at = surface.value(u, v);
                let back = surface
                    .parameters(anchor.point, Some(anchor.uv))
                    .map(|(a, b)| surface.value(a, b));
                let off = back.map_or(f64::INFINITY, |q| haecceity::geom::dist(q, anchor.point));
                if haecceity::geom::dist(at, anchor.point) > ON_SURFACE || off > ON_SURFACE {
                    out.4.push(format!(
                        "{file} face {face}: {anchor:?} is {off:e} off the surface"
                    ));
                }
                if part
                    .face_centre(face)
                    .is_some_and(|c| haecceity::geom::dist(c, anchor.point) <= 1e-12)
                {
                    out.1 += 1;
                } else {
                    out.2 += 1;
                }
            }
            Err(why) => {
                assert!(
                    why.reason.starts_with(&format!("face {face}: ")),
                    "{file}: {why}"
                );
                out.3.push(format!("{file} {why}"));
            }
        }
    }
    out
}

/// The faces of the corpus the anchor refuses, with their reasons (a change must be made here, so
/// the count is seen to move): one whose centre is outside and whose mesh is refused
/// (tests/mesh.rs `KNOWN_REFUSALS`).
const REFUSED: &[&str] = &[
    "cadgenbench_inputs/cgb202.step.gz face 399: its centre is outside it and its boundary \
     crosses itself in parameter space",
];

#[test]
fn every_corpus_face_has_a_point_on_its_trimmed_region() {
    let Some(dir) = common::corpus_dir() else {
        assert!(std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none());
        return;
    };
    let corpus = common::load("corpus.json");
    let files: Vec<String> = corpus["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["file"].as_str().unwrap().to_owned())
        .collect();
    let outcomes = common::parallel::map(&files, |file| {
        // (A file the reader refuses has no faces to anchor; tests/corpus.rs owns those.)
        match read_step_file(&dir.join(file)) {
            Ok(part) => check(&part, file),
            Err(_) => (0, 0, 0, Vec::new(), Vec::new()),
        }
    });
    let (mut faces, mut centre, mut mesh) = (0, 0, 0);
    let (mut refused, mut problems) = (Vec::new(), Vec::new());
    for o in outcomes {
        faces += o.0;
        centre += o.1;
        mesh += o.2;
        refused.extend(o.3);
        problems.extend(o.4);
    }
    println!(
        "{faces} faces: {centre} anchored at their centre, {mesh} from their mesh, {} refused",
        refused.len()
    );
    for r in &refused {
        println!("  {r}");
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    assert_eq!(refused, REFUSED, "the refusals changed");
}
