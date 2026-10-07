//! Revision-stable fingerprints of a recognition result's features and of the part's faces
//! (`docs/correspondence.md`, "Fingerprints"). Everything in them is unchanged by a rigid motion
//! of the part except the placements (positions, axes, normals), which move with it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::features::Features;
use crate::kernel::brep::{Arc, Part};
use crate::kernel::geom::{self, Surface, V3};

/// Bumped whenever a fingerprint's content or meaning changes: [`super::correspond`] refuses to
/// compare fingerprints of two versions.
pub const FINGERPRINT_VERSION: &str = "quiddity-rust/fingerprint/1";

/// The fingerprints of one recognition result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Fingerprints {
    pub fingerprint_version: String,
    /// The part's size, mm: the square root of its faces' total area (unchanged by motion,
    /// unlike a box). Tolerances and placement costs are relative to it.
    pub scale: f64,
    pub features: Vec<FeatureFingerprint>,
    pub faces: Vec<FaceFingerprint>,
}

/// A recognised feature.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FeatureFingerprint {
    /// `<family>/<index>`: the record's place in the family's list, so stable within one result
    /// only.
    pub id: String,
    pub family: String,
    /// Semantics, the subtype: the record's categorical fields (a hole's `bottom`, a fillet's
    /// `side`, whether a counterbore is present, a pattern's shape), by name.
    pub traits: BTreeMap<String, String>,
    /// Semantics, the intrinsic sizes: the record's size fields by name (a counterbore's diameter
    /// is `cbore.diameter`), and a pattern's member `count`.
    pub sizes: BTreeMap<String, f64>,
    /// Placement: the feature's axis, normal or run, as a unit vector (its sign is not
    /// significant).
    pub axis: Option<V3>,
    /// Placement: the area-weighted centroid of its faces.
    pub position: Option<V3>,
    /// The part's faces that define it, ascending (a pattern's are its members').
    pub faces: Vec<usize>,
    /// Neighbourhood: for each of its faces and each face that one meets,
    /// `type>in|out:type:convex|concave|smooth|unknown` (`in` when the neighbour is its own),
    /// sorted: its subgraph of the attributed adjacency graph, as draftwright's bridge writes it.
    pub neighbourhood: Vec<String>,
}

/// A face of the part.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FaceFingerprint {
    pub index: usize,
    /// `plane`, `cylinder`, `cone`, `sphere`, `torus`, or the file's name for a freeform surface
    /// that is not exactly one of those.
    pub surface: String,
    /// Intrinsic surface parameters: `radius`, `semi_angle` (degrees), `major_radius`,
    /// `minor_radius`.
    pub parameters: BTreeMap<String, f64>,
    /// The surface's axis (cylinder, cone, torus) or a plane's outward normal, as a unit vector.
    pub axis: Option<V3>,
    /// A point the surface is placed by: a cylinder's or torus's axis point, a cone's apex, a
    /// sphere's centre, a plane's centroid.
    pub support: Option<V3>,
    /// None where the boundary quadrature cannot integrate the face.
    pub area: Option<f64>,
    /// The area-weighted centroid; where the area is unknown, the mean of its edge samples.
    pub centroid: V3,
    /// The mean outward normal (the unit area vector), where the face has one.
    pub normal: Option<V3>,
    /// Its neighbours across shared edges, with how the solid turns there.
    pub neighbours: Vec<(usize, String)>,
    /// Its adjacency signature: `type:arc` for each neighbour, sorted.
    pub adjacency: Vec<String>,
}

/// The record fields that are a feature's intrinsic sizes. Positions, frame-aligned bounds and
/// body keys are left out: they move with the part.
const SIZES: [&str; 32] = [
    "across",
    "across_flats",
    "angle",
    "axial_length",
    "col_pitch",
    "cols",
    "corner_radius",
    "count",
    "depth",
    "diameter",
    "drill_diameter",
    "end_radius",
    "flat_width",
    "half_width",
    "height",
    "height_above_base",
    "included_angle",
    "leg1",
    "leg2",
    "legs",
    "length",
    "major_diameter",
    "major_radius",
    "minor_radius",
    "pitch",
    "pitch_degrees",
    "radius",
    "row_pitch",
    "rows",
    "thickness",
    "width",
    "half_widths",
];

/// Fields read as the feature's axis, in order of preference: a vector, or an axis letter.
const AXES: [&str; 6] = [
    "axis",
    "axis_direction",
    "run",
    "direction",
    "thickness_axis",
    "depth_direction",
];

/// Derived families: their members are records of another family, whose faces they take.
const DERIVED: [(&str, &str, &str); 2] = [
    ("hole_patterns", "holes", "holes"),
    ("gusset_rib_patterns", "ribs", "gusset_ribs"),
];

/// The fingerprints of *features*, recognised on *part*.
pub fn fingerprint(part: &Part, features: &Features) -> Fingerprints {
    let faces: Vec<FaceFingerprint> = (0..part.faces.len()).map(|i| face(part, i)).collect();
    let scale = faces
        .iter()
        .filter_map(|f| f.area)
        .sum::<f64>()
        .sqrt()
        .max(1e-3);
    let json = serde_json::to_value(features).expect("records serialise");
    let mut out = Vec::new();
    for (family, records) in json.as_object().expect("an object") {
        let Some(records) = records.as_array() else {
            continue;
        };
        for (n, record) in records.iter().enumerate() {
            let own = features
                .defining
                .get(family.as_str())
                .and_then(|d| d.get(n))
                .cloned();
            let faces_of = own.unwrap_or_else(|| derived_faces(&json, features, family, record));
            out.push(feature(
                &faces,
                format!("{family}/{n}"),
                family,
                record,
                faces_of,
            ));
        }
    }
    Fingerprints {
        fingerprint_version: FINGERPRINT_VERSION.into(),
        scale,
        features: out,
        faces,
    }
}

/// A derived record's faces: its members' defining faces, each member found among the base
/// family's records (the first not yet taken that is equal to it).
fn derived_faces(json: &Value, features: &Features, family: &str, record: &Value) -> Vec<usize> {
    let Some(&(_, field, base)) = DERIVED.iter().find(|d| d.0 == family) else {
        return Vec::new();
    };
    let (Some(members), Some(pool), Some(defining)) = (
        record.get(field).and_then(Value::as_array),
        json.get(base).and_then(Value::as_array),
        features.defining.get(base),
    ) else {
        return Vec::new();
    };
    let mut taken = vec![false; pool.len()];
    let mut out = Vec::new();
    for m in members {
        if let Some(k) = (0..pool.len()).find(|&k| !taken[k] && pool[k] == *m) {
            taken[k] = true;
            out.extend(defining.get(k).into_iter().flatten().copied());
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

fn feature(
    faces: &[FaceFingerprint],
    id: String,
    family: &str,
    record: &Value,
    mut own: Vec<usize>,
) -> FeatureFingerprint {
    own.sort_unstable();
    own.dedup();
    let mut traits = BTreeMap::new();
    let mut sizes = BTreeMap::new();
    let mut axis = None;
    let Some(fields) = record.as_object() else {
        return FeatureFingerprint {
            id,
            family: family.into(),
            traits,
            sizes,
            axis,
            position: None,
            faces: own,
            neighbourhood: Vec::new(),
        };
    };
    for name in AXES {
        if axis.is_none() {
            axis = fields.get(name).and_then(read_axis);
        }
    }
    let pattern = family.ends_with("_patterns");
    if pattern {
        let keys: Vec<&str> = fields.keys().map(String::as_str).collect();
        traits.insert("shape".into(), keys.join(","));
    }
    for (k, v) in fields {
        // A pattern's angle is its orientation in the part's frame, not a size.
        let size = SIZES.contains(&k.as_str()) && !(pattern && k == "angle");
        match v {
            Value::Number(x) if size => {
                sizes.insert(k.clone(), x.as_f64().unwrap_or(0.0));
            }
            Value::String(s) if !matches!(s.as_str(), "x" | "y" | "z") => {
                traits.insert(k.clone(), s.clone());
            }
            Value::Bool(b) => {
                traits.insert(k.clone(), b.to_string());
            }
            Value::Null => {
                traits.insert(k.clone(), "none".into());
            }
            // A pair of sizes is ordered by the frame (a ramp's two half-widths), so it is
            // kept smallest first.
            Value::Array(items) if size && items.iter().all(Value::is_number) => {
                let mut values: Vec<f64> = items.iter().filter_map(Value::as_f64).collect();
                values.sort_by(f64::total_cmp);
                for (i, x) in values.into_iter().enumerate() {
                    sizes.insert(format!("{k}.{i}"), x);
                }
            }
            Value::Array(items) if !items.is_empty() && items.iter().all(Value::is_object) => {
                sizes.insert("count".into(), items.len() as f64);
            }
            Value::Object(nested) => {
                traits.insert(k.clone(), "present".into());
                for (n, x) in nested {
                    if let (true, Some(x)) = (SIZES.contains(&n.as_str()), x.as_f64()) {
                        sizes.insert(format!("{k}.{n}"), x);
                    }
                }
            }
            _ => {}
        }
    }
    let position = weighted_centroid(faces, &own);
    let neighbourhood = neighbourhood(faces, &own);
    FeatureFingerprint {
        id,
        family: family.into(),
        traits,
        sizes,
        axis,
        position,
        faces: own,
        neighbourhood,
    }
}

/// A unit vector from a 3-vector or an axis letter.
fn read_axis(v: &Value) -> Option<V3> {
    match v {
        Value::String(s) => match s.as_str() {
            "x" => Some([1.0, 0.0, 0.0]),
            "y" => Some([0.0, 1.0, 0.0]),
            "z" => Some([0.0, 0.0, 1.0]),
            _ => None,
        },
        Value::Array(items) if items.len() == 3 => {
            let c: Vec<f64> = items.iter().filter_map(Value::as_f64).collect();
            (c.len() == 3)
                .then(|| geom::unit([c[0], c[1], c[2]]))
                .flatten()
        }
        _ => None,
    }
}

/// The area-weighted centroid of the faces (each face's centroid weighted equally where an
/// area is unknown and no face has one).
fn weighted_centroid(faces: &[FaceFingerprint], own: &[usize]) -> Option<V3> {
    if own.is_empty() {
        return None;
    }
    let (mut sum, mut weight) = ([0.0; 3], 0.0);
    for &f in own {
        if let Some(a) = faces[f].area {
            sum = geom::add(sum, geom::scale(faces[f].centroid, a));
            weight += a;
        }
    }
    if weight > 0.0 {
        return Some(geom::scale(sum, 1.0 / weight));
    }
    let sum = own
        .iter()
        .fold([0.0; 3], |s, &f| geom::add(s, faces[f].centroid));
    Some(geom::scale(sum, 1.0 / own.len() as f64))
}

fn neighbourhood(faces: &[FaceFingerprint], own: &[usize]) -> Vec<String> {
    let mut out = Vec::new();
    for &f in own {
        for (g, arc) in &faces[f].neighbours {
            let side = if own.contains(g) { "in" } else { "out" };
            out.push(format!(
                "{}>{side}:{}:{arc}",
                faces[f].surface, faces[*g].surface
            ));
        }
    }
    out.sort();
    out
}

/// Normals at an edge closer than this to parallel (or to opposite) leave convex and concave
/// to round-off: about 2.6 degrees (a B-spline blend exported as an approximate tangent, a knife
/// edge where a face folds back onto its neighbour).
const NEAR_TANGENT: f64 = 1e-3;

/// How the solid turns where faces *a* and *b* meet, as the fingerprint records it: the kernel's
/// [`Part::arc`], except that a nearly tangent edge is `smooth` and a nearly folded one
/// `unknown`, since which way such an edge turns can change when the part is merely moved.
fn arc_label(part: &Part, a: usize, b: usize) -> &'static str {
    let arc = part.arc(a, b);
    for e in part.shared_edges(a, b) {
        let p = part.edges[e].midpoint();
        let normal = |f: usize| {
            let (u, v) = part.faces[f].surface.parameters(p, None)?;
            part.face_normal(f, u, v)
        };
        if let (Some(x), Some(y)) = (normal(a), normal(b)) {
            let c = geom::dot(x, y);
            if 1.0 - c < NEAR_TANGENT {
                return "smooth";
            }
            if 1.0 + c < NEAR_TANGENT {
                return "unknown";
            }
        }
    }
    arc_name(arc)
}

fn arc_name(arc: Option<Arc>) -> &'static str {
    match arc {
        Some(Arc::Convex) => "convex",
        Some(Arc::Concave) => "concave",
        Some(Arc::Smooth) => "smooth",
        _ => "unknown",
    }
}

/// The face's surface as the recognisers see it: a freeform face that is exactly an analytic
/// surface counts as that surface.
fn effective(part: &Part, i: usize) -> &Surface {
    match &part.faces[i].surface {
        Surface::Freeform { .. } => part.recovered(i).unwrap_or(&part.faces[i].surface),
        s => s,
    }
}

fn surface_name(s: &Surface) -> String {
    match s {
        Surface::Plane { .. } => "plane".into(),
        Surface::Cylinder { .. } => "cylinder".into(),
        Surface::Cone { .. } => "cone".into(),
        Surface::Sphere { .. } => "sphere".into(),
        Surface::Torus { .. } => "torus".into(),
        Surface::Freeform { kind, .. } | Surface::Other { kind } => kind.to_lowercase(),
    }
}

fn face(part: &Part, i: usize) -> FaceFingerprint {
    let surface = effective(part, i);
    // A face whose quadrature gives no area (a kernel failure: such faces exist) is fingerprinted
    // as one of unknown area.
    let moments = part.face_moments(i).filter(|m| m.area > 1e-9);
    let centroid = moments.map(|m| m.centroid).unwrap_or_else(|| {
        let (mut sum, mut n) = ([0.0; 3], 0usize);
        for e in part.face_edges(i) {
            for p in &part.edges[e].samples {
                sum = geom::add(sum, *p);
                n += 1;
            }
        }
        geom::scale(sum, 1.0 / n.max(1) as f64)
    });
    let normal = moments.and_then(|m| {
        let length = geom::norm(m.area_vector);
        (length > 1e-6 * m.area).then(|| geom::scale(m.area_vector, 1.0 / length))
    });
    let mut parameters = BTreeMap::new();
    let (axis, support) = match surface {
        Surface::Plane { frame } => {
            let n = if part.faces[i].reversed {
                geom::scale(frame.z, -1.0)
            } else {
                frame.z
            };
            // A recovered plane is unoriented: take the side the area vector is on.
            let n = match normal {
                Some(m) if geom::dot(m, n) < 0.0 => geom::scale(n, -1.0),
                _ => n,
            };
            (Some(n), Some(centroid))
        }
        Surface::Cylinder { frame, radius } => {
            parameters.insert("radius".into(), *radius);
            (Some(frame.z), Some(frame.origin))
        }
        Surface::Cone {
            frame,
            radius,
            semi_angle,
        } => {
            parameters.insert("semi_angle".into(), semi_angle.abs().to_degrees());
            let apex = surface.cone_apex().unwrap_or_else(|| {
                geom::add(
                    frame.origin,
                    geom::scale(frame.z, -radius / semi_angle.tan()),
                )
            });
            (Some(frame.z), Some(apex))
        }
        Surface::Sphere { frame, radius } => {
            parameters.insert("radius".into(), *radius);
            (None, Some(frame.origin))
        }
        Surface::Torus {
            frame,
            major,
            minor,
        } => {
            parameters.insert("major_radius".into(), *major);
            parameters.insert("minor_radius".into(), *minor);
            (Some(frame.z), Some(frame.origin))
        }
        _ => (None, None),
    };
    let name = surface_name(surface);
    let neighbours: Vec<(usize, String)> = part
        .neighbours(i)
        .into_iter()
        .map(|g| (g, arc_label(part, i, g).to_string()))
        .collect();
    let mut adjacency: Vec<String> = neighbours
        .iter()
        .map(|(g, arc)| format!("{}:{arc}", surface_name(effective(part, *g))))
        .collect();
    adjacency.sort();
    FaceFingerprint {
        index: i,
        surface: name,
        parameters,
        axis,
        support,
        area: moments.map(|m| m.area),
        centroid,
        normal,
        neighbours,
        adjacency,
    }
}
