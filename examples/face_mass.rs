//! Each face's area and natural-normal flux: `-- file`.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let part = quiddity::read_step_file(std::path::Path::new(&a[1])).expect("read");
    for f in 0..part.faces.len() {
        println!(
            "{f} {:?} {:?}",
            part.faces[f].surface.kind(),
            part.face_mass(f).map(|m| m[0])
        );
    }
}
