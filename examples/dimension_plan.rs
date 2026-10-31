//! JSON requirement set: cargo run --release --example dimension_plan -- part.step
use quiddity::dimensioning::{Input, Policy, plan};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os().nth(1).ok_or("expected STEP path")?;
    let part = quiddity::read_step_file(std::path::Path::new(&path))?;
    let input = Input::from_recognition(&part, vec![], Policy::default())?;
    let (required, conveyed) = plan(&input);
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"input": input, "required": required, "conveyed": conveyed})
        )?
    );
    Ok(())
}
