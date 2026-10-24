//! Compare geometric measurements rather than a basis's gauge or a body key's box orientation.
use quiddity::kernel::geom::{V3, dot};
use serde_json::{Value, json};
fn vector(v: &Value) -> Option<V3> {
    match v.as_str() {
        Some("x") => Some([1., 0., 0.]),
        Some("y") => Some([0., 1., 0.]),
        Some("z") => Some([0., 0., 1.]),
        _ => serde_json::from_value(v.clone()).ok(),
    }
}
fn point(v: &Value) -> V3 {
    serde_json::from_value(v.clone()).unwrap()
}
fn add(o: V3, d: V3, s: f64) -> V3 {
    std::array::from_fn(|i| o[i] + d[i] * s)
}
fn positive(d: V3) -> (V3, f64) {
    let w = (0..3)
        .max_by(|&a, &b| d[a].abs().total_cmp(&d[b].abs()))
        .unwrap();
    let s = 1_f64.copysign(d[w]);
    (d.map(|v| v * s), s)
}
fn plane(d: V3, o: V3, s: f64) -> Value {
    let (n, sign) = positive(d);
    json!({"normal":n,"offset":(dot(o,d)+s)*sign})
}
fn order(v: &mut [Value]) {
    fn key(v: &Value) -> String {
        match v {
            Value::Number(n) => {
                format!("{:024.6}", (n.as_f64().unwrap() * 1e6).round() / 1e6 + 0.0)
            }
            Value::Array(a) => a.iter().map(key).collect::<Vec<_>>().join(","),
            Value::Object(o) => o
                .iter()
                .map(|(k, v)| format!("{k}:{}", key(v)))
                .collect::<Vec<_>>()
                .join(","),
            _ => v.to_string(),
        }
    }
    v.sort_by_cached_key(key);
}

pub fn canonical(v: &Value, family: &str) -> Value {
    let Value::Object(raw) = v else {
        return v.clone();
    };
    let mut r = raw.clone();
    let o = raw.get("origin").map(point).unwrap_or([0.; 3]);
    let axis = raw.get("axis").and_then(vector);
    // Boxes are a geometry-derived body identity, not a feature's outline. Their independent
    // coordinate reconstruction is checked on every published box by vector_publication.
    for key in ["body_key", "body_bounds"] {
        if let Some(b) = r.remove(key)
            && key == "body_key"
            && !b.is_null()
        {
            let mass = if b.is_array() {
                json!([b[6], b[7]])
            } else {
                json!([b["volume"], b["area"]])
            };
            r.insert(key.into(), mass);
        }
    }
    if let Some(p) = r.get_mut("profile") {
        *p = canonical(p, "profile");
    }
    if family == "profile"
        && let Some(d) = axis
        && let Some(p) = raw.get("axis_origin")
    {
        let p = point(p);
        r.insert("axis_origin".into(), json!(add(p, d, -dot(p, d))));
    }
    if family == "flats"
        && let Some(d) = axis
    {
        let basis: Vec<V3> = raw
            .get("section_axes")
            .map(|v| {
                v.as_array()
                    .unwrap()
                    .iter()
                    .map(|v| vector(v).unwrap())
                    .collect()
            })
            .unwrap_or_else(|| {
                (0..3)
                    .map(|i| {
                        let mut a = [0.; 3];
                        a[i] = 1.;
                        a
                    })
                    .filter(|a| dot(*a, d).abs() < 0.5)
                    .collect()
            });
        let p = raw["axis_line"].as_array().unwrap();
        let q = add(
            add(o, basis[0], p[0].as_f64().unwrap()),
            basis[1],
            p[1].as_f64().unwrap(),
        );
        r.insert("axis_line".into(), json!(add(q, d, -dot(q, d))));
        r.remove("section_axes");
        r.remove("origin");
    }
    if [
        "turned_steps",
        "plates",
        "polygonal_stock",
        "polygonal_bosses",
        "repeating_radial_profiles",
    ]
    .contains(&family)
        && let Some(d) = axis
    {
        for (lo, hi) in [("lo", "hi"), ("base", "top")] {
            if let (Some(a), Some(b)) = (raw.get(lo), raw.get(hi)) {
                let mut planes = vec![
                    plane(d, o, a.as_f64().unwrap()),
                    plane(d, o, b.as_f64().unwrap()),
                ];
                order(&mut planes);
                r.remove(lo);
                r.remove(hi);
                r.insert("stations".into(), json!(planes));
            }
        }
        r.remove("origin");
    }
    if family == "plates"
        && let Some(d) = axis
    {
        let axes: Vec<V3> = raw
            .get("section_axes")
            .map(|v| {
                v.as_array()
                    .unwrap()
                    .iter()
                    .map(|v| vector(v).unwrap())
                    .collect()
            })
            .unwrap_or_else(|| {
                (0..3)
                    .map(|i| {
                        let mut a = [0.; 3];
                        a[i] = 1.;
                        a
                    })
                    .filter(|a| dot(*a, d).abs() < 0.5)
                    .collect()
            });
        let q = add(
            add(o, axes[0], raw["u"].as_f64().unwrap()),
            axes[1],
            raw["v"].as_f64().unwrap(),
        );
        r.insert("transverse_center".into(), json!(add(q, d, -dot(q, d))));
        for k in ["u", "v", "section_axes"] {
            r.remove(k);
        }
    }
    if family == "risers" {
        for (key, a) in [("positions", "axis"), ("other_positions", "other_axis")] {
            if let Some(values) = raw.get(key).and_then(Value::as_array) {
                let d = vector(&raw[a]).unwrap();
                let mut planes: Vec<_> = values
                    .iter()
                    .map(|v| plane(d, o, v.as_f64().unwrap()))
                    .collect();
                order(&mut planes);
                r.insert(key.into(), json!(planes));
            }
        }
        let h = raw
            .get("height_axis")
            .and_then(vector)
            .unwrap_or([0., 0., 1.]);
        let mut ends:Vec<_>=[("z_lo","lo_at_envelope"),("z_hi","hi_at_envelope")].into_iter().map(|(s,flag)|json!({"plane":plane(h,o,raw[s].as_f64().unwrap()),"envelope":raw[flag]})).collect();
        order(&mut ends);
        for k in [
            "z_lo",
            "z_hi",
            "lo_at_envelope",
            "hi_at_envelope",
            "origin",
            "height_axis",
        ] {
            r.remove(k);
        }
        r.insert("height_ends".into(), json!(ends));
    }
    if family == "step_levels" {
        let h = raw
            .get("height_axis")
            .and_then(vector)
            .unwrap_or([0., 0., 1.]);
        let x = raw.get("x_axis").and_then(vector).unwrap_or([1., 0., 0.]);
        let y = raw.get("y_axis").and_then(vector).unwrap_or([0., 1., 0.]);
        let z = raw["z"].as_f64().unwrap();
        let mut corners = Vec::new();
        if let (Some(xs), Some(ys)) = (raw["x_span"].as_array(), raw["y_span"].as_array()) {
            for a in xs {
                for b in ys {
                    corners.push(json!(add(
                        add(add(o, h, z), x, a.as_f64().unwrap()),
                        y,
                        b.as_f64().unwrap()
                    )));
                }
            }
        }
        order(&mut corners);
        for k in [
            "z",
            "x_span",
            "y_span",
            "origin",
            "height_axis",
            "x_axis",
            "y_axis",
        ] {
            r.remove(k);
        }
        r.insert("plane".into(), plane(h, o, z));
        r.insert("corners".into(), json!(corners));
    }
    if let Some(levels) = r.get_mut("body_levels").and_then(Value::as_array_mut) {
        for level in levels.iter_mut() {
            *level = canonical(level, "step_levels");
        }
        order(levels);
    }
    if family == "paired_ramp_steps"
        && let Some(d) = raw.get("width_axis").and_then(vector)
    {
        let (d, sign) = positive(d);
        r.insert("width_axis".into(), json!(d));
        if sign < 0.
            && let Some(pair) = r.get_mut("half_widths").and_then(Value::as_array_mut)
        {
            pair.reverse();
        }
    }
    if let Some(d) = axis {
        r.insert("axis".into(), json!(positive(d).0));
    }
    if let Some(d) = raw.get("other_axis").and_then(vector) {
        r.insert("other_axis".into(), json!(positive(d).0));
    }
    Value::Object(r)
}
