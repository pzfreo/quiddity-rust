//! Lossless semantic PMI JSON, beside `pmi` and `presentation`.
//!
//! Use [`to_value`] / [`from_value`] for the versioned wire form. Serde on the
//! underlying model is an implementation detail: it has neither ids nor a unit table.
//! See the packaged `docs/pmi-json.md` and the schema constants.
use crate::p21::{Attribute, Document};
use crate::pmi::{self, PartIdentifiers, PartPmi};
use crate::step::PartDefinition;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

mod identity;
pub(crate) mod invariants;
mod parse;
/// The file's As Drawn graphics and generated notation, keyed by semantic id.
pub mod presentation;
mod preserve;
mod schema;
pub(crate) mod transport;
mod units;
pub use parse::parse;

pub const PMI_SCHEMA: &str = include_str!("../../schema/haecceity.pmi-1.json");
pub const PRESENTATION_SCHEMA: &str = include_str!("../../schema/haecceity.presentation-1.json");
pub(crate) const COLLECTIONS: &[&str] = &[
    "features",
    "geometry",
    "derived_geometry",
    "datum_targets",
    "datums",
    "dimensions",
    "tolerances",
    "tolerance_relations",
    "general",
    "threads",
    "knurls",
    "notes",
    "surface_textures",
    "attributes",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(pub String);
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self(e.to_string())
    }
}

fn pmi_schema() -> &'static Value {
    static S: OnceLock<Value> = OnceLock::new();
    #[allow(
        clippy::expect_used,
        reason = "the schema is compiled into the crate and checked by tests"
    )]
    S.get_or_init(|| serde_json::from_str(PMI_SCHEMA).expect("packaged PMI schema"))
}

/// The decoded model and its external requirement ids, kept apart like `PmiRead.identifiers`.
#[derive(Clone, Debug)]
pub struct Semantic {
    pub pmi: PartPmi,
    pub ids: BTreeMap<String, Vec<String>>,
    /// Optional caller unit table; native quantities remain authoritative.
    pub unit_table: Option<Value>,
}

/// Caller-provided face fingerprint support keys, in the kernel's face order.
/// Keys may be shared; ambiguous input is refused, never resolved by choosing a face.
#[derive(Clone, Debug, Default)]
pub struct Anchors {
    pub names: Vec<Option<String>>,
    pub fingerprints: Vec<Option<String>>,
    pub edge_count: usize,
}
impl Anchors {
    /// Read face names directly from the STEP representation items. Fingerprint keys
    /// belong to the consumer's snapshot layer and can be filled without a dependency
    /// on quiddity recognition or Specify.
    pub fn read(doc: &Document, part: &PartDefinition) -> Self {
        Self {
            names: part
                .faces
                .iter()
                .map(|id| {
                    doc.get(*id)
                        .and_then(|e| match e {
                            crate::p21::RawEntity::Simple { attributes, .. } => attributes.first(),
                            crate::p21::RawEntity::Complex { parts, .. } => parts
                                .iter()
                                .find(|p| p.name.eq_ignore_ascii_case("representation_item"))
                                .and_then(|p| p.attributes.first()),
                        })
                        .and_then(|a| {
                            if let Attribute::String(s) = a {
                                Some(s.clone())
                            } else {
                                None
                            }
                        })
                        .filter(|s| !s.is_empty())
                })
                .collect(),
            fingerprints: vec![None; part.faces.len()],
            edge_count: part.edges.len(),
        }
    }
    /// Bind Specify schema 1/2's verified face fingerprint to each singleton support.
    /// The consumer computes/verifies `faces` using its existing fingerprint algorithm.
    pub fn bind_support(&mut self, schema: u8, faces: &str) -> Result<()> {
        if ![1, 2].contains(&schema) || faces.is_empty() {
            return Err(Error("invalid support fingerprint namespace".into()));
        }
        self.fingerprints = (0..self.names.len())
            .map(|i| Some(format!("{schema}:{faces}/{i}")))
            .collect();
        Ok(())
    }

    fn resolve(&self, v: &Value) -> Result<usize> {
        if let Some(i) = v.as_u64() {
            let i = usize::try_from(i).map_err(|_| Error("face index out of range".into()))?;
            return if i < self.names.len() {
                Ok(i)
            } else {
                Err(Error(format!("unknown face index {i}")))
            };
        }
        let support_key;
        let (kind, key, list) = if v["fingerprint"].is_object() {
            let f = &v["fingerprint"];
            let schema = schema::unsigned(&f["schema"])
                .ok_or_else(|| Error("invalid fingerprint schema".into()))?;
            let faces = f["faces"]
                .as_str()
                .ok_or_else(|| Error("missing face fingerprint".into()))?;
            let support = schema::unsigned(&f["support"][0])
                .ok_or_else(|| Error("missing face support".into()))?;
            support_key = format!("{schema}:{faces}/{support}");
            ("fingerprint", support_key.as_str(), &self.fingerprints)
        } else if let Some(s) = v["name"].as_str() {
            ("name", s, &self.names)
        } else if let Some(s) = v["fingerprint"].as_str() {
            ("fingerprint", s, &self.fingerprints)
        } else {
            return Err(Error("invalid face anchor".into()));
        };
        let matches: Vec<_> = list
            .iter()
            .enumerate()
            .filter(|(_, s)| s.as_deref() == Some(key))
            .map(|(i, _)| i)
            .collect();
        match matches.as_slice() {
            [i] if *i < self.names.len() => Ok(*i),
            [] => Err(Error(format!("unknown face {kind} {key:?}"))),
            _ => Err(Error(format!("ambiguous face {kind} {key:?}"))),
        }
    }
}

fn file_ids<'a>(ids: Option<&'a PartIdentifiers>, name: &str) -> &'a [Option<String>] {
    let Some(i) = ids else { return &[] };
    match name {
        "features" => &i.features,
        "geometry" => &i.geometry,
        "derived_geometry" => &i.derived_geometry,
        "datum_targets" => &i.datum_targets,
        "datums" => &i.datums,
        "dimensions" => &i.dimensions,
        "tolerances" => &i.tolerances,
        "tolerance_relations" => &i.tolerance_relations,
        "general" => &i.general,
        "threads" => &i.threads,
        "knurls" => &i.knurls,
        "notes" => &i.notes,
        "surface_textures" => &i.surface_textures,
        "attributes" => &i.attributes,
        _ => &[],
    }
}

/// Assign source ids, otherwise stable content ids. Supports have an independent namespace.
pub fn identify(pmi: PartPmi, identifiers: Option<&PartIdentifiers>) -> Result<Semantic> {
    identify_with_anchors(pmi, identifiers, &Anchors::default())
}

/// As `identify`, using consumer-verified support fingerprints in content-derived ids.
pub fn identify_with_anchors(
    pmi: PartPmi,
    identifiers: Option<&PartIdentifiers>,
    anchors: &Anchors,
) -> Result<Semantic> {
    identity::identify(pmi, identifiers, anchors)
}

fn model_schema() -> Value {
    let mut schema = pmi_schema()["$defs"]["PartPmi"].clone();
    for &c in COLLECTIONS {
        schema["properties"][c]["items"] =
            schema["properties"][c]["items"]["properties"]["value"].clone();
    }
    schema
}

fn reference_collection(kind: &str) -> Option<&'static str> {
    match kind {
        "FeatureId" => Some("features"),
        "DatumId" => Some("datums"),
        "DatumTargetId" => Some("datum_targets"),
        "DimensionId" => Some("dimensions"),
        "ToleranceId" => Some("tolerances"),
        "GeometryId" => Some("geometry"),
        "DerivedGeometryId" => Some("derived_geometry"),
        _ => None,
    }
}

/// Encode every `PartPmi` field without rounding or changing model order.
pub fn to_value(p: &Semantic) -> Result<Value> {
    for collection in p.ids.keys() {
        if !COLLECTIONS.contains(&collection.as_str()) {
            return Err(Error(format!(
                "unknown requirement id collection {collection:?}"
            )));
        }
    }
    let mut raw = serde_json::to_value(&p.pmi)?;
    schema::walk(
        &mut raw,
        &model_schema(),
        pmi_schema(),
        &mut |kind, value| {
            if let Some(collection) = reference_collection(kind) {
                let index = value
                    .as_u64()
                    .ok_or_else(|| Error(format!("invalid {kind}")))?
                    as usize;
                *value = json!(
                    p.ids
                        .get(collection)
                        .and_then(|ids| ids.get(index))
                        .ok_or_else(|| Error(format!("unknown {collection} index {index}")))?
                );
            }
            Ok(())
        },
    )?;
    let mut used = BTreeSet::new();
    for &c in COLLECTIONS {
        let values = raw[c]
            .as_array_mut()
            .ok_or_else(|| Error(format!("missing collection {c}")))?;
        let ids = p
            .ids
            .get(c)
            .ok_or_else(|| Error(format!("missing ids for {c}")))?;
        if values.len() != ids.len() {
            return Err(Error(format!(
                "{c}: {} items but {} ids",
                values.len(),
                ids.len()
            )));
        }
        for (v, id) in values.iter_mut().zip(ids) {
            if id.is_empty() || !used.insert(id.clone()) {
                return Err(Error(format!("duplicate or empty item id {id:?}")));
            }
            *v = json!({"id":id,"value":v.take()});
        }
    }
    let mut units = p.unit_table.clone().unwrap_or_else(units::empty);
    units::validate(&units, &mut used)?;
    schema::walk(
        &mut raw,
        &pmi_schema()["$defs"]["PartPmi"],
        pmi_schema(),
        &mut |kind, value| {
            let Some(table) = units::table(kind) else {
                return Ok(());
            };
            let entries = units[table]
                .as_array_mut()
                .ok_or_else(|| Error(format!("missing {table} unit table")))?;
            let id = if let Some(entry) = entries.iter().find(|entry| entry["value"] == *value) {
                entry["id"]
                    .as_str()
                    .ok_or_else(|| Error("missing unit id".into()))?
                    .to_string()
            } else {
                let id = format!("unit:{table}:{}", identity::digest(&value.to_string()));
                if !used.insert(id.clone()) {
                    return Err(Error(format!(
                        "unit content id conflicts with item id {id:?}"
                    )));
                }
                entries.push(json!({"id":id,"value":value}));
                id
            };
            *value = json!(id);
            Ok(())
        },
    )?;
    let out = json!({"schema":"haecceity.pmi/1","units":units,"pmi":raw});
    validate_schema(&out)?;
    Ok(out)
}

/// Validate the exact schema shipped with this crate, including rejecting extra fields.
pub fn validate_schema(v: &Value) -> Result<()> {
    schema::check(v, pmi_schema(), pmi_schema(), "")
}

/// Decode, apply constructors/model invariants, and resolve all topology anchors.
pub fn from_value(v: &Value, anchors: &Anchors) -> Result<Semantic> {
    validate_schema(v)?;
    let mut used = BTreeSet::new();
    units::validate(&v["units"], &mut used)?;
    let mut raw = v["pmi"].clone();
    identity::expand_supports(&mut raw, anchors)?;
    schema::walk(
        &mut raw,
        &pmi_schema()["$defs"]["PartPmi"],
        pmi_schema(),
        &mut |kind, x| {
            if let Some(collection) = reference_collection(kind) {
                let id = x
                    .as_str()
                    .ok_or_else(|| Error(format!("invalid {kind} id")))?;
                let index = v["pmi"][collection]
                    .as_array()
                    .and_then(|items| items.iter().position(|item| item["id"] == id))
                    .ok_or_else(|| Error(format!("unknown {collection} id {id:?}")))?;
                *x = json!(index);
                return Ok(());
            }
            match kind {
                "FaceIndex" => *x = json!(anchors.resolve(x)?),
                "EdgeIndex" => {
                    if x.as_u64().is_none_or(|i| i >= anchors.edge_count as u64) {
                        return Err(Error(format!("unknown edge index {x}")));
                    }
                }
                "LengthUnit" | "AngleUnit" | "DensityUnit" | "OtherUnit" => {
                    let table =
                        units::table(kind).ok_or_else(|| Error("unknown unit type".into()))?;
                    let id = x.as_str().ok_or_else(|| Error("invalid unit id".into()))?;
                    *x = v["units"][table]
                        .as_array()
                        .and_then(|entries| entries.iter().find(|entry| entry["id"] == id))
                        .map(|entry| entry["value"].clone())
                        .ok_or_else(|| Error(format!("undefined {table} unit id {id:?}")))?;
                }
                _ => {}
            }
            Ok(())
        },
    )?;
    let mut ids = BTreeMap::new();
    for &c in COLLECTIONS {
        let values = raw[c]
            .as_array_mut()
            .ok_or_else(|| Error(format!("missing collection {c}")))?;
        let mut list = Vec::new();
        for item in values {
            let id = item["id"]
                .as_str()
                .ok_or_else(|| Error("missing item id".into()))?
                .to_string();
            if !used.insert(id.clone()) {
                return Err(Error(format!("duplicate item id {id:?}")));
            }
            list.push(id);
            *item = item["value"].take();
        }
        ids.insert(c.to_string(), list);
    }
    let pmi: PartPmi = serde_json::from_value(raw)?;
    pmi.validate().map_err(|errors| {
        Error(
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; "),
        )
    })?;
    validate_feature_roles(&pmi)?;
    Ok(Semantic {
        pmi,
        ids,
        unit_table: Some(v["units"].clone()),
    })
}

/// Replace selected parts' PMI using JSON ids. The ordinary STEP writer is unchanged.
/// Refusals come from the existing AP242 writer and retain the item and reason.
pub fn write(
    doc: &Document,
    parts: &[PartDefinition],
    requested: &[(pmi::PartId, Semantic)],
    policy: pmi::write::PresentationPolicy,
) -> Result<(Vec<u8>, pmi::write::WriteReport)> {
    let mut values = Vec::new();
    let mut ids = BTreeMap::new();
    for (part, input) in requested {
        let encoded = to_value(input)?;
        values.push((*part, input.pmi.clone()));
        let mut item_ids = input.ids.clone();
        item_ids.insert(
            "__units".into(),
            vec![serde_json::to_string(&encoded["units"])?],
        );
        ids.insert(*part, item_ids);
    }
    let (base, kept, protected) = preserve::detach(doc, parts, requested)?;
    let (edit, mut report) =
        pmi::write::write_identified(&base, parts, &values, &ids, &protected, policy)
            .map_err(|e| Error(e.to_string()))?;
    let bytes = base.apply(&edit).map_err(|e| Error(e.to_string()))?.bytes;
    let definitions =
        crate::step::read_part_definitions(&bytes).map_err(|e| Error(e.to_string()))?;
    let after = Document::parse(bytes.clone()).map_err(|e| Error(e.to_string()))?;
    let after_read = pmi::read(&after, &definitions).map_err(|e| Error(e.to_string()))?;
    let prior_rules = crate::express_rules::check_all(&after);
    let (bytes, added) = preserve::attach(&after, &after_read, kept)?;
    report.added += added;
    let after = Document::parse(bytes.clone()).map_err(|e| Error(e.to_string()))?;
    let added_schema: Vec<_> = crate::express::validate_document(after.graph())
        .into_iter()
        .filter(|v| v.id > doc.max_id())
        .collect();
    if !added_schema.is_empty() {
        return Err(Error(format!(
            "preserved presentation schema violations: {added_schema:?}"
        )));
    }
    let added_rules: Vec<_> = crate::express_rules::check_all(&after)
        .into_iter()
        .filter(|v| !prior_rules.contains(v))
        .collect();
    if !added_rules.is_empty() {
        return Err(Error(format!(
            "preserved presentation rule violations: {added_rules:?}"
        )));
    }
    let after_read = pmi::read(&after, &definitions).map_err(|e| Error(e.to_string()))?;
    for (part, expected) in requested {
        let actual = &after_read.parts[part.0];
        if &expected.pmi != actual {
            return Err(Error(format!(
                "part {}: JSON identity failed: {:?}",
                part.0,
                pmi::write::differences_as_stated(&expected.pmi, actual)
            )));
        }
        let actual = identify(actual.clone(), Some(&after_read.identifiers.parts[part.0]))?;
        if to_value(expected)?["units"] != to_value(&actual)?["units"] {
            return Err(Error(format!(
                "part {}: unit identities or order changed",
                part.0
            )));
        }
        for &collection in COLLECTIONS {
            if expected.ids[collection] != actual.ids[collection] {
                return Err(Error(format!(
                    "part {}: ids changed collection or item in {collection}: expected {:?}, read {:?}",
                    part.0, expected.ids[collection], actual.ids[collection]
                )));
            }
        }
    }
    Ok((bytes, report))
}

fn validate_feature_roles(p: &PartPmi) -> Result<()> {
    use pmi::{Feature, GroupKind};
    for datum in &p.datums {
        if let Some(f) = datum.feature() {
            let allowed = matches!(
                p.features[f.0],
                Feature::Items(_)
                    | Feature::Group {
                        kind: GroupKind::MultipleElements | GroupKind::PatternOfFeatures,
                        ..
                    }
            );
            if !allowed {
                return Err(Error(format!(
                    "datum {}: kind not allowed on feature {}",
                    datum.label(),
                    f.0
                )));
            }
        }
    }
    for (kind, f) in p
        .threads
        .iter()
        .flat_map(|t| {
            t.partial_area
                .into_iter()
                .chain(t.runout)
                .map(|f| ("thread", f))
        })
        .chain(p.knurls.iter().map(|k| ("knurl", k.feature)))
    {
        if !matches!(p.features[f.0], Feature::Items(_)) {
            return Err(Error(format!(
                "{kind}: kind not allowed on feature {}",
                f.0
            )));
        }
    }
    Ok(())
}

/// Parse and decode one versioned semantic document. Duplicate keys are errors.
pub fn from_str(text: &str, anchors: &Anchors) -> Result<Semantic> {
    from_value(&parse(text)?, anchors)
}

/// Pretty JSON, preserving all stated decimal strings.
pub fn to_string(p: &Semantic) -> Result<String> {
    Ok(serde_json::to_string_pretty(&to_value(p)?)?)
}

/// Import policy: retain foreign graphics of unchanged surviving items and report removals.
pub fn write_preserving(
    doc: &Document,
    parts: &[PartDefinition],
    requested: &[(pmi::PartId, Semantic)],
) -> Result<(Vec<u8>, pmi::write::WriteReport)> {
    write(
        doc,
        parts,
        requested,
        pmi::write::PresentationPolicy::RemovePresentation,
    )
}
