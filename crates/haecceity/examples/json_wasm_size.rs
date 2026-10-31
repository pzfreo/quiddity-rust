//! Size probe, not a public FFI: input is UTF-8 STEP (or JSON for import).
//! Build each feature with --target wasm32-unknown-unknown --release --example json_wasm_size.
use haecceity::{json, p21::Document, pmi, step::read_part_definitions};

/// Return the encoded companion sizes; zero on invalid input.
///
/// # Safety
/// `data` must point to `len` readable bytes throughout the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn export_json(data: *const u8, len: usize) -> usize {
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    let run = || -> Option<usize> {
        let doc = Document::parse(bytes.to_vec()).ok()?;
        let parts = read_part_definitions(bytes).ok()?;
        let read = pmi::read(&doc, &parts).ok()?;
        let semantic =
            json::identify(read.parts.first()?.clone(), read.identifiers.parts.first()).ok()?;
        let drawn = json::presentation::read(&doc, &read, pmi::PartId(0), &semantic).ok()?;
        Some(
            serde_json::to_vec(&json::to_value(&semantic).ok()?)
                .ok()?
                .len()
                + serde_json::to_vec(&drawn).ok()?.len(),
        )
    };
    run().unwrap_or(0)
}

/// Return written STEP size; zero on invalid input. Keeps the complete importer reachable.
///
/// # Safety
/// Both pointer/length pairs must describe readable bytes throughout the call.
#[cfg(feature = "json-import")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn import_json(
    step: *const u8,
    step_len: usize,
    data: *const u8,
    len: usize,
) -> usize {
    let step = unsafe { std::slice::from_raw_parts(step, step_len) };
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    let run = || -> Option<usize> {
        let doc = Document::parse(step.to_vec()).ok()?;
        let parts = read_part_definitions(step).ok()?;
        let value = json::parse(std::str::from_utf8(bytes).ok()?).ok()?;
        json::validate(&value).ok()?;
        let semantic = json::from_value(&value, &json::Anchors::read(&doc, parts.first()?)).ok()?;
        let (written, _) = json::write(
            &doc,
            &parts,
            &[(pmi::PartId(0), semantic)],
            pmi::write::PresentationPolicy::RemovePresentation,
        )
        .ok()?;
        Some(written.len())
    };
    run().unwrap_or(0)
}
