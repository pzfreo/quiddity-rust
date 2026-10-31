//! Original wire anchor forms, verified against their resolved native support.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnchorBinding {
    path: String,
    wire: Value,
    resolved: Value,
}

fn differences(wire: &Value, resolved: &Value, path: &str, out: &mut Vec<AnchorBinding>) {
    if wire == resolved
        || (wire.is_number() && resolved.is_number() && wire.as_f64() == resolved.as_f64())
    {
        return;
    }
    match (wire, resolved) {
        (Value::Object(a), Value::Object(b)) if a.keys().eq(b.keys()) => {
            for (k, v) in a {
                differences(
                    v,
                    &b[k],
                    &format!("{path}/{}", k.replace('~', "~0").replace('/', "~1")),
                    out,
                );
            }
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                differences(a, b, &format!("{path}/{i}"), out);
            }
        }
        _ => out.push(AnchorBinding {
            path: path.into(),
            wire: wire.clone(),
            resolved: resolved.clone(),
        }),
    }
}

pub(super) fn capture(wire: &Value, anchors: &Anchors) -> Result<Vec<AnchorBinding>> {
    let mut resolved = wire.clone();
    identity::expand_supports(&mut resolved, anchors)?;
    schema::walk(
        &mut resolved,
        &pmi_schema()["$defs"]["PartPmi"],
        pmi_schema(),
        &mut |kind, v| {
            if kind == "FaceIndex" {
                *v = json!(anchors.resolve(v)?);
            }
            Ok(())
        },
    )?;
    let mut out = vec![];
    differences(wire, &resolved, "", &mut out);
    // Only the typed topology leaves/arrays may carry a wire-form binding.
    apply(&mut resolved, &out)?;
    Ok(out)
}

pub(super) fn apply(raw: &mut Value, bindings: &[AnchorBinding]) -> Result<()> {
    if bindings.is_empty() {
        return Ok(());
    }
    let mut faces = BTreeSet::new();
    schema::walk_path(
        raw,
        &pmi_schema()["$defs"]["PartPmi"],
        pmi_schema(),
        "",
        &mut |kind, _, path| {
            if kind == "FaceIndex" {
                faces.insert(path.to_string());
            }
            Ok(())
        },
    )?;
    let mut seen = BTreeSet::new();
    for b in bindings {
        if !seen.insert(&b.path) {
            return Err(Error("duplicate wire anchor binding".into()));
        }
        let current = raw
            .pointer(&b.path)
            .ok_or_else(|| Error(format!("stale wire anchor path {}", b.path)))?;
        if current != &b.resolved {
            return Err(Error(format!(
                "wire anchor {} no longer selects the same native faces",
                b.path
            )));
        }
        let leaf = faces.contains(&b.path);
        let direct_face = |path: &str, wrapped: bool| {
            let Some(rest) = path.strip_prefix(&format!("{}/", b.path)) else {
                return false;
            };
            let mut parts = rest.split('/');
            parts.next().is_some_and(|n| n.parse::<usize>().is_ok())
                && if wrapped {
                    parts.next() == Some("face") && parts.next().is_none()
                } else {
                    parts.next().is_none()
                }
        };
        let anchor_array = faces.iter().any(|p| direct_face(p, true));
        let array = anchor_array || faces.iter().any(|p| direct_face(p, false));
        if !leaf && !array {
            return Err(Error(format!("wire binding {} is not topology", b.path)));
        }
        if leaf {
            schema::check(
                &b.wire,
                &pmi_schema()["$defs"]["FaceIndex"],
                pmi_schema(),
                &b.path,
            )?;
            if b.wire.is_number() {
                return Err(Error(
                    "a wire anchor binding cannot replace a native face index".into(),
                ));
            }
        } else {
            // A support expansion may only change face addresses. Edges and
            // supplemental geometry references remain independently authoritative.
            let old = b
                .wire
                .as_array()
                .ok_or_else(|| Error("wire support is not an array".into()))?;
            let new = b
                .resolved
                .as_array()
                .ok_or_else(|| Error("resolved support is not an array".into()))?;
            if anchor_array {
                let nonfaces = |a: &[Value]| {
                    a.iter()
                        .filter(|a| a.get("face").is_none())
                        .cloned()
                        .collect::<Vec<_>>()
                };
                if nonfaces(old) != nonfaces(new) {
                    return Err(Error("wire support changes non-face anchors".into()));
                }
            }
        }
        *raw.pointer_mut(&b.path)
            .ok_or_else(|| Error("wire anchor path changed during restoration".into()))? =
            b.wire.clone();
    }
    Ok(())
}
