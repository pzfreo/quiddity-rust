//! Stable content identity and support expansion; no randomized hashes or model-order keys.
use super::*;

/// Canonical UTF-8 JSON: sorted object keys, ordered arrays, no whitespace.
/// Explicit sorting keeps ids portable even when a consumer enables preserve_order.
pub(super) fn canonical(value: &Value) -> Result<String> {
    match value {
        Value::Object(object) => {
            let mut keys: Vec<_> = object.keys().collect();
            keys.sort();
            let fields = keys
                .into_iter()
                .map(|key| {
                    Ok(format!(
                        "{}:{}",
                        serde_json::to_string(key)?,
                        canonical(&object[key])?
                    ))
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(format!("{{{}}}", fields.join(",")))
        }
        Value::Array(array) => Ok(format!(
            "[{}]",
            array
                .iter()
                .map(canonical)
                .collect::<Result<Vec<_>>>()?
                .join(",")
        )),
        _ => Ok(serde_json::to_string(value)?),
    }
}

pub(super) fn digest(text: &str) -> String {
    let mut h: u128 = 0x6c62272e07bb014262b821756295c58d;
    for byte in text.bytes() {
        h ^= u128::from(byte);
        h = h.wrapping_mul(0x0000000001000000000000000000013b);
    }
    format!("{h:032x}")
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
    let schema = model_schema();
    schema::walk(
        &mut value,
        &schema["properties"][collection]["items"],
        pmi_schema(),
        &mut |kind, value| {
            if let Some(c) = reference_collection(kind) {
                *value = content(
                    raw,
                    c,
                    value
                        .as_u64()
                        .ok_or_else(|| Error(format!("invalid {kind}")))?
                        as usize,
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
        },
    )?;
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
    // Requirement source ids reserve the global namespace before fallback/support ids.
    for &c in COLLECTIONS.iter().filter(|&&c| c != "features") {
        for id in file_ids(identifiers, c).iter().flatten() {
            if id.is_empty() || !used.insert(id.clone()) {
                return Err(Error(format!("duplicate or empty item id {id:?}")));
            }
        }
    }
    let mut contents = BTreeMap::<String, String>::new();
    for &c in COLLECTIONS.iter().filter(|&&c| c != "features") {
        for (i, id) in file_ids(identifiers, c)
            .iter()
            .enumerate()
            .take(raw[c].as_array().map_or(0, Vec::len))
        {
            if let Some(id) = id {
                contents.insert(
                    id.clone(),
                    canonical(&content(&raw, c, i, anchors, &mut BTreeSet::new())?)?,
                );
            }
        }
    }
    for &c in COLLECTIONS {
        let mut out = vec![];
        for i in 0..raw[c].as_array().map_or(0, Vec::len) {
            if c != "features"
                && let Some(Some(id)) = file_ids(identifiers, c).get(i)
            {
                out.push(id.clone());
                continue;
            }
            let text = canonical(&content(&raw, c, i, anchors, &mut BTreeSet::new())?)?;
            let base = if c == "features"
                && let Some(Some(id)) = file_ids(identifiers, c).get(i)
            {
                if used.contains(id) {
                    format!("features:{id}")
                } else {
                    id.clone()
                }
            } else {
                format!("{c}:{}", digest(&text))
            };
            if contents
                .get(&base)
                .is_some_and(|previous| previous != &text)
            {
                return Err(Error(format!("content id collision {base:?}")));
            }
            if used.contains(&base) && !contents.contains_key(&base) {
                return Err(Error(format!(
                    "content id conflicts with source id {base:?}"
                )));
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
    let unit_table = identifiers
        .and_then(|i| i.json_units.as_ref())
        .map(|s| serde_json::from_str(s))
        .transpose()?;
    Ok(Semantic {
        pmi,
        ids,
        unit_table,
    })
}

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
