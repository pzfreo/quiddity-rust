//! `quiddity <file.step>`: recognise a STEP file's features and print them as JSON.

use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: quiddity <file.step>");
        return ExitCode::from(2);
    };
    let part = match quiddity::read_step_file(std::path::Path::new(&path)) {
        Ok(part) => part,
        Err(error) => {
            eprintln!("quiddity: {path}: {error}");
            return ExitCode::FAILURE;
        }
    };
    let features = quiddity::features::recognise(&part);
    println!(
        "{}",
        serde_json::to_string_pretty(&features).expect("records serialise")
    );
    ExitCode::SUCCESS
}
