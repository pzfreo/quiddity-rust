//! `quiddity <file.step>`: recognise a STEP file's features and print them as JSON: each
//! family's records at the top level, their `fingerprints`, and the versioned recognition
//! `document` (`quiddity::recognition`: per feature its family, Python record type, defining
//! faces and record). The part is recognised in its own frame and the records reported in the
//! file's coordinates; the document gives the frame and, per record, the fields no file axis
//! corresponds to (left in the frame). A part with no frame (no plane or cylinder, no material)
//! is recognised as placed in the file, with a warning on stderr.
//!
//! `quiddity correspond <old> <new>`: match two revisions and print the correspondence as JSON.
//! Each revision is a recognition result written by `quiddity <file.step>`, or a STEP file
//! (recognised on the spot).
//!
//! Exit status: 0 with the JSON on stdout; 1 with the error on stderr when a file cannot be
//! read (an unreadable, truncated, incomplete or unsupported STEP file is refused, never
//! recognised as empty); 2 with the usage on stderr for bad arguments. `-h`/`--help` prints the
//! usage on stdout and exits 0.
//!
//! `quiddity serve`: recognition and correspondence as a JSON-lines service on stdin and stdout
//! (`quiddity::serve` has the protocol); exits 0 at the end of the input.

use std::process::ExitCode;

use quiddity::serve::{self, Failure};

const USAGE: &str = "usage: quiddity <file.step>\n       quiddity correspond <old.json|old.step> <new.json|new.step>\n       quiddity serve\n\n`quiddity <file.step>` prints JSON: each family's records, their `fingerprints`, and the\nversioned recognition `document`. `quiddity serve` answers JSON-lines requests on stdin\n(`recognise`, `correspond`), one response line each on stdout.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [flag] if flag == "-h" || flag == "--help" => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        [command] if command == "serve" => {
            return match serve::serve(std::io::stdin().lock(), std::io::stdout().lock()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("quiddity: serve: {error}");
                    ExitCode::FAILURE
                }
            };
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

/// Warnings, on stderr: geometry that did not resolve, a part frame that could not be inferred.
fn warn(warnings: Vec<String>) {
    for warning in warnings {
        eprintln!("quiddity: {warning}");
    }
}

fn recognise(path: &str) -> Result<String, Failure> {
    let (output, warning) = serve::recognise_file(path)?;
    warn(warning);
    Ok(serde_json::to_string_pretty(&output).expect("records serialise"))
}

fn correspond(old: &str, new: &str) -> Result<String, Failure> {
    let revision = |path| {
        let (fingerprints, warning) = serve::fingerprints_file(path)?;
        warn(warning);
        Ok::<_, Failure>(fingerprints)
    };
    let c = serve::correspond(&revision(old)?, &revision(new)?)?;
    Ok(serde_json::to_string_pretty(&c).expect("a correspondence serialises"))
}
