//! Loop orientation as the STEP reader builds it, against OpenCascade's import.

mod common;

use haecceity::brep::Arc;
use haecceity::step::read_step_file;

/// cgb217's one void shell (`BREP_WITH_VOIDS` holding an `ORIENTED_CLOSED_SHELL` used `.F.`)
/// is the drill point's cone 361, its bore 362 and the cross bore 363. Used reversed, each face
/// is reversed with its bounds: OpenCascade's explorer runs the circle between 361 and 362
/// forward in 361 and backward in 362, and every edge of 363 forward (the file writes them the
/// other way). Walked that way, all three pairs are concave, as they are by ray parity.
#[test]
fn reversed_void_shell_reverses_its_loops() {
    let Some(dir) = common::corpus_dir() else {
        assert!(std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none());
        return;
    };
    let part = read_step_file(&dir.join("cadgenbench_inputs/cgb217.step.gz")).unwrap();
    let uses = |face: usize| -> Vec<(u64, bool)> {
        part.faces[face]
            .loops
            .iter()
            .flat_map(|l| &l.edges)
            .map(|&(e, forward)| (part.edge_source(e).unwrap().entity, forward))
            .collect()
    };
    assert!(part.faces[361].reversed && part.faces[362].reversed && !part.faces[363].reversed);
    assert!(uses(361).contains(&(31916, true)), "{:?}", uses(361));
    assert!(uses(362).contains(&(31916, false)), "{:?}", uses(362));
    assert!(
        uses(363).iter().all(|&(_, forward)| forward),
        "{:?}",
        uses(363)
    );
    for (a, b) in [(361, 362), (361, 363), (362, 363)] {
        assert_eq!(part.arc(a, b), Some(Arc::Concave), "{a}-{b}");
        assert_eq!(part.arc(b, a), Some(Arc::Concave), "{b}-{a}");
    }
}
