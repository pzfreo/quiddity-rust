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

pub(crate) mod invariants;
/// The file's As Drawn graphics and generated notation, keyed by semantic id.
pub mod presentation;
mod schema;

pub const PMI_SCHEMA: &str = include_str!("../../schema/haecceity.pmi-1.json");
pub const PRESENTATION_SCHEMA: &str = include_str!("../../schema/haecceity.presentation-1.json");
const COLLECTIONS: &[&str] = &[
    "features",
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

/// The decoded model and its external ids, kept apart like `PmiRead.identifiers`.
#[derive(Clone, Debug)]
pub struct Semantic {
    pub pmi: PartPmi,
    pub ids: BTreeMap<String, Vec<String>>,
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
            let schema = f["schema"]
                .as_u64()
                .ok_or_else(|| Error("invalid fingerprint schema".into()))?;
            let faces = f["faces"]
                .as_str()
                .ok_or_else(|| Error("missing face fingerprint".into()))?;
            let support = f["support"][0]
                .as_u64()
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

/// Assign file ids first, then deterministic collision-free model-order ids.
pub fn identify(pmi: PartPmi, identifiers: Option<&PartIdentifiers>) -> Result<Semantic> {
    let raw = serde_json::to_value(&pmi)?;
    let mut used = BTreeSet::new();
    for &c in COLLECTIONS {
        for id in file_ids(identifiers, c).iter().flatten() {
            if id.is_empty() || !used.insert(id.clone()) {
                return Err(Error(format!("duplicate or empty item id {id:?}")));
            }
        }
    }
    let mut ids = BTreeMap::new();
    for &c in COLLECTIONS {
        let mut out = Vec::new();
        for i in 0..raw[c].as_array().map_or(0, Vec::len) {
            let id = if let Some(Some(id)) = file_ids(identifiers, c).get(i) {
                id.clone()
            } else {
                let base = format!("{c}/{i}");
                let mut id = base.clone();
                let mut n = 0;
                while !used.insert(id.clone()) {
                    n += 1;
                    id = format!("{base}~{n}");
                }
                id
            };
            out.push(id);
        }
        ids.insert(c.to_string(), out);
    }
    Ok(Semantic { pmi, ids })
}

/// Encode every `PartPmi` field without rounding or changing model order.
pub fn to_value(p: &Semantic) -> Result<Value> {
    let mut raw = serde_json::to_value(&p.pmi)?;
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
            if id.is_empty() || !used.insert(id) {
                return Err(Error(format!("duplicate or empty item id {id:?}")));
            }
            *v = json!({"id":id,"value":v.take()});
        }
    }
    let mut lengths = Vec::new();
    let mut angles = Vec::new();
    schema::walk(
        &mut raw,
        &pmi_schema()["$defs"]["PartPmi"],
        pmi_schema(),
        &mut |kind, v| {
            let units = match kind {
                "LengthUnit" => &mut lengths,
                "AngleUnit" => &mut angles,
                _ => return Ok(()),
            };
            let index = units.iter().position(|u| u == v).unwrap_or_else(|| {
                units.push(v.clone());
                units.len() - 1
            });
            *v = json!(index);
            Ok(())
        },
    )?;
    let out =
        json!({"schema":"haecceity.pmi/1","units":{"length":lengths,"angle":angles},"pmi":raw});
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
    let mut raw = v["pmi"].clone();
    schema::walk(
        &mut raw,
        &pmi_schema()["$defs"]["PartPmi"],
        pmi_schema(),
        &mut |kind, x| {
            match kind {
                "FaceIndex" => *x = json!(anchors.resolve(x)?),
                "EdgeIndex" => {
                    if x.as_u64().is_none_or(|i| i >= anchors.edge_count as u64) {
                        return Err(Error(format!("unknown edge index {x}")));
                    }
                }
                "LengthUnit" | "AngleUnit" => {
                    let table = if kind == "LengthUnit" {
                        "length"
                    } else {
                        "angle"
                    };
                    let i = x
                        .as_u64()
                        .ok_or_else(|| Error("invalid unit index".into()))?
                        as usize;
                    *x = v["units"][table]
                        .get(i)
                        .ok_or_else(|| Error(format!("undefined {table} unit {i}")))?
                        .clone();
                }
                _ => {}
            }
            Ok(())
        },
    )?;
    let mut ids = BTreeMap::new();
    let mut used = BTreeSet::new();
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
    Ok(Semantic { pmi, ids })
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
        // Validate ids, even when a caller built Semantic without decoding JSON.
        to_value(input)?;
        let normalized = pmi::write::standard_form(&input.pmi);
        let mut item_ids = input.ids.clone();
        // Legacy notes migrate to AP242 semantic text or surface texture, exactly as
        // in the ordinary writer. Their external ids follow the migrated item.
        for (i, note) in input.pmi.notes.iter().enumerate() {
            let one = PartPmi {
                notes: vec![note.clone()],
                ..PartPmi::default()
            };
            let migrated = pmi::write::standard_form(&one);
            let collection = if migrated.surface_textures.is_empty() {
                "attributes"
            } else {
                "surface_textures"
            };
            let id = input.ids["notes"][i].clone();
            item_ids.entry(collection.into()).or_default().push(id);
        }
        item_ids.insert("notes".into(), Vec::new());
        if normalized.general.len() != input.pmi.general.len() {
            let retained = input
                .pmi
                .general
                .iter()
                .zip(&input.ids["general"])
                .filter(|(g, _)| normalized.general.contains(g))
                .map(|(_, id)| id.clone())
                .collect();
            item_ids.insert("general".into(), retained);
        }
        values.push((*part, normalized));
        ids.insert(*part, item_ids);
    }
    let (edit, report) = pmi::write::write_identified(doc, parts, &values, &ids, policy)
        .map_err(|e| Error(e.to_string()))?;
    let bytes = doc.apply(&edit).map_err(|e| Error(e.to_string()))?.bytes;
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
