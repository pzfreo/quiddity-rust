//! Self-contained publication of fields that cannot use the file's principal axes.
//!
//! Recognition's typed working records remain available through `Deref` for kernel algorithms.
//! Consumers use `families()` or serialization for the published contract. Only records needing
//! an explicit measuring basis acquire fields; ordinary file-axis records retain their bytes.
use std::collections::BTreeMap;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};
use serde_json::{Value, json};

use crate::features::Features;
use crate::framed_records::Rigid;
use crate::kernel::geom::V3;

#[derive(Clone, Debug)]
pub struct PublishedFeatures {
    working: Features,
    records: BTreeMap<String, Value>,
    fingerprint_axes: BTreeMap<(String, String), V3>,
    source: Arc<Features>,
    motion: Rigid,
    reach: f64,
}

impl Deref for PublishedFeatures {
    type Target = Features;
    fn deref(&self) -> &Features {
        &self.working
    }
}
impl DerefMut for PublishedFeatures {
    fn deref_mut(&mut self) -> &mut Features {
        &mut self.working
    }
}

impl PublishedFeatures {
    pub(crate) fn new(
        working: Features,
        pending: BTreeMap<String, Vec<String>>,
        motion: Rigid,
        source: Arc<Features>,
        reach: f64,
    ) -> Self {
        let mut records = BTreeMap::new();
        let mut fingerprint_axes = BTreeMap::new();
        if !pending.is_empty() {
            for (family, list) in crate::recognition::families(&working) {
                for (n, mut record) in list.into_iter().enumerate() {
                    let id = format!("{family}/{n}");
                    let Some(paths) = pending.get(&id) else {
                        continue;
                    };
                    // Preserve the exact v3 intrinsic axis, including the chosen sense. It is
                    // read at extraction time, rather than repairing fingerprints afterwards.
                    if let Some(table) = crate::correspondence::fingerprint::family(&family) {
                        for &(path, role) in table.fields {
                            if role == crate::correspondence::fingerprint::Role::Axis
                                && paths.iter().any(|p| p == path)
                                && let Some(i) = record.get(path).and_then(axis_index)
                            {
                                fingerprint_axes
                                    .insert((id.clone(), path.into()), column(&motion, i));
                            }
                        }
                    }
                    if family == "paired_ramp_steps" && paths.iter().any(|p| p == "half_widths") {
                        let i = axis_index(&json!(source.paired_ramp_steps[n].half_width_axis))
                            .expect("width axis");
                        let (d, sign) = direction(&motion, i, reach);
                        record["width_axis"] = d;
                        if sign < 0.
                            && let Some(pair) = record["half_widths"].as_array_mut()
                        {
                            pair.reverse();
                        }
                    }
                    convert(&mut record, paths, &motion, &family, reach);
                    records.insert(id, record);
                }
            }
        }
        Self {
            working,
            records,
            fingerprint_axes,
            source,
            motion,
            reach,
        }
    }

    /// All document families, including refusals, in registry order. Each payload is usable
    /// without the document's provenance frame.
    pub fn families(&self) -> Vec<(String, Vec<Value>)> {
        let mut families = crate::recognition::families(&self.working);
        for (family, list) in &mut families {
            for (n, record) in list.iter_mut().enumerate() {
                if let Some(published) = self.records.get(&format!("{family}/{n}")) {
                    *record = published.clone();
                }
            }
        }
        families
    }

    pub(crate) fn fingerprint_axis(&self, id: &str, path: &str) -> Option<V3> {
        self.fingerprint_axes
            .get(&(id.into(), path.into()))
            .copied()
    }
}

impl Serialize for PublishedFeatures {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.records.is_empty() {
            return self.working.serialize(serializer);
        }
        // Preserve the old top-level family coverage and order, including omitted empty threads.
        let raw = crate::recognition::families(&self.working);
        let mut map = serializer.serialize_map(None)?;
        for (family, mut list) in raw {
            if ["section_recess_patterns", "modelled_thread_refusals"].contains(&family.as_str()) {
                continue;
            }
            if family == "interior_voids" {
                list.truncate(self.working.interior_voids.len());
            }
            if family == "section_recesses" {
                list.truncate(self.working.section_recesses.len());
            }
            for (n, record) in list.iter_mut().enumerate() {
                if let Some(published) = self.records.get(&format!("{family}/{n}")) {
                    *record = published.clone();
                }
            }
            map.serialize_entry(&family, &list)?;
        }
        map.end()
    }
}

/// Input to a subsequent rigid mapping. Retaining the recognition source lets motions compose
/// without decoding vector records back into frame letters or applying publication rounding twice.
pub struct MappingSource {
    pub(crate) features: Features,
    pub(crate) motion: Rigid,
    pub(crate) reach: f64,
}
const IDENTITY: Rigid = Rigid {
    rotation: [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
    translation: [0.; 3],
};
impl From<Features> for MappingSource {
    fn from(features: Features) -> Self {
        Self {
            features,
            motion: IDENTITY,
            reach: f64::INFINITY,
        }
    }
}
impl From<PublishedFeatures> for MappingSource {
    fn from(features: PublishedFeatures) -> Self {
        Self {
            features: (*features.source).clone(),
            motion: features.motion,
            reach: features.reach,
        }
    }
}
impl From<Features> for PublishedFeatures {
    fn from(features: Features) -> Self {
        Self::new(
            features.clone(),
            BTreeMap::new(),
            IDENTITY,
            Arc::new(features),
            f64::INFINITY,
        )
    }
}

fn axis_index(v: &Value) -> Option<usize> {
    match v.as_str()? {
        "x" => Some(0),
        "y" => Some(1),
        "z" => Some(2),
        _ => None,
    }
}
fn column(m: &Rigid, i: usize) -> V3 {
    std::array::from_fn(|w| m.rotation[w][i])
}

/// A signed measuring vector becomes a positive file-axis letter only when exactly aligned
/// after #70's reach-bounded snap. Its coordinates are multiplied by the returned sign.
fn direction(m: &Rigid, i: usize, reach: f64) -> (Value, f64) {
    let d = column(m, i);
    let w = (0..3)
        .max_by(|&a, &b| d[a].abs().total_cmp(&d[b].abs()))
        .unwrap();
    let sign = 1_f64.copysign(d[w]);
    let squared_deviation = |d: V3| {
        let w = (0..3)
            .max_by(|&a, &b| d[a].abs().total_cmp(&d[b].abs()))
            .unwrap();
        let sign = 1_f64.copysign(d[w]);
        (0..3)
            .map(|k| (d[k] - if k == w { sign } else { 0. }).powi(2))
            .sum::<f64>()
    };
    let deviation = squared_deviation(d);
    // Bound the combined change of every snapped column, as #70 bounds the whole
    // rotation. Three individually small snaps must not together exceed the coordinate floor.
    let combined = (0..3)
        .map(|j| squared_deviation(column(m, j)))
        .filter(|&v| v == 0. || v.sqrt() * reach <= crate::kernel::geom::COORD_FLOOR)
        .sum::<f64>()
        .sqrt();
    if deviation == 0.
        || (deviation.sqrt() * reach <= crate::kernel::geom::COORD_FLOOR
            && combined * reach <= crate::kernel::geom::COORD_FLOOR)
    {
        (json!(["x", "y", "z"][w]), sign)
    } else {
        (json!(d), 1.)
    }
}
fn scaled(v: &mut Value, sign: f64, interval: bool) {
    if sign == 1. {
        return;
    }
    match v {
        Value::Number(n) => *v = json!(-n.as_f64().expect("coordinate")),
        Value::Array(a) => {
            for v in a.iter_mut() {
                scaled(v, sign, false);
            }
            if interval {
                a.reverse();
            }
        }
        Value::Null => {}
        _ => panic!("invalid coordinate {v}"),
    }
}

fn convert(record: &mut Value, paths: &[String], m: &Rigid, family: &str, reach: f64) {
    // Walk nested members with the same contract; no record asks its reader for a part frame.
    let mut nested: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut direct = Vec::new();
    for path in paths {
        if let Some((head, tail)) = path.split_once('.') {
            nested.entry(head.into()).or_default().push(tail.into());
        } else {
            direct.push(path.as_str());
        }
    }
    for (head, tails) in nested {
        if let Some(name) = head.strip_suffix("[]") {
            if let Some(members) = record.get_mut(name).and_then(Value::as_array_mut) {
                for member in members {
                    convert(
                        member,
                        &tails,
                        m,
                        if name == "body_levels" {
                            "step_levels"
                        } else {
                            name
                        },
                        reach,
                    );
                }
            }
        } else if let Some(member) = record.get_mut(&head) {
            convert(member, &tails, m, &head, reach);
        }
    }
    if direct.is_empty() {
        return;
    }
    let raw = record.clone();
    let obj = record.as_object_mut().expect("record object");
    let has = |p: &str| direct.contains(&p);
    let axis = raw.get("axis").and_then(axis_index);
    let width = raw.get("width_axis").and_then(axis_index);
    let long = raw.get("long_axis").and_then(axis_index);
    let depth = raw
        .get("depth_axis")
        .and_then(axis_index)
        .or_else(|| width.zip(long).map(|(w, l)| 3 - w - l));
    let mut uses_origin = false;
    for &key in &direct {
        assert!(
            [
                "axis",
                "width_axis",
                "long_axis",
                "depth_axis",
                "thickness_axis",
                "other_axis",
                "lo",
                "hi",
                "base",
                "top",
                "span",
                "run_interval",
                "thickness_bounds",
                "w_center",
                "d_lo",
                "d_hi",
                "z",
                "z_lo",
                "z_hi",
                "x_span",
                "y_span",
                "x0",
                "x1",
                "y0",
                "y1",
                "z0",
                "z1",
                "positions",
                "other_positions",
                "body_key",
                "body_bounds",
                "axis_origin",
                "axis_line",
                "supports",
                "directions",
                "legs",
                "mirror_plane",
                "section",
                "endpoint_scopes",
                "half_widths",
                "open_sign",
                "depth_sign",
                "lo_at_envelope",
                "hi_at_envelope",
                "u",
                "v",
                "direction",
                "row_direction",
                "col_direction",
                "center"
            ]
            .contains(&key),
            "{family}.{key}: missing publication conversion"
        );
        if let Some(v) = raw.get(key)
            && let Some(i) = axis_index(v)
        {
            obj.insert(key.into(), direction(m, i, reach).0);
        }
        let coordinate_axis = match key {
            "lo" | "hi" | "base" | "top" | "span" | "run_interval" | "thickness_bounds" => long
                .or(axis)
                .or_else(|| raw.get("thickness_axis").and_then(axis_index)),
            "w_center" => width,
            "d_lo" | "d_hi" => depth,
            "z" | "z_lo" | "z_hi" => Some(2),
            "x_span" | "x0" | "x1" => Some(0),
            "y_span" | "y0" | "y1" => Some(1),
            "z0" | "z1" => Some(2),
            "positions" => axis,
            "other_positions" => raw.get("other_axis").and_then(axis_index),
            _ => None,
        };
        if let Some(i) = coordinate_axis {
            uses_origin = true;
            if let Some(v) = obj.get_mut(key) {
                scaled(
                    v,
                    direction(m, i, reach).1,
                    [
                        "span",
                        "run_interval",
                        "thickness_bounds",
                        "x_span",
                        "y_span",
                        "positions",
                        "other_positions",
                    ]
                    .contains(&key),
                );
            }
        }
        if ["body_key", "body_bounds"].contains(&key)
            && let Some(bounds) = raw.get(key).and_then(Value::as_array)
        {
            let interleaved = key == "body_bounds";
            let intervals: Vec<Value> = (0..3)
                .map(|i| {
                    let (a, b) = if interleaved {
                        (2 * i, 2 * i + 1)
                    } else {
                        (i, i + 3)
                    };
                    let mut pair = json!([bounds[a], bounds[b]]);
                    scaled(&mut pair, direction(m, i, reach).1, true);
                    pair
                })
                .collect();
            let mut box_value = json!({"origin":m.translation,"axes":(0..3).map(|i| direction(m,i,reach).0).collect::<Vec<_>>(),"bounds":intervals});
            if key == "body_key" {
                box_value["volume"] = bounds[6].clone();
                box_value["area"] = bounds[7].clone();
            }
            obj.insert(key.into(), box_value);
        }
        if key == "axis_origin"
            && let Some(p) = raw.get(key).and_then(Value::as_array)
        {
            let p = std::array::from_fn(|i| p[i].as_f64().expect("point"));
            obj.insert(key.into(), json!(m.point(p)));
        }
    }
    if has("supports") {
        uses_origin = true;
        let mut supports = raw["supports"].as_array().expect("two supports").clone();
        let mut signs = Vec::new();
        for support in &mut supports {
            let pair = support.as_array_mut().expect("support plane");
            let i = axis_index(&pair[0]).expect("support axis");
            let (d, s) = direction(m, i, reach);
            pair[0] = d;
            scaled(&mut pair[1], s, false);
            signs.push(s);
        }
        obj.insert("supports".into(), json!(supports));
        if let Some(directions) = obj.get_mut("directions").and_then(Value::as_array_mut) {
            for (v, s) in directions.iter_mut().zip(signs) {
                scaled(v, s, false);
            }
        }
    }
    if has("mirror_plane") {
        uses_origin = true;
        let mut pair = raw["mirror_plane"]
            .as_array()
            .expect("mirror plane")
            .clone();
        let i = axis_index(&pair[0]).expect("mirror axis");
        let (d, s) = direction(m, i, reach);
        pair[0] = d;
        scaled(&mut pair[1], s, false);
        obj.insert("mirror_plane".into(), json!(pair));
    }
    for key in ["direction", "row_direction", "col_direction", "center"] {
        if has(key)
            && let Some(p) = raw.get(key).and_then(Value::as_array)
            && p.len() == 3
        {
            let v = std::array::from_fn(|i| p[i].as_f64().expect("vector"));
            obj.insert(
                key.into(),
                json!(if key == "center" {
                    m.point(v)
                } else {
                    m.direction(v)
                }),
            );
        }
    }
    // Order interval endpoints and associated end flags in the published vector's sense.
    for (lo, hi, i) in [
        ("lo", "hi", long.or(axis)),
        ("base", "top", axis),
        ("d_lo", "d_hi", depth),
        ("z_lo", "z_hi", Some(2)),
        ("x0", "x1", Some(0)),
        ("y0", "y1", Some(1)),
        ("z0", "z1", Some(2)),
    ] {
        if has(lo) && has(hi) && i.is_some_and(|i| direction(m, i, reach).1 < 0.) {
            let a = obj[lo].clone();
            let b = obj[hi].clone();
            obj.insert(lo.into(), b);
            obj.insert(hi.into(), a);
            if lo == "z_lo" {
                let a = obj["lo_at_envelope"].clone();
                let b = obj["hi_at_envelope"].clone();
                obj.insert("lo_at_envelope".into(), b);
                obj.insert("hi_at_envelope".into(), a);
            }
        }
    }
    for (key, i) in [
        ("open_sign", depth.or(axis)),
        ("depth_sign", depth),
        ("direction", axis),
    ] {
        if has(key)
            && let Some(i) = i
            && let Some(v) = obj.get_mut(key)
        {
            scaled(v, direction(m, i, reach).1, false);
        }
    }
    if uses_origin {
        obj.insert("origin".into(), json!(m.translation));
    }
    if width.is_some() && long.is_some() && has("d_lo") {
        obj.insert("depth_axis".into(), direction(m, depth.unwrap(), reach).0);
    }
    if family == "step_levels" || family == "risers" {
        if has("z") || has("z_lo") {
            obj.insert("height_axis".into(), direction(m, 2, reach).0);
        }
        if has("x_span") {
            obj.insert("x_axis".into(), direction(m, 0, reach).0);
        }
        if has("y_span") {
            obj.insert("y_axis".into(), direction(m, 1, reach).0);
        }
    }
    if let Some(i) = axis {
        let across: Vec<_> = (0..3).filter(|&k| k != i).collect();
        if has("section") || has("axis_line") || has("u") || has("v") {
            for key in ["section", "axis_line"] {
                if has(key)
                    && let Some(v) = obj.get_mut(key)
                {
                    across_coordinates(
                        v,
                        [
                            direction(m, across[0], reach).1,
                            direction(m, across[1], reach).1,
                        ],
                    );
                }
            }
            obj.insert(
                "section_axes".into(),
                json!(
                    across
                        .iter()
                        .map(|&k| direction(m, k, reach).0)
                        .collect::<Vec<_>>()
                ),
            );
            obj.insert("origin".into(), json!(m.translation));
            for (key, k) in [("u", across[0]), ("v", across[1])] {
                if has(key)
                    && let Some(v) = obj.get_mut(key)
                {
                    scaled(v, direction(m, k, reach).1, false);
                }
            }
        }
    }
    if ["pads"].contains(&family) {
        obj.insert(
            "basis_axes".into(),
            json!((0..3).map(|i| direction(m, i, reach).0).collect::<Vec<_>>()),
        );
    }
}

// Re-express section points and traversal sense when a snapped basis direction reverses.
fn across_coordinates(v: &mut Value, signs: [f64; 2]) {
    match v {
        Value::Array(a) if a.len() == 2 && a.iter().all(Value::is_number) => {
            for (v, s) in a.iter_mut().zip(signs) {
                scaled(v, s, false);
            }
        }
        Value::Array(a) => {
            for v in a {
                across_coordinates(v, signs);
            }
        }
        Value::Object(o) => {
            for (k, v) in o {
                if ["sweep", "bulge"].contains(&k.as_str()) {
                    scaled(v, signs[0] * signs[1], false);
                } else {
                    across_coordinates(v, signs);
                }
            }
        }
        _ => {}
    }
}
