//! Inspect measured threads and refusals through the supported recognition pipeline.
fn main() {
    for path in std::env::args().skip(1) {
        let part = quiddity::read_step_file(std::path::Path::new(&path)).expect("read STEP");
        let start = std::time::Instant::now();
        let found = quiddity::correspondence::recognise(&part).features;
        println!(
            "{}",
            serde_json::json!({"file":path,"seconds":start.elapsed().as_secs_f64(),"threads":found.modelled_threads.iter().zip(&found.defining["modelled_threads"]).map(|(record,faces)| serde_json::json!({"record":record,"faces":faces})).collect::<Vec<_>>(),"refusals":found.modelled_thread_refusals})
        );
    }
}
