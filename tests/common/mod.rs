//! Shared by the parity tests: calling a ported recogniser by its Python name, and comparing
//! answers with Python's semantics.

#![allow(dead_code)]

use quiddity::Part;
use quiddity::features::chamfers::{ChamferOptions, recognise_chamfers};
use quiddity::features::countersinks::recognise_countersinks;
use quiddity::features::fillets::{FilletOptions, recognise_fillets};
use quiddity::features::hole_patterns::recognise_hole_patterns;
use quiddity::features::holes::{HoleOptions, HoleRecord, recognise_holes};
use quiddity::kernel::geom::SurfaceType;
use serde_json::Value;

/// Structural equality with Python's float semantics (-0.0 == 0.0), measured values agreeing to
/// one part in a million: below that, the two kernels' parameter-range arithmetic differs in its
/// last digits (sub-micron at part scale). Rounded fields still compare exactly at their grid.
pub fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
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

pub fn fillet_options(o: &Value) -> FilletOptions {
    let mut opts = FilletOptions::default();
    if let Some(m) = o.get("min_radius").and_then(Value::as_f64) {
        opts.min_radius = Some(m);
    }
    if let Some(f) = o.get("max_radius_frac").and_then(Value::as_f64) {
        opts.max_radius_frac = f;
    }
    if let Some(c) = o.get("include_cylindrical").and_then(Value::as_bool) {
        opts.include_cylindrical = c;
    }
    opts
}

fn hole_options(o: &Value) -> HoleOptions {
    HoleOptions {
        with_countersinks: o.get("csinks").is_some_and(|c| c == "auto"),
    }
}

/// The port's answer, as JSON, to a Python recogniser called on a part with these options.
/// Hole patterns over a part mean "patterns of the part's holes with countersinks composed".
pub fn recognise(function: &str, part: &Part, options: &Value) -> Value {
    match function {
        "recognise_fillets" => json(recognise_fillets(part, &fillet_options(options))),
        "recognise_countersinks" => json(recognise_countersinks(part)),
        "recognise_chamfers" => {
            let mut opts = ChamferOptions::default();
            if let Some(t) = options.get("tol").and_then(Value::as_f64) {
                opts.tol = Some(t);
            }
            if let Some(f) = options.get("max_leg_frac").and_then(Value::as_f64) {
                opts.max_leg_frac = f;
            }
            if let Some(p) = options.get("include_planar").and_then(Value::as_bool) {
                opts.include_planar = p;
            }
            json(recognise_chamfers(part, &opts))
        }
        "recognise_holes" => json(recognise_holes(part, &hole_options(options))),
        "recognise_hole_patterns" => {
            let holes = recognise_holes(
                part,
                &HoleOptions {
                    with_countersinks: true,
                },
            );
            json(recognise_hole_patterns(&holes))
        }
        other => panic!("{other} is not ported"),
    }
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

/// The records only one side has, for readable failure messages.
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
    format!(
        "{} vs {} records\n  only rust   {}\n  only python {}",
        g.len(),
        w.len(),
        only(g, w).join("\n              "),
        only(w, g).join("\n              ")
    )
}
