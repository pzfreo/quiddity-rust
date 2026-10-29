//! Dump raw and published records for the #123 before/after audit. Paths on stdin.
#[path = "../src/engine.rs"]
mod engine;
pub use engine::*;
extern crate self as quiddity;
use std::io::{BufRead, Write};
fn main() {
    if std::env::args().any(|a| a == "--supports") {
        for path in std::io::stdin().lock().lines().map(Result::unwrap) {
            let p = read_step_file(std::path::Path::new(&path)).unwrap();
            let faces: Vec<_> = p.faces.iter().enumerate().map(|(i,f)| {
                let (axis, origin) = match f.surface {
                    kernel::geom::Surface::Plane { frame } | kernel::geom::Surface::Cylinder { frame, .. }
                    | kernel::geom::Surface::Cone { frame, .. } | kernel::geom::Surface::Torus { frame, .. }
                    | kernel::geom::Surface::Sphere { frame, .. } => (Some(frame.z), Some(frame.origin)),
                    _ => (None, None),
                };
                serde_json::json!({"face":i,"solid":f.solid,"kind":format!("{:?}",f.surface.kind()),"axis":axis,"origin":origin})
            }).collect();
            let valid: Vec<_> = (0..p.solids.len())
                .map(|s| p.solid_is_geometrically_valid(s))
                .collect();
            println!(
                "{}",
                serde_json::json!({"path":path,"valid":valid,"faces":faces})
            );
        }
        return;
    }

    let paths: Vec<_> = std::io::stdin()
        .lock()
        .lines()
        .map(Result::unwrap)
        .collect();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let out = std::sync::Mutex::new(std::io::stdout());
    std::thread::scope(|scope| {
        for _ in 0..4 {
            std::thread::Builder::new()
                .stack_size(128 << 20)
                .spawn_scoped(scope, || {
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(path) = paths.get(i) else { break };
                        let result = std::panic::catch_unwind(|| {
                            let part = read_step_file(std::path::Path::new(path))
                                .map_err(|e| e.to_string())?;
                            let f = features::recognise(&part);
                            let raw = serde_json::json!({"features": f, "defining": f.defining});
                            let published = serve::recognise_step(path, &kernel::step::IDENTITY)
                                .map(|(p, r, _)| recognition::document(&r, p.faces.len()))
                                .map_err(|e| e.to_string());
                            Ok::<_, String>(serde_json::json!({"raw": raw, "document": published}))
                        });
                        let row = match result {
                            Ok(Ok(value)) => serde_json::json!({"path": path, "value": value}),
                            Ok(Err(error)) => serde_json::json!({"path": path, "error": error}),
                            Err(_) => serde_json::json!({"path": path, "error": "panic"}),
                        };
                        writeln!(out.lock().unwrap(), "{row}").unwrap();
                    }
                })
                .unwrap();
        }
    });
}
