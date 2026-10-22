//! Rust-owned expected results. Only tools/rust_snapshot.py regenerates the committed record.
use serde_json::Value;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub fn check(root: &Path, name: &str, actual: &Value) {
    check_with(root, name, actual, &same);
}

pub fn check_with(root: &Path, name: &str, actual: &Value, equal: &dyn Fn(&Value, &Value) -> bool) {
    assert!(!name.contains(".."));
    let regenerate = std::env::var_os("QUIDDITY_SNAPSHOT_DIR");
    if regenerate.is_some() {
        assert_eq!(
            std::env::var("QUIDDITY_SNAPSHOT_REGENERATE").as_deref(),
            Ok("1"),
            "snapshot writing requires explicit QUIDDITY_SNAPSHOT_REGENERATE=1"
        );
        assert!(
            std::env::var_os("CI").is_none(),
            "snapshot regeneration is forbidden in CI"
        );
    }
    if let Some(dir) = regenerate {
        let path = PathBuf::from(dir).join(format!("{name}.json.gz"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut out = flate2::GzBuilder::new().mtime(0).write(
            std::fs::File::create(path).unwrap(),
            flate2::Compression::best(),
        );
        out.write_all(&serde_json::to_vec_pretty(actual).unwrap())
            .unwrap();
        out.finish().unwrap();
        return;
    }
    let path = root.join("rust-snapshot").join(format!("{name}.json.gz"));
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(std::fs::File::open(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e}; regenerate with tools/rust_snapshot.py",
            path.display()
        )
    }))
    .read_to_end(&mut bytes)
    .unwrap();
    let expected: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        equal(actual, &expected),
        "Rust snapshot {name} changed. Regenerate with tools/rust_snapshot.py and review the decompressed diff.\n{}",
        first_difference(actual, &expected, "")
    );
    // --check records only after the ordinary comparison succeeded; never an acceptance mode.
    if let Some(dir) = std::env::var_os("QUIDDITY_SNAPSHOT_VERIFY_DIR") {
        let path = PathBuf::from(dir).join(format!("{name}.json.gz"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut out = flate2::GzBuilder::new().mtime(0).write(
            std::fs::File::create(path).unwrap(),
            flate2::Compression::best(),
        );
        out.write_all(&serde_json::to_vec_pretty(actual).unwrap())
            .unwrap();
        out.finish().unwrap();
    }
}

/// Existing record tolerance: structural equality, numeric abs/relative tolerance 1e-6.
pub fn same(a: &Value, b: &Value) -> bool {
    close(a, b, 1e-6)
}
pub fn close(a: &Value, b: &Value, tolerance: f64) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            (x - y).abs() <= tolerance * x.abs().max(y.abs()).max(1.0)
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| close(a, b, tolerance))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, a)| y.get(k).is_some_and(|b| close(a, b, tolerance)))
        }
        _ => a == b,
    }
}
fn first_difference(a: &Value, b: &Value, at: &str) -> String {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) if x.keys().eq(y.keys()) => {
            x.iter().find(|(k, a)| !same(a, &y[*k])).map_or_else(
                || format!("{at}: comparison changed"),
                |(k, a)| first_difference(a, &y[k], &format!("{at}/{k}")),
            )
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => x
            .iter()
            .zip(y)
            .enumerate()
            .find(|(_, (a, b))| !same(a, b))
            .map_or_else(
                || format!("{at}: comparison changed"),
                |(i, (a, b))| first_difference(a, b, &format!("{at}/{i}")),
            ),
        _ => format!("{at}: actual {a}, expected {b}"),
    }
}
