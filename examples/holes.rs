//! Print holes with their defining faces: `cargo run --example holes -- file.step`.
fn main() {
    let path = std::env::args().nth(1).unwrap();
    let part = quiddity::read_step_file(std::path::Path::new(&path)).expect("read");
    let recognised = quiddity::correspondence::recognise(&part);
    for (record, faces) in recognised
        .features
        .holes
        .iter()
        .zip(&recognised.features.defining["holes"])
    {
        println!("{record:?}\n   defining {faces:?}");
    }
}
