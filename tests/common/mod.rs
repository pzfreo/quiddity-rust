//! Shared by the parity tests: calling a ported recogniser by its Python name, and comparing
//! answers with Python's semantics.

#![allow(dead_code)]

use quiddity::Part;
use quiddity::features::chamfers::recognise_chamfers;
use quiddity::features::countersinks::recognise_countersinks;
use quiddity::features::fillets::recognise_fillets;
use quiddity::features::hole_patterns::recognise_hole_patterns;
use quiddity::features::holes::{HoleRecord, recognise_holes};
use quiddity::kernel::geom::SurfaceType;
use serde_json::Value;

/// Structural equality with -0.0 == 0.0, measured values agreeing to
/// one part in a million: below that, the two kernels' parameter-range arithmetic differs in its
/// last digits (sub-micron at part scale). Rounded fields still compare exactly at their grid.
pub fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            // Zero's sign is not compared: a record coordinate of ±0.0 is ~1e-17 of kernel
            // round-off rounded away, and OpenCascade's sign of it is not reproducible. Where a
            // sign could decide something (atan2 in patterns), the port uses Python's exact
            // arithmetic, and the pattern replay agrees even sign-strictly.
            (x - y).abs() <= 1e-6 * x.abs().max(y.abs()).max(1.0)
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        _ => a == b,
    }
}

/// Inventory hints a Python call may pass (precomputed from the same part); the port computes
/// its own, so they are not options.
const HINTS: [&str; 2] = ["cyls", "face_edges"];

/// A recogniser's options from a captured call's keyword arguments; an argument the port does
/// not know is an error, never a silent default.
pub fn options<T: serde::de::DeserializeOwned>(kwargs: &Value) -> T {
    let mut kwargs = kwargs.clone();
    if let Some(map) = kwargs.as_object_mut() {
        map.retain(|k, _| !HINTS.contains(&k.as_str()));
    }
    serde_json::from_value(kwargs.clone()).unwrap_or_else(|e| panic!("options {kwargs}: {e}"))
}

/// The port's answer, as JSON, to a Python recogniser called on a part with these options.
/// Hole patterns over a part mean "patterns of the part's holes with countersinks composed".
pub fn recognise(function: &str, part: &Part, kwargs: &Value) -> Value {
    match function {
        "recognise_fillets" => json(recognise_fillets(part, &options(kwargs))),
        "recognise_chamfers" => json(recognise_chamfers(part, &options(kwargs))),
        "recognise_countersinks" => json(recognise_countersinks(part)),
        "recognise_holes" => json(recognise_holes(part, &options(kwargs))),
        // Over a part: the patterns among the part's holes found with these hole options.
        "recognise_hole_patterns" => json(recognise_hole_patterns(&recognise_holes(
            part,
            &options(kwargs),
        ))),
        other => panic!("{other} is not ported"),
    }
}

/// The defining faces (sorted) of each occurrence the port's evidence path publishes, or `Err`
/// when it refuses, for a Python recogniser called on a part with these options.
pub fn defining(function: &str, part: &Part, kwargs: &Value) -> Result<Vec<Vec<usize>>, String> {
    use quiddity::features::{Context, Occurrence, chamfers, countersinks, fillets, holes};
    fn faces<R>(found: Vec<Occurrence<R>>) -> Vec<Vec<usize>> {
        found
            .into_iter()
            .map(|o| {
                let mut d = o.defining;
                d.sort_unstable();
                d
            })
            .collect()
    }
    let ctx = Context::new(part);
    let result = match function {
        "recognise_fillets" => fillets::discover_verified(&ctx, &options(kwargs)).map(faces),
        "recognise_chamfers" => chamfers::discover_verified(&ctx, &options(kwargs)).map(faces),
        "recognise_countersinks" => countersinks::discover_verified(&ctx).map(faces),
        "recognise_holes" => {
            let opts: quiddity::HoleOptions = options(kwargs);
            let seats = if opts.with_countersinks {
                countersinks::discover(&ctx)
            } else {
                vec![]
            };
            holes::discover_verified(&ctx, &seats).map(faces)
        }
        other => panic!("{other} has no evidence path"),
    };
    result.map_err(|e| e.to_string())
}

fn json<T: serde::Serialize>(value: T) -> Value {
    serde_json::to_value(value).unwrap()
}

/// The port's answer to a recogniser called on records rather than a part.
pub fn recognise_records(function: &str, arguments: &Value) -> Value {
    match function {
        "recognise_hole_patterns" => {
            let holes: Vec<HoleRecord> = serde_json::from_value(arguments[0].clone()).unwrap();
            json(recognise_hole_patterns(&holes))
        }
        other => panic!("{other} takes a part"),
    }
}

fn type_name(t: SurfaceType) -> &'static str {
    match t {
        SurfaceType::Plane => "PLANE",
        SurfaceType::Cylinder => "CYLINDER",
        SurfaceType::Cone => "CONE",
        SurfaceType::Sphere => "SPHERE",
        SurfaceType::Torus => "TORUS",
        SurfaceType::Freeform | SurfaceType::Other => "OTHER",
    }
}

/// The reader must walk faces in OpenCascade's order with the same surfaces and extents. The
/// extents are a fingerprint, not a parity target: OpenCascade's optimal boxes overshoot curved
/// B-spline boundaries by a few microns, hence the 5e-3 band.
pub fn check_inventory(name: &str, part: &Part, inventory: &[Value], problems: &mut Vec<String>) {
    if part.faces.len() != inventory.len() {
        problems.push(format!(
            "{name}: {} faces, Python has {}",
            part.faces.len(),
            inventory.len()
        ));
        return;
    }
    for (i, expected) in inventory.iter().enumerate() {
        let want = expected["type"].as_str().unwrap();
        let got = type_name(part.faces[i].surface.kind());
        if got != want
            && !(got == "OTHER"
                && !["PLANE", "CYLINDER", "CONE", "SPHERE", "TORUS"].contains(&want))
        {
            problems.push(format!("{name}: face {i} is {got}, Python has {want}"));
            return;
        }
        let edges = part.face_edges(i).len() as u64;
        if Some(edges) != expected["edges"].as_u64() {
            problems.push(format!(
                "{name}: face {i} has {edges} edges, Python has {}",
                expected["edges"]
            ));
            return;
        }
        let b = part.face_bounds(i);
        let close = |got: [f64; 3], want: &Value| {
            (0..3).all(|k| (got[k] - want[k].as_f64().unwrap()).abs() <= 5e-3 + 1e-6 * got[k].abs())
        };
        // Sphere patches through a pole get approximate interior extremes (see README).
        let fingerprinted = got != "OTHER" && got != "SPHERE";
        if fingerprinted && (!close(b.min, &expected["min"]) || !close(b.max, &expected["max"])) {
            problems.push(format!(
                "{name}: face {i} ({got}) bounds {:?}..{:?}, Python {}..{}",
                b.min, b.max, expected["min"], expected["max"]
            ));
            return;
        }
    }
}

/// The records only one side has, and — when both sides have the same count — the fields that
/// differ between records in the same position, for failure messages precise enough to pin a
/// known divergence to.
pub fn diff(got: &Value, want: &Value) -> String {
    let (Some(g), Some(w)) = (got.as_array(), want.as_array()) else {
        return format!("\n  rust   {got}\n  python {want}");
    };
    let only = |a: &Vec<Value>, b: &Vec<Value>| -> Vec<String> {
        a.iter()
            .filter(|x| !b.iter().any(|y| same(x, y)))
            .map(|x| x.to_string())
            .collect()
    };
    let mut fields = std::collections::BTreeSet::new();
    if g.len() == w.len() {
        for (x, y) in g.iter().zip(w) {
            if let (Some(x), Some(y)) = (x.as_object(), y.as_object()) {
                fields.extend(
                    x.keys()
                        .filter(|k| !y.get(*k).is_some_and(|v| same(&x[*k], v)))
                        .cloned(),
                );
            }
        }
    }
    format!(
        "{} vs {} records, fields {fields:?}\n  only rust   {}\n  only python {}",
        g.len(),
        w.len(),
        only(g, w).join("\n              "),
        only(w, g).join("\n              ")
    )
}
