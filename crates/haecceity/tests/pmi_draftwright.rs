//! draftwright parity: everything draftwright reads from a STEP file's PMI (its Part 21 readers'
//! facts, and the records of its `pmi='annotate'` extraction) is in the reader's model with the
//! same semantic content. Facts carry their source entity ids, matched through the reader's
//! provenance; records without ids are matched by kind, value and faces. Every difference is
//! pinned in `tests/fixtures/known_pmi_draftwright.json` with a verdict; the test fails on an
//! unexplained, changed or duplicate entry.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use haecceity::p21::Document;
use haecceity::pmi::{
    self, AttributeValue, DimTolerance, DimensionKind, Feature, PartPmi, PmiRead, ToleranceKind,
    ToleranceModifier,
};
use haecceity::step::{PartDefinition, read_part_definitions};
use serde_json::{Value as Json, json};

fn file_bytes(path: &Path) -> Vec<u8> {
    let raw = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    if path.extension().is_some_and(|e| e == "gz") {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(raw.as_slice())
            .read_to_end(&mut out)
            .unwrap();
        out
    } else {
        raw
    }
}

fn id(s: &str) -> Option<u64> {
    s.trim_start_matches('#').parse().ok()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0)
}

/// The faces and edges a feature covers, as the file's `#N`.
fn cover(p: &PartPmi, part: &PartDefinition, geometry: &[u64], f: pmi::FeatureId) -> BTreeSet<u64> {
    let mut out = BTreeSet::new();
    let mut stack = vec![f];
    while let Some(f) = stack.pop() {
        match &p.features[f.0] {
            Feature::Items(a) => {
                for x in a {
                    out.insert(match x {
                        pmi::Anchor::Face(i) => part.faces[i.0],
                        pmi::Anchor::Edge(e) => part.edges[e.0],
                        pmi::Anchor::Geometry(g) => geometry[g.0],
                    });
                }
            }
            Feature::Group { members, .. } => stack.extend(members.iter().copied()),
            Feature::Derived { from, .. } => stack.extend(from.iter().copied()),
        }
    }
    out
}

fn ids_of(j: &Json) -> BTreeSet<u64> {
    let mut out = BTreeSet::new();
    let mut stack = vec![j];
    while let Some(j) = stack.pop() {
        match j {
            Json::Array(a) => stack.extend(a.iter()),
            Json::String(s) => out.extend(id(s)),
            _ => {}
        }
    }
    out
}

fn kind_name(k: ToleranceKind) -> &'static str {
    match k {
        ToleranceKind::Angularity => "angularity",
        ToleranceKind::CircularRunout => "circular_runout",
        ToleranceKind::Coaxiality => "coaxiality",
        ToleranceKind::Concentricity => "concentricity",
        ToleranceKind::Cylindricity => "cylindricity",
        ToleranceKind::Flatness => "flatness",
        ToleranceKind::LineProfile => "profile_line",
        ToleranceKind::Parallelism => "parallelism",
        ToleranceKind::Perpendicularity => "perpendicularity",
        ToleranceKind::Position => "position",
        ToleranceKind::Roundness => "roundness",
        ToleranceKind::Straightness => "straightness",
        ToleranceKind::SurfaceProfile => "profile_surface",
        ToleranceKind::Symmetry => "symmetry",
        ToleranceKind::TotalRunout => "total_runout",
    }
}

/// Where in the read an instance id was consumed: (part, item kind, index).
struct Index(BTreeMap<u64, Vec<(usize, &'static str, usize)>>);

impl Index {
    fn new(r: &PmiRead) -> Index {
        let mut m: BTreeMap<u64, Vec<(usize, &'static str, usize)>> = BTreeMap::new();
        for (pi, p) in r.provenance.parts.iter().enumerate() {
            let lists: [(&'static str, &Vec<Vec<u64>>); 8] = [
                ("feature", &p.features),
                ("datum", &p.datums),
                ("target", &p.datum_targets),
                ("dimension", &p.dimensions),
                ("tolerance", &p.tolerances),
                ("note", &p.notes),
                ("attribute", &p.attributes),
                ("general", &p.general),
            ];
            for (k, l) in lists {
                for (i, ids) in l.iter().enumerate() {
                    for &x in ids {
                        m.entry(x).or_default().push((pi, k, i));
                    }
                }
            }
            for &x in &p.material {
                m.entry(x).or_default().push((pi, "material", 0));
            }
        }
        Index(m)
    }

    fn find(&self, n: u64, kind: &str) -> Option<(usize, usize)> {
        self.0
            .get(&n)?
            .iter()
            .find(|(_, k, _)| *k == kind)
            .map(|&(p, _, i)| (p, i))
    }
}

fn mm(v: &pmi::Value) -> f64 {
    v.si()
}

/// The differences between draftwright's capture of one file and the reader's PMI of it.
fn compare(name: &str, dw: &Json, r: &PmiRead, parts: &[PartDefinition]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let ix = Index::new(r);
    let facts = |k: &str| {
        dw["part21"][k]["facts"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    };
    let mut push = |k: String, d: String| out.push((format!("{name} {k}"), d));

    for f in facts("read_geometric_tolerances") {
        let e = f["entity_id"].as_str().unwrap();
        let key = format!("tolerance {e}");
        let Some((pi, i)) = id(e).and_then(|n| ix.find(n, "tolerance")) else {
            push(key, format!("{} not read ({})", f["kind"], f["reason"]));
            continue;
        };
        let t = &r.parts[pi].tolerances[i];
        if kind_name(t.kind) != f["kind"].as_str().unwrap_or("") {
            push(
                format!("{key} kind"),
                format!("{} vs {:?}", f["kind"], t.kind),
            );
        }
        let v = t.magnitude.as_ref().map(mm);
        match (f["value_mm"].as_f64(), v) {
            (Some(a), Some(b)) if close(a, b) => {}
            (a, b) => push(format!("{key} value"), format!("{a:?} vs {b:?}")),
        }
    }

    for f in facts("read_datum_definitions") {
        let d = f["datum_id"].as_str().unwrap();
        let key = format!("datum {d}");
        let Some((pi, i)) = id(d).and_then(|n| ix.find(n, "datum")) else {
            push(key, format!("{} not read ({})", f["letter"], f["reason"]));
            continue;
        };
        let p = &r.parts[pi];
        let datum = &p.datums[i];
        if datum.label().as_str() != f["letter"].as_str().unwrap_or("") {
            push(
                format!("{key} letter"),
                format!("{} vs {}", f["letter"], datum.label()),
            );
        }
        let theirs = ids_of(&f["reference_item_ids"]);
        let ours = datum
            .feature()
            .map(|x| cover(p, &parts[pi], &r.provenance.parts[pi].geometry_items, x))
            .unwrap_or_default();
        if theirs != ours {
            push(format!("{key} faces"), format!("{theirs:?} vs {ours:?}"));
        }
    }

    for f in facts("read_datum_occurrences") {
        let t = f["tolerance_id"].as_str().unwrap();
        let letter = f["letter"].as_str().unwrap_or("");
        let key = format!("datum occurrence {t} {letter}");
        let Some((pi, i)) = id(t).and_then(|n| ix.find(n, "tolerance")) else {
            push(key, "tolerance not read".into());
            continue;
        };
        let p = &r.parts[pi];
        let cited = p.tolerances[i].datums.as_ref().is_some_and(|s| {
            s.compartments()
                .iter()
                .flat_map(|c| &c.references)
                .any(|x| p.datums[x.datum.0].label().as_str() == letter)
        });
        if !cited {
            push(key, "datum not cited by the tolerance".into());
        }
    }

    let dims = facts("read_dimension_associations");
    let display = facts("read_dimension_display_facts");
    for f in &dims {
        let e = f["entity_id"].as_str().unwrap();
        let key = format!("dimension {e}");
        let Some((pi, i)) = id(e).and_then(|n| ix.find(n, "dimension")) else {
            push(key, format!("{} not read ({})", f["kind"], f["reason"]));
            continue;
        };
        let p = &r.parts[pi];
        let d = &p.dimensions[i];
        let (kind, faces) = match &d.kind {
            DimensionKind::Size { feature, .. } => (
                "size",
                cover(
                    p,
                    &parts[pi],
                    &r.provenance.parts[pi].geometry_items,
                    *feature,
                ),
            ),
            DimensionKind::Location { from, to, .. } => {
                let mut c = cover(p, &parts[pi], &r.provenance.parts[pi].geometry_items, *from);
                c.extend(cover(
                    p,
                    &parts[pi],
                    &r.provenance.parts[pi].geometry_items,
                    *to,
                ));
                ("location", c)
            }
        };
        if f["kind"].as_str() != Some(kind) {
            push(format!("{key} kind"), format!("{} vs {kind}", f["kind"]));
        }
        if f["basic"].as_bool() != Some(d.tolerance == DimTolerance::Basic) {
            push(
                format!("{key} basic"),
                format!("{} vs {:?}", f["basic"], d.tolerance),
            );
        }
        let theirs = ids_of(&f["reference_item_groups"]);
        if !theirs.is_empty() && theirs != faces {
            push(format!("{key} faces"), format!("{theirs:?} vs {faces:?}"));
        }
    }
    for f in &display {
        let e = f["entity_id"].as_str().unwrap();
        let key = format!("display {e}");
        let Some((pi, i)) = id(e).and_then(|n| ix.find(n, "dimension")) else {
            push(key, "dimension not read".into());
            continue;
        };
        let d = &r.parts[pi].dimensions[i];
        let nominal = d.nominal.as_ref();
        if f["authored_value"].as_f64() != nominal.map(|n| n.decimal().to_f64())
            && !matches!((f["authored_value"].as_f64(), nominal), (Some(a), Some(n)) if close(a, n.decimal().to_f64()))
        {
            push(
                format!("{key} value"),
                format!(
                    "{} vs {:?}",
                    f["authored_value"],
                    nominal.map(|n| n.decimal().to_string())
                ),
            );
        }
        let vd = f["value_decimals"].as_u64().map(|x| x as u8);
        if vd != nominal.and_then(|n| n.decimal_places) {
            push(
                format!("{key} value decimals"),
                format!("{vd:?} vs {:?}", nominal.and_then(|n| n.decimal_places)),
            );
        }
        let td = f["tolerance_decimals"].as_u64().map(|x| x as u8);
        let ours = match &d.tolerance {
            DimTolerance::Deviations(b) | DimTolerance::Limits(b) => b.upper().decimal_places,
            DimTolerance::Fit {
                limits: Some(b), ..
            } => b.upper().decimal_places,
            _ => None,
        };
        if td != ours {
            push(
                format!("{key} tolerance decimals"),
                format!("{td:?} vs {ours:?}"),
            );
        }
        let factor = match nominal.map(|n| &n.quantity) {
            Some(pmi::Quantity::Length(l)) => l.unit.millimetres(),
            _ => 1.0,
        };
        if let Some(u) = f["unit_factor_mm"].as_f64()
            && nominal.is_some()
            && !close(u, factor)
        {
            push(format!("{key} unit"), format!("{u} vs {factor}"));
        }
    }

    for f in facts("read_material_properties") {
        let e = f["entity_id"].as_str().unwrap();
        let key = format!("material {e}");
        match id(e).and_then(|n| ix.find(n, "material")) {
            Some((pi, _)) => {
                let m = r.parts[pi].material.as_ref().unwrap();
                if Some(m.id.as_str()) != f["designation"].as_str() {
                    push(key.clone(), format!("{} vs {}", f["designation"], m.id));
                }
                let common = f["common_name"].as_str().unwrap_or("");
                if m.name.as_deref().unwrap_or("") != common {
                    push(format!("{key} name"), format!("{common:?} vs {:?}", m.name));
                }
            }
            None => push(key, "not read".into()),
        }
    }

    for f in facts("read_manufacturing_requirements") {
        let e = f["entity_id"].as_str().unwrap();
        let key = format!("requirement {e}");
        let Some((pi, i)) = id(e).and_then(|n| ix.find(n, "note")) else {
            push(key, "not read".into());
            continue;
        };
        let n = &r.parts[pi].notes[i];
        if Some(n.text.as_str()) != f["text"].as_str()
            || Some(n.kind.as_str()) != f["semantic_name"].as_str()
        {
            push(
                key.clone(),
                format!(
                    "{} {} vs {} {}",
                    f["semantic_name"], f["text"], n.kind, n.text
                ),
            );
        }
        if !f["shape_aspect_ids"].as_array().unwrap().is_empty() {
            push(
                format!("{key} shape aspects"),
                format!(
                    "{} by presentation name; the note is on {:?}",
                    f["shape_aspect_ids"], n.on
                ),
            );
        }
    }

    for f in facts("read_structured_manufacturing_requirements") {
        let e = f["entity_id"].as_str().unwrap();
        let key = format!("structured {e}");
        let kind = f["kind"].as_str().unwrap_or("");
        if kind == "general_tolerances" {
            match id(e).and_then(|n| ix.find(n, "general")) {
                Some((pi, i)) => {
                    let g = &r.parts[pi].general[i];
                    let text = match g {
                        pmi::GeneralTolerance::Class { text, .. } => text.clone(),
                        pmi::GeneralTolerance::Table { name, .. } => name.clone(),
                    };
                    let field = f["fields"][0][1].as_str().unwrap_or("");
                    if field != text {
                        push(key, format!("{field:?} vs {text:?}"));
                    }
                }
                None => push(key, "general tolerance not read".into()),
            }
            continue;
        }
        let Some((pi, i)) = id(e).and_then(|n| ix.find(n, "attribute")) else {
            push(key, "not read".into());
            continue;
        };
        let a = &r.parts[pi].attributes[i];
        for fv in f["fields"].as_array().unwrap() {
            let n = fv[0].as_str().unwrap();
            let ours = a
                .items
                .iter()
                .find(|(k, _)| k.to_lowercase() == n)
                .map(|(_, v)| v);
            let same = match (&fv[1], ours) {
                (Json::String(s), Some(AttributeValue::Text(t))) => s == t,
                (Json::Number(x), Some(AttributeValue::Measure(v))) => {
                    close(x.as_f64().unwrap(), v.si())
                }
                _ => false,
            };
            if !same {
                push(format!("{key} {n}"), format!("{} vs {ours:?}", fv[1]));
            }
        }
        let theirs = ids_of(&f["reference_item_ids"]);
        let ours = match a.on {
            Some(pmi::NoteOwner::Feature(x)) => cover(
                &r.parts[pi],
                &parts[pi],
                &r.provenance.parts[pi].geometry_items,
                x,
            ),
            _ => BTreeSet::new(),
        };
        if theirs != ours {
            push(format!("{key} faces"), format!("{theirs:?} vs {ours:?}"));
        }
    }

    for (reader, what) in [
        ("read_surface_labels", "surface label"),
        ("read_common_labels", "common label"),
    ] {
        for f in facts(reader) {
            let e = f["entity_id"].as_str().unwrap();
            let key = format!("{what} {e}");
            let Some((pi, i)) = id(e).and_then(|n| ix.find(n, "attribute")) else {
                push(key, format!("not read ({})", f["reason"]));
                continue;
            };
            let a = &r.parts[pi].attributes[i];
            // draftwright keeps Part 21 string escapes (\X\HH); compare the decoded text, and
            // without the white space its labels trim.
            let raw = f["text"].as_str().unwrap_or("");
            let decoded = haecceity::p21::decode(raw).unwrap_or_else(|_| raw.to_string());
            let text = decoded.trim();
            let ours: Vec<&str> = a
                .items
                .iter()
                .filter_map(|(_, v)| match v {
                    AttributeValue::Text(t) => Some(t.as_str()),
                    _ => None,
                })
                .collect();
            let ours: Vec<&str> = ours.iter().map(|t| t.trim()).collect();
            if !text.is_empty() && ours.join("\n") != text && !ours.contains(&text) {
                push(key.clone(), format!("{text:?} vs {ours:?}"));
            }
            let theirs = ids_of(&f["reference_item_ids"]);
            let faces = match a.on {
                Some(pmi::NoteOwner::Feature(x)) => cover(
                    &r.parts[pi],
                    &parts[pi],
                    &r.provenance.parts[pi].geometry_items,
                    x,
                ),
                _ => BTreeSet::new(),
            };
            if !theirs.is_empty() && theirs != faces {
                push(format!("{key} faces"), format!("{theirs:?} vs {faces:?}"));
            }
        }
    }

    // The extraction's records (OpenCascade's reading, with Part 21 ids where draftwright joined
    // them).
    let mut used: BTreeSet<(usize, &'static str, usize)> = BTreeSet::new();
    for rec in dw["report"]["records"].as_array().unwrap() {
        let cat = rec["source_category"].as_str().unwrap();
        let kind = rec["kind"].as_str().unwrap();
        let sid = rec["source_id"].as_str().unwrap_or("");
        let pid = rec["part21_id"].as_str().and_then(id);
        let key = format!(
            "record {sid} {cat} {kind} {}",
            rec["label"].as_str().unwrap_or("")
        );
        match cat {
            "dimension" if kind == "common_label" => {
                // Presentation: OpenCascade's CommonLabel; the semantic text is an attribute set.
                if pid.and_then(|n| ix.find(n, "attribute")).is_none() {
                    push(key, "common label: semantic text not read".into());
                }
            }
            "dimension" => {
                let found = match pid {
                    Some(n) => ix.find(n, "dimension"),
                    None => (0..r.parts.len()).find_map(|pi| {
                        let p = &r.parts[pi];
                        (0..p.dimensions.len())
                            .find(|&i| {
                                !used.contains(&(pi, "dimension", i))
                                    && (record_dim_kind(&p.dimensions[i]) == kind
                                        || (kind == "location"
                                            && record_dim_kind(&p.dimensions[i]) == "linear"))
                                    && record_dim_matches(rec, &p.dimensions[i])
                            })
                            .map(|i| (pi, i))
                    }),
                };
                match found {
                    Some((pi, i)) => {
                        used.insert((pi, "dimension", i));
                        let d = &r.parts[pi].dimensions[i];
                        if !record_dim_matches(rec, d) {
                            push(
                                key,
                                format!(
                                    "value {} tol +{}/-{} bounds {}..{} vs {:?}",
                                    rec["value"],
                                    rec["upper_tol"],
                                    rec["lower_tol"],
                                    rec["lower_bound"],
                                    rec["upper_bound"],
                                    (
                                        d.nominal.as_ref().map(|n| n.decimal().to_string()),
                                        &d.tolerance
                                    )
                                ),
                            );
                        }
                    }
                    None => push(
                        key,
                        format!(
                            "no dimension read: value {} tol +{}/-{} bounds {}..{}",
                            rec["value"],
                            rec["upper_tol"],
                            rec["lower_tol"],
                            rec["lower_bound"],
                            rec["upper_bound"]
                        ),
                    ),
                }
            }
            "geometric_tolerance" => {
                let refs: Vec<String> = rec["datum_refs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_str().unwrap_or("").to_string())
                    .collect();
                let mods: Vec<&str> = rec["gtol_modifiers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(|x| x.as_str())
                    .collect();
                let ok = |p: &PartPmi, t: &pmi::GeometricTolerance| {
                    kind_name(t.kind) == kind
                        && t.magnitude.as_ref().is_some_and(|m| {
                            close(mm(m), rec["value"].as_f64().unwrap_or(f64::NAN))
                        })
                        && datum_letters(p, t) == refs
                        && mods.contains(&"maximum_material_requirement")
                            == t.modifiers
                                .contains(&ToleranceModifier::MaximumMaterialRequirement)
                        && mods.contains(&"least_material_requirement")
                            == t.modifiers
                                .contains(&ToleranceModifier::LeastMaterialRequirement)
                        && mods.contains(&"diameter_zone")
                            == t.zone.as_ref().is_some_and(|z| z.form.is_diametral())
                };
                let found = match pid {
                    Some(n) => ix.find(n, "tolerance"),
                    None => (0..r.parts.len()).find_map(|pi| {
                        let p = &r.parts[pi];
                        (0..p.tolerances.len())
                            .find(|&i| {
                                !used.contains(&(pi, "tolerance", i)) && ok(p, &p.tolerances[i])
                            })
                            .map(|i| (pi, i))
                    }),
                };
                match found {
                    Some((pi, i)) => {
                        used.insert((pi, "tolerance", i));
                        let p = &r.parts[pi];
                        let t = &p.tolerances[i];
                        if !ok(p, t) {
                            push(
                                key,
                                format!(
                                    "value {} datums {refs:?} modifiers {mods:?} vs {:?} {:?} {:?} {:?}",
                                    rec["value"],
                                    t.magnitude.as_ref().map(|m| m.decimal().to_string()),
                                    datum_letters(p, t),
                                    t.modifiers,
                                    t.zone.as_ref().map(|z| &z.form)
                                ),
                            );
                        }
                    }
                    None => push(
                        key,
                        format!(
                            "no tolerance read: value {} datums {refs:?} modifiers {mods:?}",
                            rec["value"]
                        ),
                    ),
                }
            }
            "datum" => {
                let label = rec["label"].as_str().unwrap_or("");
                let found = match pid {
                    Some(n) => ix.find(n, "datum").or_else(|| {
                        ix.find(n, "feature").and_then(|(pi, f)| {
                            r.parts[pi]
                                .datums
                                .iter()
                                .position(|d| d.feature() == Some(pmi::FeatureId(f)))
                                .map(|i| (pi, i))
                        })
                    }),
                    None => (0..r.parts.len()).find_map(|pi| {
                        r.parts[pi]
                            .datums
                            .iter()
                            .position(|d| d.label().as_str() == label)
                            .map(|i| (pi, i))
                    }),
                };
                match found {
                    Some((pi, i)) => {
                        if r.parts[pi].datums[i].label().as_str() != label {
                            push(
                                key,
                                format!("label {label} vs {}", r.parts[pi].datums[i].label()),
                            );
                        }
                    }
                    None => push(key, "no datum read".into()),
                }
            }
            "manufacturing_requirement" => {
                // A note, a structured requirement (attribute set) or the general tolerance.
                let label = rec["label"].as_str().unwrap_or("");
                match pid {
                    Some(n) if ix.find(n, "note").is_some() => {
                        let (pi, i) = ix.find(n, "note").unwrap();
                        let note = &r.parts[pi].notes[i];
                        if label != note.text {
                            push(key, format!("{label:?} vs {:?}", note.text));
                        }
                    }
                    Some(n) if ix.find(n, "attribute").is_some() => {}
                    Some(n) if ix.find(n, "general").is_some() => {
                        let (pi, i) = ix.find(n, "general").unwrap();
                        let text = match &r.parts[pi].general[i] {
                            pmi::GeneralTolerance::Class { text, .. } => text.clone(),
                            pmi::GeneralTolerance::Table { name, .. } => name.clone(),
                        };
                        if label != text {
                            push(key, format!("{label:?} vs {text:?}"));
                        }
                    }
                    _ => push(
                        key,
                        "no note, attribute set or general tolerance read".into(),
                    ),
                }
            }
            "surface_label" => {
                if pid.and_then(|n| ix.find(n, "attribute")).is_none() {
                    push(key, "no attribute set read".into());
                }
            }
            other => push(key, format!("record category {other} not compared")),
        }
    }
    out
}

fn datum_letters(p: &PartPmi, t: &pmi::GeometricTolerance) -> Vec<String> {
    t.datums
        .as_ref()
        .map(|s| {
            s.compartments()
                .iter()
                .flat_map(|c| &c.references)
                .map(|x| p.datums[x.datum.0].label().to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn record_dim_kind(d: &pmi::Dimension) -> &'static str {
    match &d.kind {
        DimensionKind::Size { angle: Some(_), .. }
        | DimensionKind::Location { angle: Some(_), .. } => "angular",
        DimensionKind::Size {
            kind: pmi::SizeKind::Diameter,
            ..
        } => "diameter",
        DimensionKind::Size {
            kind: pmi::SizeKind::Radius,
            ..
        } => "radius",
        DimensionKind::Size {
            kind: pmi::SizeKind::Thickness,
            ..
        } => "thickness",
        DimensionKind::Size { .. } => "size",
        DimensionKind::Location { .. } => "linear",
    }
}

/// Whether an extraction record's values are the dimension's (millimetres; degrees).
fn record_dim_matches(rec: &Json, d: &pmi::Dimension) -> bool {
    let si = |v: &pmi::Value| match &v.quantity {
        pmi::Quantity::Length(l) => l.mm(),
        pmi::Quantity::Angle(a) => a.rad().to_degrees(),
    };
    let f = |k: &str| rec[k].as_f64();
    match &d.tolerance {
        DimTolerance::Limits(b) => {
            f("lower_bound").is_some_and(|x| close(x, si(b.lower())))
                && f("upper_bound").is_some_and(|x| close(x, si(b.upper())))
        }
        DimTolerance::Deviations(b) => {
            d.nominal
                .as_ref()
                .is_some_and(|n| f("value").is_some_and(|x| close(x, si(n))))
                && f("upper_tol").is_some_and(|x| close(x, si(b.upper())))
                && f("lower_tol").is_some_and(|x| close(x, -si(b.lower())))
        }
        _ => match &d.nominal {
            Some(n) => f("value").is_some_and(|x| close(x, si(n))),
            None => f("value").is_some_and(|x| x == 0.0),
        },
    }
}

fn pins() -> BTreeMap<String, Json> {
    let path = common::fixtures().join("known_pmi_draftwright.json");
    let all: Json = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let entries = all["differences"].as_array().cloned().unwrap_or_default();
    common::check_verdicts("known_pmi_draftwright.json", &entries);
    let mut out = BTreeMap::new();
    for e in entries {
        let k = e["key"].as_str().unwrap().to_string();
        assert!(
            out.insert(k.clone(), e).is_none(),
            "known_pmi_draftwright.json: {k} listed twice"
        );
    }
    out
}

/// The files draftwright was captured on: the committed NIST models and specify-core outputs.
fn cases() -> Vec<(String, PathBuf)> {
    let dir = common::fixtures().join("ap242/draftwright");
    let mut out = Vec::new();
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    for n in names {
        let stem = n.trim_end_matches(".json.gz").to_string();
        let nist = common::fixtures().join(format!("ap242/nist/{stem}.stp.gz"));
        let specify = common::fixtures().join(format!("ap242/specify/{stem}.step.gz"));
        let path = if nist.exists() { nist } else { specify };
        assert!(path.exists(), "{stem}: no file for the capture");
        out.push((stem, path));
    }
    out
}

#[test]
fn draftwright_parity() {
    let mut diffs = Vec::new();
    for (stem, path) in cases() {
        let dw: Json = serde_json::from_slice(&file_bytes(
            &common::fixtures().join(format!("ap242/draftwright/{stem}.json.gz")),
        ))
        .unwrap();
        let bytes = file_bytes(&path);
        let parts = read_part_definitions(&bytes).unwrap();
        let doc = Document::parse(bytes).unwrap();
        let r = pmi::read(&doc, &parts).unwrap();
        diffs.extend(compare(&stem, &dw, &r, &parts));
    }
    let pins = pins();
    let mut seen = BTreeSet::new();
    let mut unexplained = Vec::new();
    for (k, d) in &diffs {
        if !seen.insert(k.clone()) {
            unexplained.push(format!("{k} (twice): {d}"));
        } else if !pins.contains_key(k) {
            unexplained.push(format!("{k}: {d}"));
        }
    }
    let stale: Vec<&String> = pins.keys().filter(|k| !seen.contains(*k)).collect();
    if !unexplained.is_empty() || !stale.is_empty() {
        let dump: Vec<Json> = diffs
            .iter()
            .map(|(k, d)| json!({"key": k, "difference": d}))
            .collect();
        let _ = std::fs::write(
            std::env::temp_dir().join("pmi_draftwright_differences.json"),
            serde_json::to_string_pretty(&dump).unwrap(),
        );
    }
    assert!(
        unexplained.is_empty(),
        "{} unexplained differences:\n{}",
        unexplained.len(),
        unexplained.join("\n")
    );
    assert!(
        stale.is_empty(),
        "pinned but no longer different: {stale:?}"
    );
}
