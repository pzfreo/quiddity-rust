//! Print holes with their defining faces: `cargo run --example holes -- file.step`.
fn main() {
    let path = std::env::args().nth(1).unwrap();
    let part = quiddity::read_step_file(std::path::Path::new(&path)).expect("read");
    let ctx = quiddity::features::Context::new(&part);
    for o in quiddity::features::holes::discover(&ctx, &[]) {
        println!("{:?}\n   defining {:?}", o.record, o.defining);
    }
}
