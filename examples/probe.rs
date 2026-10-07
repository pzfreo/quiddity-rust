//! Classify a point: `cargo run --example probe -- file.step x y z`.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let part = quiddity::read_step_file(std::path::Path::new(&a[1])).expect("read");
    let p = [
        a[2].parse().unwrap(),
        a[3].parse().unwrap(),
        a[4].parse().unwrap(),
    ];
    let c = quiddity::kernel::classify::Classifier::new(&part);
    println!("{:?}", c.classify(p));
    println!("{:?}", c.explain(p));
}
