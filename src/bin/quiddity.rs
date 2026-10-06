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
    let fillets = quiddity::recognise_fillets(&part, &quiddity::FilletOptions::default());
    let document = serde_json::json!({ "fillets": fillets });
    println!(
        "{}",
        serde_json::to_string_pretty(&document).expect("records serialise")
    );
    ExitCode::SUCCESS
}
