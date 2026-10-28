//! File-format dispatch shared by the command line and JSON-lines service.

use std::path::Path;

/// Whether the extension names a Fusion archive, case-insensitively.
pub fn is_f3d(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("f3d"))
}

/// Convert a Fusion byte snapshot, or explain how to enable the optional reader.
pub fn fusion_step(bytes: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    #[cfg(feature = "f3d")]
    {
        crate::kernel::f3d::to_step(bytes)
    }
    #[cfg(not(feature = "f3d"))]
    {
        let _ = bytes;
        Err(std::io::Error::other("F3D input requires the `f3d` feature; install with `cargo install quiddity --features f3d` (or build with `--features f3d`)").into())
    }
}

/// Conversion provenance, carried in service responses and printed on the CLI's stderr.
pub fn fusion_warning(path: &str) -> String {
    format!("{path}: warning: converted from Fusion geometry (visible solids only; no PMI)")
}
