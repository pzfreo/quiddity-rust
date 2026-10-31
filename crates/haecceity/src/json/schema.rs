//! Validator for the vocabulary used by the two shipped 2020-12 schemas.
//! No run-time dependency on a general-purpose JSON Schema engine.
use super::{Error, Result};
use serde_json::Value;

pub(super) fn check(v: &Value, s: &Value, root: &Value, path: &str) -> Result<()> {
    let fail = |why: &str| Err(Error(format!("{path}: schema violation: {why}")));
    if let Some(r) = s["$ref"].as_str() {
        let target = root
            .pointer(r.trim_start_matches('#'))
            .ok_or_else(|| Error(format!("invalid schema reference {r}")))?;
        return check(v, target, root, path);
    }
    for key in ["oneOf", "anyOf"] {
        if let Some(branches) = s[key].as_array() {
            let matches = branches
                .iter()
                .filter(|b| check(v, b, root, path).is_ok())
                .count();
            if matches == 0 || (key == "oneOf" && matches != 1) {
                return fail("value matches no unique allowed alternative");
            }
        }
    }
    if let Some(c) = s.get("const")
        && !equal(v, c)
    {
        return fail("wrong constant or schema version");
    }
    if let Some(e) = s["enum"].as_array()
        && !e.iter().any(|candidate| equal(v, candidate))
    {
        return fail("unknown enumeration value");
    }
    if let Some(t) = s["type"].as_str() {
        let ok = match t {
            "object" => v.is_object(),
            "array" => v.is_array(),
            "string" => v.is_string(),
            "integer" => v.is_i64() || v.is_u64() || v.as_f64().is_some_and(|n| n.fract() == 0.),
            "number" => v.is_number(),
            "boolean" => v.is_boolean(),
            "null" => v.is_null(),
            _ => false,
        };
        if !ok {
            return fail(&format!("expected {t}"));
        }
    }
    if let Some(n) = v.as_f64() {
        if let Some(min) = s["minimum"].as_f64()
            && n < min
        {
            return fail("below minimum");
        }
        if let Some(max) = s["maximum"].as_f64()
            && n > max
        {
            return fail("above maximum");
        }
    }
    if let Some(t) = v.as_str() {
        if let Some(n) = s["minLength"].as_u64()
            && t.chars().count() < n as usize
        {
            return fail("string too short");
        }
        if let Some(p) = s["pattern"].as_str() {
            // These are the only two patterns in our schemas; evaluate with the model's
            // exact lexical rules, rather than adding a regular-expression dependency.
            let ok = match p {
                "^\\S+$" => crate::pmi::DatumLabel::new(t).is_ok(),
                "^[+-]?[0-9]+(?:\\.[0-9]*)?(?:[Ee][+-]?[0-9]+)?$" => {
                    crate::pmi::Decimal::parse(t).is_ok()
                }
                _ => return fail("unsupported schema pattern"),
            };
            if !ok {
                return fail("string does not match pattern");
            }
        }
    }
    if let Some(a) = v.as_array() {
        if let Some(n) = s["minItems"].as_u64()
            && a.len() < n as usize
        {
            return fail("array too short");
        }
        if let Some(n) = s["maxItems"].as_u64()
            && a.len() > n as usize
        {
            return fail("array too long");
        }
        let prefix = s["prefixItems"].as_array();
        for (i, x) in a.iter().enumerate() {
            if let Some(child) = prefix.and_then(|p| p.get(i)).or_else(|| s.get("items")) {
                check(x, child, root, &format!("{path}/{i}"))?;
            }
        }
    }
    if let Some(o) = v.as_object() {
        if let Some(req) = s["required"].as_array() {
            for k in req.iter().filter_map(Value::as_str) {
                if !o.contains_key(k) {
                    return fail(&format!("missing property {k}"));
                }
            }
        }
        for (k, x) in o {
            if let Some(child) = s["properties"].get(k) {
                check(x, child, root, &format!("{path}/{k}"))?;
            } else if let Some(add) = s.get("additionalProperties") {
                if add == &Value::Bool(false) {
                    return fail(&format!("unknown property {k}"));
                }
                if add.is_object() {
                    check(x, add, root, &format!("{path}/{k}"))?;
                }
            }
        }
    }
    Ok(())
}

/// Walk typed leaves (units and face anchors) through the same shipped schema.
pub(super) fn walk(
    v: &mut Value,
    s: &Value,
    root: &Value,
    f: &mut impl FnMut(&str, &mut Value) -> Result<()>,
) -> Result<()> {
    walk_path(v, s, root, "", &mut |kind, v, _| f(kind, v))
}
pub(super) fn walk_path(
    v: &mut Value,
    s: &Value,
    root: &Value,
    path: &str,
    f: &mut impl FnMut(&str, &mut Value, &str) -> Result<()>,
) -> Result<()> {
    if let Some(r) = s["$ref"].as_str() {
        let name = r.rsplit('/').next().unwrap_or("");
        if super::reference_collection(name).is_some() {
            return f(name, v, path);
        }
        if [
            "LengthUnit",
            "AngleUnit",
            "DensityUnit",
            "OtherUnit",
            "Decimal",
            "FaceIndex",
            "EdgeIndex",
        ]
        .contains(&name)
        {
            integer_value(v)?;
            return f(name, v, path);
        }
        return walk_path(
            v,
            root.pointer(r.trim_start_matches('#'))
                .ok_or_else(|| Error(format!("invalid schema reference {r}")))?,
            root,
            path,
            f,
        );
    }
    if s["type"] == "integer" {
        integer_value(v)?;
    }
    for key in ["oneOf", "anyOf"] {
        if let Some(branches) = s[key].as_array() {
            let branch = branches.iter().find(|b| {
                if v.is_null() {
                    b["type"] == "null"
                } else if let Some(c) = b.get("const") {
                    v == c
                } else if let Some(p) = b["properties"].as_object() {
                    v.as_object()
                        .is_some_and(|o| p.keys().all(|k| o.contains_key(k)))
                } else {
                    b["type"] != "null"
                }
            });
            if let Some(b) = branch {
                return walk_path(v, b, root, path, f);
            }
        }
    }
    if let Some(o) = v.as_object_mut() {
        for (k, x) in o {
            if let Some(child) = s["properties"].get(k) {
                walk_path(
                    x,
                    child,
                    root,
                    &format!("{path}/{}", k.replace('~', "~0").replace('/', "~1")),
                    f,
                )?;
            }
        }
    }
    if let Some(a) = v.as_array_mut() {
        for (i, x) in a.iter_mut().enumerate() {
            if let Some(child) = s["prefixItems"]
                .as_array()
                .and_then(|p| p.get(i))
                .or_else(|| s.get("items"))
            {
                walk_path(x, child, root, &format!("{path}/{i}"), f)?;
            }
        }
    }
    Ok(())
}

fn equal(a: &Value, b: &Value) -> bool {
    a == b || (a.is_number() && b.is_number() && a.as_f64() == b.as_f64())
}

pub(super) fn unsigned(v: &Value) -> Option<u64> {
    v.as_u64().or_else(|| {
        v.as_f64()
            .filter(|n| n.fract() == 0. && *n >= 0. && *n < (u64::MAX as f64))
            .map(|n| n as u64)
    })
}

fn integer_value(v: &mut Value) -> Result<()> {
    if !v.is_f64() {
        return Ok(());
    }
    if let Some(n) = unsigned(v) {
        *v = Value::from(n);
        return Ok(());
    }
    if let Some(n) = v
        .as_f64()
        .filter(|n| n.fract() == 0. && *n < 0. && *n >= (i64::MIN as f64))
    {
        *v = Value::from(n as i64);
        return Ok(());
    }
    Err(Error(
        "integer outside the model's representable range".into(),
    ))
}
