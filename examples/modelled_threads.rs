//! Inspect measured threads and refusals without running the other feature families.
// Internal diagnostic: use the exact engine implementation without exporting caller-space APIs.
include!("../src/engine.rs");
extern crate self as quiddity;
fn main() {
    for path in std::env::args().skip(1) {
        let part = quiddity::read_step_file(std::path::Path::new(&path)).expect("read STEP");
        let start = std::time::Instant::now();
        let found = quiddity::features::modelled_threads::analyse_modelled_threads(&part);
        println!(
            "{}",
            serde_json::json!({"file":path,"seconds":start.elapsed().as_secs_f64(),"threads":found.threads.iter().map(|o| serde_json::json!({"record":o.record,"faces":o.defining})).collect::<Vec<_>>(),"refusals":found.refusals})
        );
    }
}
