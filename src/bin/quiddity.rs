//! The Rust command line, also used by the Python console script.
fn main() -> std::process::ExitCode {
    std::process::ExitCode::from(quiddity::cli::main(
        &std::env::args().skip(1).collect::<Vec<_>>(),
    ))
}
