//! AP242 transport metadata for order and alias bindings. Values remain in native PMI.
//! Every binding is checked against independently decoded native semantic content.
use super::*;
use crate::pmi::{PartId, PmiRead};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    id: String,
    key: String,
    references: BTreeMap<String, Value>,
    anchors: Option<Value>,
    callouts: Option<Value>,
}
type Layout = BTreeMap<String, Vec<Entry>>;

fn key(raw: &Value, collection: &str, index: usize) -> Result<String> {
    let mut value = identity::content(
        raw,
        collection,
        index,
        &Anchors::default(),
        &mut BTreeSet::new(),
    )?;
    fn sets(v: &mut Value) {
        if let Value::Object(o) = v {
            for (k, x) in o.iter_mut() {
                sets(x);
                if ["items", "members", "from"].contains(&k.as_str())
                    && let Some(a) = x.as_array_mut()
                {
                    a.sort_by_key(Value::to_string);
                    a.dedup();
                }
            }
        } else if let Value::Array(a) = v {
            for x in a {
                sets(x);
            }
        }
    }
    // Ordering of structural supports is represented by bindings; all semantic values
    // (including ordered UDA cells and coordinate tuples) remain ordered.
    if collection == "features" {
        sets(&mut value);
    }
    Ok(identity::digest(&serde_json::to_string(&value)?))
}
fn collect(value: &Value, path: &str, out: &mut BTreeMap<String, Value>) {
    if let Some(reference) = value.get("$reference") {
        out.insert(path.into(), reference.clone());
        return;
    }
    match value {
        Value::Object(o) => {
            for (k, v) in o {
                collect(
                    v,
                    &format!("{path}/{}", k.replace('~', "~0").replace('/', "~1")),
                    out,
                );
            }
        }
        Value::Array(a) => {
            for (i, v) in a.iter().enumerate() {
                collect(v, &format!("{path}/{i}"), out);
            }
        }
        _ => {}
    }
}
pub(crate) fn layout(pmi: &PartPmi, ids: &BTreeMap<String, Vec<String>>) -> Result<String> {
    let raw = serde_json::to_value(pmi)?;
    let mut linked = raw.clone();
    schema::walk(
        &mut linked,
        &model_schema(),
        pmi_schema(),
        &mut |kind, value| {
            if let Some(c) = reference_collection(kind) {
                let i = value
                    .as_u64()
                    .ok_or_else(|| Error(format!("invalid {kind}")))?
                    as usize;
                *value = json!({"$reference":{"collection":c,"id":ids[c][i]}});
            }
            Ok(())
        },
    )?;
    let mut layout = Layout::new();
    for &c in COLLECTIONS {
        let mut entries = vec![];
        for (i, id) in ids[c].iter().enumerate() {
            let mut references = BTreeMap::new();
            collect(&linked[c][i], "", &mut references);
            entries.push(Entry {
                id: id.clone(),
                key: key(&raw, c, i)?,
                references,
                callouts: if c == "notes" {
                    Some(linked[c][i]["callouts"].clone())
                } else {
                    None
                },
                anchors: if c == "features" {
                    linked[c][i].get("items").cloned()
                } else {
                    None
                },
            });
        }
        layout.insert(c.into(), entries);
    }
    Ok(serde_json::to_string(&layout)?)
}
fn ids_mut<'a>(ids: &'a mut PartIdentifiers, c: &str) -> &'a mut Vec<Option<String>> {
    match c {
        "features" => &mut ids.features,
        "geometry" => &mut ids.geometry,
        "derived_geometry" => &mut ids.derived_geometry,
        "datum_targets" => &mut ids.datum_targets,
        "datums" => &mut ids.datums,
        "dimensions" => &mut ids.dimensions,
        "tolerances" => &mut ids.tolerances,
        "tolerance_relations" => &mut ids.tolerance_relations,
        "general" => &mut ids.general,
        "threads" => &mut ids.threads,
        "knurls" => &mut ids.knurls,
        "notes" => &mut ids.notes,
        "surface_textures" => &mut ids.surface_textures,
        _ => &mut ids.attributes,
    }
}
fn provenance_mut<'a>(p: &'a mut pmi::PartProvenance, c: &str) -> Option<&'a mut Vec<Vec<u64>>> {
    Some(match c {
        "features" => &mut p.features,
        "geometry" => &mut p.geometry,
        "datum_targets" => &mut p.datum_targets,
        "datums" => &mut p.datums,
        "dimensions" => &mut p.dimensions,
        "tolerances" => &mut p.tolerances,
        "tolerance_relations" => &mut p.tolerance_relations,
        "general" => &mut p.general,
        "threads" => &mut p.threads,
        "knurls" => &mut p.knurls,
        "notes" => &mut p.notes,
        "surface_textures" => &mut p.surface_textures,
        "attributes" => &mut p.attributes,
        _ => return None,
    })
}
fn binding(value: &Value, layout: &Layout) -> Result<Value> {
    let c = value["collection"]
        .as_str()
        .ok_or_else(|| Error("ordering binding missing collection".into()))?;
    let id = value["id"]
        .as_str()
        .ok_or_else(|| Error("ordering binding missing id".into()))?;
    let i = layout
        .get(c)
        .and_then(|entries| entries.iter().position(|e| e.id == id))
        .ok_or_else(|| Error(format!("unknown ordering {c} id {id:?}")))?;
    Ok(json!(i))
}
fn unmark(value: &mut Value, layout: &Layout) -> Result<()> {
    if let Some(reference) = value.get("$reference") {
        *value = binding(reference, layout)?;
        return Ok(());
    }
    match value {
        Value::Object(o) => {
            for v in o.values_mut() {
                unmark(v, layout)?;
            }
        }
        Value::Array(a) => {
            for v in a {
                unmark(v, layout)?;
            }
        }
        _ => {}
    }
    Ok(())
}
pub(crate) fn restore(doc: &Document, read: &mut PmiRead) -> Result<()> {
    for part in 0..read.parts.len() {
        let items: Vec<_> = read.provenance.parts[part]
            .transport
            .iter()
            .filter_map(|id| match doc.get(*id) {
                Some(crate::p21::RawEntity::Simple {
                    name, attributes, ..
                }) if name.eq_ignore_ascii_case("descriptive_representation_item")
                    && attributes.first() == Some(&Attribute::String("ordering".into())) =>
                {
                    match attributes.get(1) {
                        Some(Attribute::String(s)) => Some(s),
                        _ => None,
                    }
                }
                _ => None,
            })
            .collect();
        if items.is_empty() {
            continue;
        }
        if items.len() != 1 {
            return Err(Error("multiple PMI ordering records on a part".into()));
        }
        let layout: Layout = serde_json::from_value(parse(items[0])?)?;
        if layout.len() != COLLECTIONS.len() || COLLECTIONS.iter().any(|c| !layout.contains_key(*c))
        {
            return Err(Error("incomplete PMI ordering record".into()));
        }
        let mut seen = BTreeSet::new();
        let raw = serde_json::to_value(&read.parts[part])?;
        let mut restored = raw.clone();
        let mut maps = BTreeMap::<String, Vec<usize>>::new();
        for &c in COLLECTIONS {
            let old = raw[c]
                .as_array()
                .ok_or_else(|| Error(format!("missing {c}")))?;
            let mut values = vec![];
            let mut order = vec![];
            for e in &layout[c] {
                if e.id.is_empty() || !seen.insert(&e.id) {
                    return Err(Error(format!("duplicate ordering id {:?}", e.id)));
                }
                let index = if ["features", "geometry", "derived_geometry"].contains(&c) {
                    (0..old.len()).find(|&i| key(&raw, c, i).is_ok_and(|k| k == e.key))
                } else {
                    file_ids(Some(&read.identifiers.parts[part]), c)
                        .iter()
                        .position(|id| id.as_deref() == Some(&e.id))
                };
                let i = index.ok_or_else(|| {
                    Error(format!("ordering id {:?} not found in native {c}", e.id))
                })?;
                let mut value = old[i].clone();
                if let Some(anchors) = &e.anchors {
                    value["items"] = anchors.clone();
                    unmark(&mut value["items"], &layout)?;
                }
                if let Some(callouts) = &e.callouts {
                    value["callouts"] = callouts.clone();
                    unmark(&mut value["callouts"], &layout)?;
                }
                for (path, reference) in &e.references {
                    let target = value.pointer_mut(path).ok_or_else(|| {
                        Error(format!("ordering reference path {c}{path} is absent"))
                    })?;
                    *target = binding(reference, &layout)?;
                }
                values.push(value);
                order.push(i);
            }
            restored[c] = Value::Array(values);
            maps.insert(c.into(), order);
        }
        // Reference reindexing/aliases are checked against native, resolved semantic content.
        for &c in COLLECTIONS {
            for (i, e) in layout[c].iter().enumerate() {
                if key(&restored, c, i)? != e.key {
                    return Err(Error(format!(
                        "ordering id {:?} native content changed in {c}",
                        e.id
                    )));
                }
            }
        }
        let pmi: PartPmi = serde_json::from_value(restored)?;
        pmi.validate()
            .map_err(|e| Error(format!("invalid ordered PMI: {e:?}")))?;
        let old_carriers = read.json_carriers.clone();
        for &c in COLLECTIONS {
            let order = &maps[c];
            if let Some(prov) = provenance_mut(&mut read.provenance.parts[part], c) {
                let old = prov.clone();
                *prov = order
                    .iter()
                    .map(|&i| old.get(i).cloned().unwrap_or_default())
                    .collect();
            }
            if c == "notes" {
                let old = read.provenance.parts[part].note_callouts.clone();
                read.provenance.parts[part].note_callouts = order
                    .iter()
                    .map(|&i| old.get(i).cloned().unwrap_or_default())
                    .collect();
            }
            *ids_mut(&mut read.identifiers.parts[part], c) =
                layout[c].iter().map(|e| Some(e.id.clone())).collect();
            read.json_carriers
                .retain(|(p, collection, _), _| *p != PartId(part) || collection != c);
            for (i, &old) in order.iter().enumerate() {
                if let Some(carrier) = old_carriers.get(&(PartId(part), c.into(), old)) {
                    read.json_carriers
                        .insert((PartId(part), c.into(), i), *carrier);
                }
            }
        }
        read.parts[part] = pmi;
    }
    Ok(())
}
