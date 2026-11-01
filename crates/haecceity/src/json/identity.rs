//! Stable content identity and support expansion; no randomized hashes or model-order keys.
use super::*;

/// Canonical UTF-8 JSON: sorted object keys, ordered arrays, no whitespace.
/// Explicit sorting keeps ids portable even when a consumer enables preserve_order.
pub(super) fn canonical(value: &Value) -> Result<String> {
    fn append(value: &Value, out: &mut String) -> Result<()> {
        match value {
            Value::Object(object) => {
                let mut keys: Vec<_> = object.keys().collect();
                keys.sort();
                out.push('{');
                for (i, key) in keys.into_iter().enumerate() {
                    if i != 0 {
                        out.push(',');
                    }
                    out.push_str(&serde_json::to_string(key)?);
                    out.push(':');
                    append(&object[key], out)?;
                }
                out.push('}');
            }
            Value::Array(array) => {
                out.push('[');
                for (i, value) in array.iter().enumerate() {
                    if i != 0 {
                        out.push(',');
                    }
                    append(value, out)?;
                }
                out.push(']');
            }
            _ => out.push_str(&serde_json::to_string(value)?),
        }
        Ok(())
    }
    let mut out = String::new();
    append(value, &mut out)?;
    Ok(out)
}

pub(super) fn digest(text: &str) -> String {
    let mut h: u128 = 0x6c62272e07bb014262b821756295c58d;
    for byte in text.bytes() {
        h ^= u128::from(byte);
        h = h.wrapping_mul(0x0000000001000000000000000000013b);
    }
    let mut out = String::with_capacity(32);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in h.to_be_bytes() {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 15) as usize] as char);
    }
    out
}

pub(super) fn content(
    raw: &Value,
    collection: &str,
    index: usize,
    anchors: &Anchors,
    stack: &mut BTreeSet<(String, usize)>,
) -> Result<Value> {
    let key = (collection.to_string(), index);
    if !stack.insert(key.clone()) {
        return Err(Error(format!("cyclic {collection} reference {index}")));
    }
    let mut value = raw[collection]
        .get(index)
        .cloned()
        .ok_or_else(|| Error(format!("unknown {collection} index {index}")))?;
    if collection == "notes" {
        value["callouts"] = json!([]);
    }
    walk::item(&mut value, collection, &mut |kind, value| {
        if let Some(c) = reference_collection(kind) {
            *value = content(
                raw,
                c,
                value
                    .as_u64()
                    .ok_or_else(|| Error(format!("invalid {kind}")))? as usize,
                anchors,
                stack,
            )?;
        } else if kind == "FaceIndex"
            && let Some(key) = value
                .as_u64()
                .and_then(|i| anchors.fingerprints.get(i as usize))
                .and_then(Option::as_ref)
        {
            *value = json!({"fingerprint":key});
        }
        Ok(())
    })?;
    stack.remove(&key);
    Ok(value)
}

pub(super) fn identify(
    pmi: PartPmi,
    identifiers: Option<&PartIdentifiers>,
    anchors: &Anchors,
) -> Result<Semantic> {
    let raw = serde_json::to_value(&pmi)?;
    let mut used = BTreeSet::new();
    let mut ids = BTreeMap::new();
    let mut accepted = BTreeMap::new();
    // Stable priority is collection order, then native model order. Reserve all unique
    // requirement ids before supports and generated identities, as before. A repeated or
    // empty file identifier remains an observation, never an ambiguous wire reference.
    for &c in COLLECTIONS.iter().filter(|&&c| c != "features") {
        let mut list = vec![None; raw[c].as_array().map_or(0, Vec::len)];
        for (i, slot) in list.iter_mut().enumerate() {
            if let Some(Some(id)) = file_ids(identifiers, c).get(i)
                && !id.is_empty()
                && used.insert(id.clone())
            {
                *slot = Some(id.clone());
            }
        }
        accepted.insert(c, list);
    }
    let mut supports = BTreeSet::new();
    let mut feature_ids = vec![None; pmi.features.len()];
    for (i, slot) in feature_ids.iter_mut().enumerate() {
        if let Some(Some(id)) = file_ids(identifiers, "features").get(i)
            && !id.is_empty()
            && supports.insert(id.clone())
        {
            let id = if used.contains(id) {
                format!("features:{id}")
            } else {
                id.clone()
            };
            if used.insert(id.clone()) {
                *slot = Some(id);
            }
        }
    }
    accepted.insert("features", feature_ids);
    let mut contents = BTreeMap::<String, String>::new();
    for &c in COLLECTIONS {
        let mut out = vec![];
        for i in 0..raw[c].as_array().map_or(0, Vec::len) {
            if let Some(id) = &accepted[c][i] {
                out.push(id.clone());
                continue;
            }
            let text = canonical(&content(&raw, c, i, anchors, &mut BTreeSet::new())?)?;
            let base = format!("{c}:{}", digest(&text));
            if contents
                .get(&base)
                .is_some_and(|previous| previous != &text)
            {
                return Err(Error(format!("content id collision {base:?}")));
            }
            contents.insert(base.clone(), text);
            let mut id = base.clone();
            let mut n = 1;
            while !used.insert(id.clone()) {
                n += 1;
                id = format!("{base}~{n}");
            }
            out.push(id);
        }
        ids.insert(c.to_string(), out);
    }
    let source_identifiers = identifiers
        .and_then(|i| i.json_identifiers.as_ref())
        .map(|s| serde_json::from_str(s))
        .transpose()?
        .unwrap_or_else(|| {
            COLLECTIONS
                .iter()
                .filter_map(|&c| {
                    let list: Vec<_> = (0..raw[c].as_array().map_or(0, Vec::len))
                        .map(|i| file_ids(identifiers, c).get(i).cloned().flatten())
                        .collect();
                    list.iter()
                        .any(Option::is_some)
                        .then(|| (c.to_string(), list))
                })
                .collect()
        });
    #[cfg(feature = "json-import")]
    let unit_table = identifiers
        .and_then(|i| i.json_units.as_ref())
        .map(|s| serde_json::from_str(s))
        .transpose()?;
    #[cfg(not(feature = "json-import"))]
    let unit_table = None;
    #[cfg(feature = "json-import")]
    let anchor_bindings = identifiers
        .and_then(|i| i.json_anchors.as_ref())
        .map(|s| serde_json::from_str(s))
        .transpose()?
        .unwrap_or_default();
    #[cfg(not(feature = "json-import"))]
    let anchor_bindings = vec![];
    Ok(Semantic {
        pmi,
        ids,
        identifiers: source_identifiers,
        unit_table,
        anchor_bindings,
    })
}

#[cfg(feature = "json-import")]
pub(super) fn expand_supports(value: &mut Value, anchors: &Anchors) -> Result<()> {
    match value {
        Value::Array(array) => {
            let mut expanded = vec![];
            for mut item in std::mem::take(array) {
                let (face, wrapped) = if item.get("face").is_some() {
                    (&item["face"], true)
                } else {
                    (&item, false)
                };
                if let Some(support) = face
                    .pointer("/fingerprint/support")
                    .and_then(Value::as_array)
                {
                    for index in support {
                        let mut singleton = face.clone();
                        singleton["fingerprint"]["support"] = json!([index]);
                        let resolved = json!(anchors.resolve(&singleton)?);
                        expanded.push(if wrapped {
                            json!({"face":resolved})
                        } else {
                            resolved
                        });
                    }
                } else {
                    expand_supports(&mut item, anchors)?;
                    expanded.push(item);
                }
            }
            *array = expanded;
        }
        Value::Object(object) => {
            for item in object.values_mut() {
                expand_supports(item, anchors)?;
            }
        }
        _ => {}
    }
    Ok(())
}
