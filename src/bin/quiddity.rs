//! `quiddity <file.step>`: recognise a STEP file's features and print them, with their
//! fingerprints, as JSON.
//!
//! `quiddity correspond <old> <new>`: match two revisions and print the correspondence as JSON.
//! Each revision is a recognition result written by `quiddity <file.step>`, or a STEP file
//! (recognised on the spot).
//!
//! `quiddity parts <file.step>`: the file's distinct parts (index, name, face and edge counts,
//! placements) and the binding (the STEP text's sha256 and the PMI reader version) that face
//! and edge numbers refer to.
//!
//! `quiddity pmi read <file.step> [--part N]`: the semantic PMI of every part (or part N) and
//! the reader's findings, in the versioned JSON form of `quiddity::pmi_json`.
//!
//! `quiddity pmi check <file.step> <pmi.json>`: decodes a PMI document and checks it against
//! the file (binding, part indices and names, face and edge numbers, the model's invariants);
//! prints the document in its canonical form, or refuses naming the JSON path.
//!
//! Exit status: 0 with the JSON on stdout; 1 with the error on stderr when a file cannot be
//! read (an unreadable, truncated, incomplete or unsupported STEP file is refused, never
//! recognised as empty) or a PMI document is refused; 2 with the usage on stderr for bad
//! arguments. `-h`/`--help` prints the usage on stdout and exits 0.

use std::path::Path;
use std::process::ExitCode;

use quiddity::correspondence::{self, Fingerprints};
use quiddity::kernel::p21;
use quiddity::kernel::step::{PartDefinition, read_part_definitions};
use quiddity::pmi_json::{self, Binding};

const USAGE: &str = "usage: quiddity <file.step>
       quiddity correspond <old.json|old.step> <new.json|new.step>
       quiddity parts <file.step>
       quiddity pmi read <file.step> [--part N]
       quiddity pmi check <file.step> <pmi.json>";

const COMMANDS: [&str; 3] = ["correspond", "parts", "pmi"];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [flag] if flag == "-h" || flag == "--help" => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        [command, old, new] if command == "correspond" => correspond(old, new),
        [command, path] if command == "parts" => parts(path),
        [command, read, path] if command == "pmi" && read == "read" => pmi_read(path, None),
        [command, read, path, flag, n]
            if command == "pmi" && read == "read" && flag == "--part" =>
        {
            match n.parse::<usize>() {
                Ok(n) => pmi_read(path, Some(n)),
                Err(_) => {
                    eprintln!("quiddity: --part takes a part index, not {n:?}\n{USAGE}");
                    return ExitCode::from(2);
                }
            }
        }
        [command, check, path, json] if command == "pmi" && check == "check" => {
            pmi_check(path, json)
        }
        [path] if !COMMANDS.contains(&path.as_str()) => recognise(path),
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

/// A STEP file's text, gunzipped when the name ends in `.gz`.
fn step_bytes(path: &str) -> Result<Vec<u8>, String> {
    let raw = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    if !path.to_lowercase().ends_with(".gz") {
        return Ok(raw);
    }
    let mut out = Vec::new();
    std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(&raw[..]), &mut out)
        .map_err(|e| format!("{path}: not gzip ({e})"))?;
    Ok(out)
}

/// A STEP file's text and its distinct parts.
fn part_definitions(path: &str) -> Result<(Vec<u8>, Vec<PartDefinition>), String> {
    let bytes = step_bytes(path)?;
    let defs = read_part_definitions(&bytes).map_err(|e| format!("{path}: {e}"))?;
    Ok((bytes, defs))
}

fn parts(path: &str) -> Result<String, String> {
    let (bytes, defs) = part_definitions(path)?;
    let json = pmi_json::parts_json(&defs, &Binding::of(&bytes));
    Ok(serde_json::to_string_pretty(&json).expect("JSON serialises"))
}

fn pmi_read(path: &str, part: Option<usize>) -> Result<String, String> {
    let (bytes, defs) = part_definitions(path)?;
    if let Some(n) = part
        && n >= defs.len()
    {
        return Err(format!(
            "{path}: the file has {} parts (0 to {}); there is no part {n}",
            defs.len(),
            defs.len().saturating_sub(1)
        ));
    }
    let binding = Binding::of(&bytes);
    let doc = p21::Document::parse(bytes).map_err(|e| format!("{path}: {e}"))?;
    let read = quiddity::kernel::pmi::read(&doc, &defs).map_err(|e| format!("{path}: {e}"))?;
    let document = pmi_json::read_document(&read, &defs, binding, part);
    Ok(
        serde_json::to_string_pretty(&pmi_json::document_to_json(&document))
            .expect("JSON serialises"),
    )
}

fn pmi_check(path: &str, json: &str) -> Result<String, String> {
    let (bytes, defs) = part_definitions(path)?;
    let text = std::fs::read_to_string(json).map_err(|e| format!("{json}: {e}"))?;
    let value = pmi_json::parse(&text).map_err(|e| format!("{json}: not JSON ({e})"))?;
    let document = pmi_json::document_from_json(&value).map_err(|e| format!("{json}: {e}"))?;
    pmi_json::check_against(&document, &bytes, &defs).map_err(|e| format!("{json}: {e}"))?;
    Ok(
        serde_json::to_string_pretty(&pmi_json::document_to_json(&document))
            .expect("JSON serialises"),
    )
}
