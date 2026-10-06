//! Report edges whose two uses by different faces of a solid run the same way.
fn main() {
    for path in std::env::args().skip(1) {
        let part = quiddity::read_step_file(std::path::Path::new(&path)).expect("read");
        for (s, solid) in part.solids.iter().enumerate() {
            if part.solid_is_valid(s) {
                continue;
            }
            let mut uses: std::collections::HashMap<usize, Vec<(usize, bool)>> = Default::default();
            for &f in &solid.faces {
                for lp in &part.faces[f].loops {
                    for &(e, fwd) in &lp.edges {
                        uses.entry(e).or_default().push((f, fwd));
                    }
                }
            }
            for (e, u) in &uses {
                let bad = match u.as_slice() {
                    [(fa, da), (fb, db)] => fa != fb && da == db,
                    _ => true,
                };
                if bad {
                    println!(
                        "{path} solid {s} edge {e}: {:?} kinds {:?}",
                        u,
                        u.iter()
                            .map(|(f, _)| part.faces[*f].surface.kind())
                            .collect::<Vec<_>>()
                    );
                }
            }
        }
    }
}
