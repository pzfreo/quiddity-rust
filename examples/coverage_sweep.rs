//! Recognise every model in a list and write one JSON line per model: whether it read, whether
//! recognition completed or panicked, and the record count of every `Features` family.
//! `cargo run --release --example coverage_sweep -- STEPS_DIR IDS_FILE OUT.jsonl [WORKERS]`.
//! `tools/coverage_sweep_report.py` turns the lines into the MFInstSeg coverage-sweep report.
use std::io::Write;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

fn sweep(path: &std::path::Path) -> serde_json::Value {
    let part = match quiddity::read_step_file(path) {
        Ok(part) => part,
        Err(e) => return serde_json::json!({"status": "unreadable", "message": e.to_string()}),
    };
    let faces = part.faces.len();
    match catch_unwind(AssertUnwindSafe(|| quiddity::features::recognise(&part))) {
        Ok(features) => {
            let mut counts: serde_json::Map<String, serde_json::Value> =
                match serde_json::to_value(&features).expect("features serialise") {
                    serde_json::Value::Object(fields) => fields
                        .into_iter()
                        .filter_map(|(k, v)| v.as_array().map(|a| (k, a.len().into())))
                        .collect(),
                    _ => unreachable!(),
                };
            // Serde skips these two; the census counts the first.
            counts.insert(
                "section_recesses".into(),
                features.section_recesses.len().into(),
            );
            serde_json::json!({
                "status": "ok",
                "faces": faces,
                "counts": counts,
                "section_recess_refusal": features.section_recess_refusal.map(|e| e.to_string()),
            })
        }
        Err(payload) => {
            let message = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default();
            serde_json::json!({"status": "panicked", "faces": faces, "message": message})
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let steps = std::path::PathBuf::from(&args[1]);
    let ids: Vec<String> = std::fs::read_to_string(&args[2])
        .expect("ids")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect();
    let out = Mutex::new(std::fs::File::create(&args[3]).expect("output"));
    let workers = args.get(4).map_or(4, |w| w.parse().expect("workers"));
    std::panic::set_hook(Box::new(|_| {})); // panics are recorded, not printed
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            std::thread::Builder::new()
                .stack_size(256 << 20)
                .spawn_scoped(scope, || {
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(id) = ids.get(i) else { break };
                        let start = Instant::now();
                        let mut line = sweep(&steps.join(format!("{id}.step")));
                        line["id"] = id.as_str().into();
                        line["seconds"] = start.elapsed().as_secs_f64().into();
                        let mut out = out.lock().unwrap();
                        writeln!(out, "{line}").unwrap();
                        out.flush().unwrap();
                        if i.is_multiple_of(1000) {
                            eprintln!("{i}/{}", ids.len());
                        }
                    }
                })
                .unwrap();
        }
    });
}
