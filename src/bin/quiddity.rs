//! `quiddity <file.step>`: recognise a STEP file's features and print them, with their
//! fingerprints, as JSON.
//!
//! `quiddity correspond <old> <new>`: match two revisions and print the correspondence as JSON.
//! Each revision is a recognition result written by `quiddity <file.step>`, or a STEP file
//! (recognised on the spot).
//!
//! Exit status: 0 with the JSON on stdout; 1 with the error on stderr when a file cannot be
//! read (an unreadable, truncated, incomplete or unsupported STEP file is refused, never
//! recognised as empty); 2 with the usage on stderr for bad arguments. `-h`/`--help` prints the
//! usage on stdout and exits 0.

use std::path::Path;
use std::process::ExitCode;

use quiddity::correspondence::{self, Fingerprints};

const USAGE: &str = "usage: quiddity <file.step>\n       quiddity correspond <old.json|old.step> <new.json|new.step>";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [flag] if flag == "-h" || flag == "--help" => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        [command, old, new] if command == "correspond" => correspond(old, new),
        [path] if path != "correspond" => recognise(path),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("quiddity: {error}");
            ExitCode::FAILURE
        }
    }
}

/// A STEP file's part, warning on stderr of geometry that did not resolve: the solids holding
/// it are not valid, so their features are not recognised.
fn read_part(path: &str) -> Result<quiddity::kernel::Part, String> {
    let part = quiddity::read_step_file(Path::new(path)).map_err(|e| format!("{path}: {e}"))?;
    let (faces, edges) = (part.unresolved_faces(), part.unresolved_edges());
    if !faces.is_empty() || !edges.is_empty() {
        eprintln!(
            "quiddity: {path}: warning: geometry did not resolve (faces {faces:?}, edges \
             {edges:?}); the solids holding it are not recognised"
        );
    }
    Ok(part)
}

fn recognise(path: &str) -> Result<String, String> {
    let part = read_part(path)?;
    let recognition = correspondence::recognise(&part);
    Ok(serde_json::to_string_pretty(&recognition).expect("records serialise"))
}

/// A revision's fingerprints: from a recognition result's `fingerprints` (or a bare fingerprints
/// document), or by recognising a STEP file.
fn fingerprints(path: &str) -> Result<Fingerprints, String> {
    let lower = path.to_lowercase();
    let step = [".step", ".stp", ".step.gz", ".stp.gz"]
        .iter()
        .any(|e| lower.ends_with(e));
    if step {
        let part = read_part(path)?;
        return Ok(correspondence::recognise(&part).fingerprints);
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let mut value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{path}: not JSON ({e})"))?;
    if let Some(inner) = value.get_mut("fingerprints") {
        value = inner.take();
    }
    serde_json::from_value(value)
        .map_err(|e| format!("{path}: no fingerprints ({e}); write it with `quiddity <file.step>`"))
}

fn correspond(old: &str, new: &str) -> Result<String, String> {
    let c = correspondence::correspond(&fingerprints(old)?, &fingerprints(new)?)?;
    Ok(serde_json::to_string_pretty(&c).expect("a correspondence serialises"))
}
