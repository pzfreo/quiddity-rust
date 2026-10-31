//! Read file-owned callouts without regenerating their shape or dropping unlinked graphics.
use super::{Error, Result, Semantic};
use crate::{
    express, geom,
    p21::{Attribute as A, Document, RawEntity},
    pmi::{PartId, PmiRead},
};
use serde::Serialize;
#[cfg(feature = "json-import")]
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[path = "presentation_layout.rs"]
mod layout;
pub use layout::{Plane, TextPlacement, View};

#[derive(Clone, Debug, Serialize, PartialEq)]
#[cfg_attr(feature = "json-import", derive(serde::Deserialize))]
#[serde(deny_unknown_fields)]
pub struct Graphic {
    /// Document-local identity, shared by copies under multiple semantic keys.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plane_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub text_placements: Vec<TextPlacement>,
    pub source: Source,
    pub name: String,
    pub polylines: Vec<Vec<[f64; 3]>>,
    pub triangles: Vec<[[f64; 3]; 3]>,
    pub text: Vec<String>,
    pub step_ids: Vec<u64>,
    pub unsupported: Vec<String>,
}
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-import", derive(serde::Deserialize))]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Generated,
    File,
}
#[derive(Clone, Debug, Serialize, PartialEq)]
#[cfg_attr(feature = "json-import", derive(serde::Deserialize))]
#[serde(deny_unknown_fields)]
pub struct Presentation {
    pub schema: String,
    pub length_unit: String,
    pub semantic_ids: Vec<String>,
    pub items: BTreeMap<String, Vec<Graphic>>,
    pub unlinked: Vec<Graphic>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub planes: Vec<Plane>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub views: Vec<View>,
}
impl Default for Presentation {
    fn default() -> Self {
        Self {
            schema: "haecceity.presentation/1".into(),
            length_unit: "millimetre".into(),
            semantic_ids: vec![],
            items: BTreeMap::new(),
            unlinked: vec![],
            planes: vec![],
            views: vec![],
        }
    }
}
impl Graphic {
    fn new(source: Source, name: String) -> Self {
        Self {
            id: None,
            plane_id: None,
            text_placements: vec![],
            source,
            name,
            polylines: vec![],
            triangles: vec![],
            text: vec![],
            step_ids: vec![],
            unsupported: vec![],
        }
    }
}

fn names(doc: &Document, id: u64) -> Vec<String> {
    match doc.get(id) {
        Some(RawEntity::Simple { name, .. }) => vec![name.to_ascii_lowercase()],
        Some(RawEntity::Complex { parts, .. }) => {
            parts.iter().map(|p| p.name.to_ascii_lowercase()).collect()
        }
        None => vec![],
    }
}
pub(super) fn is(doc: &Document, id: u64, t: &str) -> bool {
    names(doc, id)
        .iter()
        .any(|n| express::TARGET.is_a(n, t) || express::AP242_ED1.is_a(n, t))
}
pub(super) fn attr<'a>(doc: &'a Document, id: u64, entity: &str, name: &str) -> Option<&'a A> {
    match doc.get(id)? {
        RawEntity::Simple {
            name: ty,
            attributes,
            ..
        } => {
            let slots = express::TARGET
                .explicit_attributes(ty)
                .or_else(|| express::AP242_ED1.explicit_attributes(ty))?;
            attributes.get(
                slots
                    .iter()
                    .position(|s| s.entity == entity && s.name == name)?,
            )
        }
        RawEntity::Complex { parts, .. } => {
            let p = parts.iter().find(|p| p.name.eq_ignore_ascii_case(entity))?;
            let decl = express::TARGET
                .entity(entity)
                .or_else(|| express::AP242_ED1.entity(entity))?;
            p.attributes
                .get(decl.attributes.iter().position(|a| a.name == name)?)
        }
    }
}
pub(super) fn reference(a: Option<&A>) -> Option<u64> {
    if let Some(A::EntityRef(id)) = a {
        Some(*id)
    } else {
        None
    }
}
fn list(a: Option<&A>) -> &[A] {
    if let Some(A::List(v)) = a { v } else { &[] }
}
fn refs(a: Option<&A>) -> Vec<u64> {
    list(a).iter().filter_map(|a| reference(Some(a))).collect()
}
fn text(a: Option<&A>) -> String {
    if let Some(A::String(s)) = a {
        s.clone()
    } else {
        String::new()
    }
}
fn number(a: &A) -> Option<f64> {
    match a {
        A::Real(n) => Some(*n),
        A::Integer(n) => Some(*n as f64),
        _ => None,
    }
}
fn xyz(a: Option<&A>) -> Option<[f64; 3]> {
    let v = list(a);
    if v.len() != 3 {
        return None;
    }
    Some([number(&v[0])?, number(&v[1])?, number(&v[2])?])
}
fn point(doc: &Document, id: u64) -> Option<[f64; 3]> {
    xyz(attr(doc, id, "cartesian_point", "coordinates"))
}
fn direction(doc: &Document, id: u64) -> Option<[f64; 3]> {
    geom::unit(xyz(attr(doc, id, "direction", "direction_ratios"))?)
}
#[derive(Clone, Copy)]
struct Transform {
    origin: [f64; 3],
    axes: [[f64; 3]; 3],
}
impl Transform {
    fn identity() -> Self {
        Self {
            origin: [0.; 3],
            axes: [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
        }
    }
    fn at(self, p: [f64; 3]) -> [f64; 3] {
        std::array::from_fn(|i| {
            self.origin[i] + (0..3).map(|j| self.axes[j][i] * p[j]).sum::<f64>()
        })
    }
    fn then(self, child: Self) -> Self {
        Self {
            origin: self.at(child.origin),
            axes: child
                .axes
                .map(|a| std::array::from_fn(|i| (0..3).map(|j| self.axes[j][i] * a[j]).sum())),
        }
    }
}
fn placement(doc: &Document, id: u64) -> Option<Transform> {
    let location = reference(attr(doc, id, "placement", "location"))?;
    let origin = if is(doc, id, "axis2_placement_2d") {
        let coordinates = list(attr(doc, location, "cartesian_point", "coordinates"));
        [
            number(coordinates.first()?)?,
            number(coordinates.get(1)?)?,
            0.,
        ]
    } else if is(doc, id, "axis2_placement_3d") {
        point(doc, location)?
    } else {
        return None;
    };
    if is(doc, id, "axis2_placement_2d") {
        let x = reference(attr(doc, id, "axis2_placement_2d", "ref_direction"))
            .and_then(|r| {
                let ratios = list(attr(doc, r, "direction", "direction_ratios"));
                geom::unit([number(ratios.first()?)?, number(ratios.get(1)?)?, 0.])
            })
            .unwrap_or([1., 0., 0.]);
        return Some(Transform {
            origin,
            axes: [x, [-x[1], x[0], 0.], [0., 0., 1.]],
        });
    }
    let z = reference(attr(doc, id, "axis2_placement_3d", "axis"))
        .and_then(|r| direction(doc, r))
        .unwrap_or([0., 0., 1.]);
    let proposed = reference(attr(doc, id, "axis2_placement_3d", "ref_direction"))
        .and_then(|r| direction(doc, r))
        .unwrap_or([1., 0., 0.]);
    let x = geom::unit(geom::sub(proposed, geom::scale(z, geom::dot(proposed, z))))?;
    let y = geom::cross(z, x);
    Some(Transform {
        origin,
        axes: [x, y, z],
    })
}
fn children(doc: &Document, id: u64) -> Vec<u64> {
    if is(doc, id, "composite_text") {
        return refs(attr(doc, id, "composite_text", "collected_text"));
    }
    if is(doc, id, "mapped_item") {
        return reference(attr(doc, id, "mapped_item", "mapping_source"))
            .and_then(|r| reference(attr(doc, r, "representation_map", "mapped_representation")))
            .map(|r| refs(attr(doc, r, "representation", "items")))
            .unwrap_or_default();
    }
    if is(doc, id, "draughting_callout") {
        return refs(attr(doc, id, "draughting_callout", "contents"));
    }
    if is(doc, id, "annotation_plane") {
        return refs(attr(doc, id, "annotation_plane", "elements"));
    }
    if is(doc, id, "annotation_occurrence") {
        return reference(attr(doc, id, "styled_item", "item"))
            .into_iter()
            .collect();
    }
    if is(doc, id, "tessellated_geometric_set") {
        return refs(attr(doc, id, "tessellated_geometric_set", "children"));
    }
    if is(doc, id, "geometric_set") {
        return refs(attr(doc, id, "geometric_set", "elements"));
    }
    vec![]
}
fn coordinates(doc: &Document, id: u64, owner: &str) -> Vec<Option<[f64; 3]>> {
    reference(attr(doc, id, owner, "coordinates"))
        .map(|r| {
            list(attr(doc, r, "coordinates_list", "position_coords"))
                .iter()
                .map(|a| xyz(Some(a)))
                .collect()
        })
        .unwrap_or_default()
}
fn indices(a: &A) -> Vec<usize> {
    list(Some(a))
        .iter()
        .map(|v| {
            // Keep every position: deleting a bad index would join different points.
            if let A::Integer(n) = v {
                usize::try_from(*n)
                    .ok()
                    .and_then(|n| n.checked_sub(1))
                    .unwrap_or(usize::MAX)
            } else {
                usize::MAX
            }
        })
        .collect()
}
fn draw(
    doc: &Document,
    id: u64,
    transform: Transform,
    scale: f64,
    g: &mut Graphic,
    seen: &mut BTreeSet<u64>,
    context: layout::TextContext,
) {
    if context.depth > 64 {
        g.unsupported
            .push(format!("#{id}: presentation nesting exceeds 64"));
        return;
    }
    if !seen.insert(id) {
        g.unsupported
            .push(format!("#{id}: cyclic or shared content path"));
        return;
    }
    g.step_ids.push(id);
    let transform =
        if let Some(loc) = reference(attr(doc, id, "repositioned_tessellated_item", "location")) {
            match placement(doc, loc) {
                Some(p) => transform.then(p),
                None => {
                    g.unsupported
                        .push(format!("#{id}: unresolved repositioned placement"));
                    return;
                }
            }
        } else {
            transform
        };
    let transform = if is(doc, id, "mapped_item") {
        match layout::mapping(doc, id) {
            Some(mapping) => transform.then(mapping),
            None => {
                g.unsupported
                    .push(format!("#{id}: unresolved mapped presentation placement"));
                seen.remove(&id);
                return;
            }
        }
    } else {
        transform
    };
    let context = context.at(doc, id);
    if is(doc, id, "annotation_placeholder_occurrence") {
        layout::placeholder(doc, id, transform, scale, g);
        seen.remove(&id);
        return;
    }
    let at = |p| geom::scale(transform.at(p), scale);
    let kids = children(doc, id);
    if !kids.is_empty() {
        for r in kids {
            draw(doc, r, transform, scale, g, seen, context.descend());
        }
        seen.remove(&id);
        return;
    }
    if is(doc, id, "polyline") {
        let points = refs(attr(doc, id, "polyline", "points"));
        let line: Option<Vec<_>> = points.iter().map(|r| point(doc, *r).map(at)).collect();
        if let Some(line) = line {
            g.polylines.push(line);
        } else {
            g.unsupported
                .push(format!("#{id}: unresolved polyline point"));
        }
    } else if is(doc, id, "tessellated_curve_set") {
        let coords = coordinates(doc, id, "tessellated_curve_set");
        for strip in list(attr(doc, id, "tessellated_curve_set", "line_strips")) {
            let line: Option<Vec<_>> = indices(strip)
                .iter()
                .map(|i| coords.get(*i).copied().flatten().map(at))
                .collect();
            if let Some(line) = line {
                g.polylines.push(line);
            } else {
                g.unsupported
                    .push(format!("#{id}: curve index out of range"));
            }
        }
    } else if is(doc, id, "tessellated_surface_set") || is(doc, id, "tessellated_face") {
        let owner = if is(doc, id, "tessellated_surface_set") {
            "tessellated_surface_set"
        } else {
            "tessellated_face"
        };
        let coords = coordinates(doc, id, owner);
        let complex = if is(doc, id, "complex_triangulated_surface_set") {
            "complex_triangulated_surface_set"
        } else {
            "complex_triangulated_face"
        };
        let simple = if owner == "tessellated_face" {
            "triangulated_face"
        } else {
            "triangulated_surface_set"
        };
        let mut triangles = Vec::new();
        let pn = indices(
            attr(doc, id, complex, "pnindex")
                .or_else(|| attr(doc, id, simple, "pnindex"))
                .unwrap_or(&A::Unset),
        );
        for (field, fan) in [("triangle_strips", false), ("triangle_fans", true)] {
            for row in list(attr(doc, id, complex, field)) {
                let row = indices(row);
                for i in 2..row.len() {
                    triangles.push(if fan {
                        [row[0], row[i - 1], row[i]]
                    } else if i % 2 == 0 {
                        [row[i - 2], row[i - 1], row[i]]
                    } else {
                        [row[i - 1], row[i - 2], row[i]]
                    });
                }
            }
        }
        for row in list(
            attr(doc, id, simple, "triangles").or_else(|| attr(doc, id, simple, "coord_index")),
        ) {
            let row = indices(row);
            if let [a, b, c] = row.as_slice() {
                triangles.push([*a, *b, *c]);
            }
        }
        for tri in triangles {
            let vertices: Option<Vec<_>> = tri
                .iter()
                .map(|i| {
                    let i = if pn.is_empty() {
                        Some(*i)
                    } else {
                        pn.get(*i).copied()
                    };
                    i.and_then(|i| coords.get(i)).copied().flatten().map(at)
                })
                .collect();
            if let Some(v) = vertices {
                g.triangles.push([v[0], v[1], v[2]]);
            } else {
                g.unsupported
                    .push(format!("#{id}: triangle index out of range"));
            }
        }
    } else if is(doc, id, "text_literal") {
        layout::literal(doc, id, transform, scale, context, g);
        g.unsupported.push(format!(
            "#{id}: editable literal retained; font glyph outlines unavailable"
        ));
    } else {
        g.unsupported.push(format!(
            "#{id}: unsupported graphic {}",
            names(doc, id).join("+")
        ));
    }
    seen.remove(&id);
}

/// Read the file's own annotations. The semantic item map is for one part; file
/// graphics without a link to that part are retained in `unlinked`.
pub fn read(
    doc: &Document,
    read: &PmiRead,
    part: PartId,
    semantic: &Semantic,
) -> Result<Presentation> {
    if part.0 >= read.parts.len() {
        return Err(Error(format!("unknown part {}", part.0)));
    }
    let mut roots: BTreeSet<_> = doc
        .ids()
        .filter(|id| is(doc, *id, "draughting_callout"))
        .collect();
    let mut contained = BTreeSet::new();
    for &root in &roots {
        let mut stack = children(doc, root);
        while let Some(id) = stack.pop() {
            if contained.insert(id) {
                stack.extend(children(doc, id));
            }
        }
    }
    roots.extend(doc.ids().filter(|id| {
        is(doc, *id, "annotation_occurrence")
            && !is(doc, *id, "annotation_plane")
            && !contained.contains(id)
    }));
    let mut carriers = BTreeMap::<u64, BTreeSet<String>>::new();
    for ((p, c, i), entity) in &read.json_carriers {
        if *p == part
            && let Some(id) = semantic.ids.get(c).and_then(|ids| ids.get(*i))
        {
            carriers.entry(*entity).or_default().insert(id.clone());
        }
    }
    let mut datum_support: BTreeMap<u64, BTreeSet<String>> = BTreeMap::new();
    for (i, datum) in semantic.pmi.datums.iter().enumerate() {
        if let Some(feature) = datum.feature()
            && let Some(carrier) = read
                .json_carriers
                .get(&(part, "features".into(), feature.0))
            && let Some(id) = semantic.ids.get("datums").and_then(|ids| ids.get(i))
        {
            datum_support
                .entry(*carrier)
                .or_default()
                .insert(id.clone());
        }
    }
    // Some features project onto a different native carrier. Resolve the actual
    // datum-feature -> datum relationship rather than guessing by equal faces.
    for relationship in doc.ids().filter(|&id| {
        is(doc, id, "shape_aspect_relationship") && !is(doc, id, "dimensional_location")
    }) {
        let feature = reference(attr(
            doc,
            relationship,
            "shape_aspect_relationship",
            "relating_shape_aspect",
        ));
        let datum = reference(attr(
            doc,
            relationship,
            "shape_aspect_relationship",
            "related_shape_aspect",
        ));
        if let (Some(feature), Some(datum)) = (feature, datum)
            && is(doc, feature, "datum_feature")
            && is(doc, datum, "datum")
            && let Some(keys) = carriers.get(&datum)
        {
            datum_support
                .entry(feature)
                .or_default()
                .extend(keys.iter().cloned());
        }
    }
    let mut support_links: BTreeMap<u64, BTreeSet<String>> = BTreeMap::new();
    let mut links: BTreeMap<u64, BTreeSet<String>> = BTreeMap::new();
    for id in doc
        .ids()
        .filter(|id| is(doc, *id, "draughting_model_item_association"))
    {
        let definition = reference(attr(
            doc,
            id,
            "item_identified_representation_usage",
            "definition",
        ));
        let item = reference(attr(
            doc,
            id,
            "item_identified_representation_usage",
            "identified_item",
        ));
        if let (Some(definition), Some(item)) = (definition, item) {
            if let Some(keys) = carriers.get(&definition) {
                links.entry(item).or_default().extend(keys.iter().cloned());
            }
            if let Some(keys) = datum_support.get(&definition) {
                support_links
                    .entry(item)
                    .or_default()
                    .extend(keys.iter().cloned());
            }
        }
    }
    for (i, callouts) in read.provenance.parts[part.0]
        .note_callouts
        .iter()
        .enumerate()
    {
        if let Some(key) = semantic.ids.get("notes").and_then(|ids| ids.get(i)) {
            for &root in callouts {
                links.entry(root).or_default().insert(key.clone());
            }
        }
    }
    let mut out = Presentation {
        semantic_ids: id_set(semantic)?,
        ..Presentation::default()
    };
    let generated: BTreeSet<_> = doc
        .ids()
        .filter(|&id| {
            is(doc, id, "draughting_model_item_association")
                && text(attr(
                    doc,
                    id,
                    "item_identified_representation_usage",
                    "description",
                )) == "haecceity.generated/1"
        })
        .filter_map(|id| {
            reference(attr(
                doc,
                id,
                "item_identified_representation_usage",
                "identified_item",
            ))
        })
        .collect();
    let plane_index = layout::PlaneIndex::read(doc);
    out.planes = plane_index.planes.clone();
    for root in roots {
        let mut graphic = Graphic::new(
            if generated.contains(&root)
                || children(doc, root)
                    .iter()
                    .any(|child| generated.contains(child))
            {
                Source::Generated
            } else {
                Source::File
            },
            text(attr(doc, root, "representation_item", "name")),
        );
        graphic.id = Some(format!("step:{root}"));
        let context = plane_index.context(root, &mut graphic);
        let scale = scale_for(doc, root);
        draw(
            doc,
            root,
            Transform::identity(),
            scale.unwrap_or(1.),
            &mut graphic,
            &mut BTreeSet::new(),
            context,
        );
        if scale.is_none() {
            graphic.polylines.clear();
            graphic.triangles.clear();
            graphic.text_placements.clear();
            graphic.plane_id = None;
            graphic.unsupported.push(format!(
                "#{root}: length unit unresolved; source retained without guessed coordinates"
            ));
        }
        let mut keys: BTreeSet<_> = graphic
            .step_ids
            .iter()
            .filter_map(|id| links.get(id))
            .flatten()
            .cloned()
            .collect();
        {
            keys.extend(
                graphic
                    .step_ids
                    .iter()
                    .filter_map(|id| support_links.get(id))
                    .flatten()
                    .cloned(),
            );
        }
        graphic.step_ids.sort_unstable();
        graphic.step_ids.dedup();
        graphic.unsupported.sort();
        graphic.unsupported.dedup();
        if keys.is_empty() {
            out.unlinked.push(graphic);
        } else {
            for key in keys {
                out.items.entry(key).or_default().push(graphic.clone());
            }
        }
    }
    out.views = layout::views(doc, &out);
    #[cfg(feature = "json-import")]
    validate(&serde_json::to_value(&out)?)?;
    Ok(out)
}

fn scale_for(doc: &Document, id: u64) -> Option<f64> {
    let mut stack = vec![id];
    let mut seen = BTreeSet::new();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        if is(doc, id, "representation") {
            let context = reference(attr(doc, id, "representation", "context_of_items"))?;
            for unit in refs(attr(doc, context, "global_unit_assigned_context", "units")) {
                if let Some(s) = unit_scale(doc, unit, 0) {
                    return Some(s);
                }
            }
        }
        for &parent in doc.referrers(id) {
            if is(doc, parent, "representation") || !children(doc, parent).is_empty() {
                stack.push(parent);
            }
        }
    }
    None
}
fn unit_scale(doc: &Document, id: u64, depth: usize) -> Option<f64> {
    if depth > 8 || !is(doc, id, "length_unit") {
        return None;
    }
    if is(doc, id, "si_unit") {
        let prefix = match attr(doc, id, "si_unit", "prefix") {
            Some(A::Enum(p)) => p.to_ascii_lowercase(),
            _ => String::new(),
        };
        return match prefix.as_str() {
            "" => Some(1000.),
            "milli" => Some(1.),
            "centi" => Some(10.),
            "micro" => Some(0.001),
            "deci" => Some(100.),
            "kilo" => Some(1e6),
            _ => None,
        };
    }
    let conversion = reference(attr(doc, id, "conversion_based_unit", "conversion_factor"))?;
    let value = attr(doc, conversion, "measure_with_unit", "value_component")?;
    let value = if let A::Typed { value, .. } = value {
        number(value)?
    } else {
        number(value)?
    };
    Some(
        value
            * unit_scale(
                doc,
                reference(attr(doc, conversion, "measure_with_unit", "unit_component"))?,
                depth + 1,
            )?,
    )
}

/// Validate the companion with the shipped schema.
#[cfg(feature = "json-import")]
pub fn validate(v: &Value) -> Result<()> {
    let schema: Value = serde_json::from_str(super::PRESENTATION_SCHEMA)?;
    super::schema::check(v, &schema, &schema, "")?;
    layout::validate(v)
}

/// Generate current notation placed on the part's geometry in model millimetres.
#[cfg(feature = "presentation")]
pub fn generated(semantic: &Semantic, part: &crate::brep::Part) -> Result<Presentation> {
    use crate::presentation;
    let mut out = Presentation {
        semantic_ids: id_set(semantic)?,
        ..Presentation::default()
    };
    let mut layout = presentation::placement::Layout::new(part.bounds());
    let mut add = |collection: &str, index: usize, callout: presentation::Callout| -> Result<()> {
        let key = semantic
            .ids
            .get(collection)
            .and_then(|ids| ids.get(index))
            .ok_or_else(|| Error(format!("missing id {collection}/{index}")))?;
        let mut g = Graphic::new(Source::Generated, callout.notation());
        let feature = match collection {
            "dimensions" => match &semantic.pmi.dimensions[index].kind {
                crate::pmi::DimensionKind::Size { feature, .. } => Some(*feature),
                crate::pmi::DimensionKind::Location { from, .. } => Some(*from),
            },
            "datums" => semantic.pmi.datums[index].feature(),
            "tolerances" => match semantic.pmi.tolerances[index].target {
                crate::pmi::ToleranceTarget::Feature(f) => Some(f),
                _ => None,
            },
            "surface_textures" => semantic.pmi.surface_textures[index].on,
            _ => None,
        };
        let site =
            feature.and_then(|f| presentation::placement::feature_site(part, &semantic.pmi, f));
        let notes = site.is_none();
        let placed = if let Some((point, normal, _)) = site {
            layout.on_face(point, normal, &callout)
        } else {
            layout.notes(&callout)
        };
        g.id = Some(format!("generated:{collection}:{key}"));
        layout::generated(
            &mut g,
            &callout,
            &placed,
            layout.text_height(),
            part.bounds().max_extent(),
            notes,
        );
        let plane = Plane::generated(&g, &placed);
        g.plane_id = Some(plane.id.clone());
        out.planes.push(plane);
        g.polylines = placed.lines;
        g.text = callout.cells;
        out.items.entry(key.clone()).or_default().push(g);
        Ok(())
    };
    for (i, d) in semantic.pmi.dimensions.iter().enumerate() {
        add("dimensions", i, presentation::dimension(d))?;
    }
    for (i, t) in semantic.pmi.tolerances.iter().enumerate() {
        add("tolerances", i, presentation::tolerance(t, &semantic.pmi))?;
    }
    for (i, d) in semantic.pmi.datums.iter().enumerate() {
        add("datums", i, presentation::datum(d.label().as_str()))?;
    }
    for (i, t) in semantic
        .pmi
        .surface_textures
        .iter()
        .enumerate()
        .filter(|(_, t)| t.on.is_some())
    {
        add(
            "surface_textures",
            i,
            presentation::notes::surface_texture(t),
        )?;
    }
    for (i, n) in semantic.pmi.notes.iter().enumerate() {
        add("notes", i, presentation::label(&n.text))?;
    }
    for row in presentation::notes::general_notes(&semantic.pmi) {
        use presentation::notes::Source;
        let (collection, index) = match row.source {
            Source::General(i) => ("general", i),
            Source::Texture(i) => ("surface_textures", i),
            Source::Attribute(i) => ("attributes", i),
            Source::Material => continue,
        };
        add(collection, index, presentation::notes::notes_block(&[row]))?;
    }
    // Material has no independent PartIdentifiers carrier; retain its callout unlinked.
    if let Some(material) = &semantic.pmi.material {
        let c = presentation::label(&format!("MATERIAL: {}", material.id));
        let mut g = Graphic::new(self::Source::Generated, c.notation());
        let placed = layout.notes(&c);
        g.id = Some("generated:material".into());
        layout::generated(
            &mut g,
            &c,
            &placed,
            layout.text_height(),
            part.bounds().max_extent(),
            true,
        );
        let plane = Plane::generated(&g, &placed);
        g.plane_id = Some(plane.id.clone());
        out.planes.push(plane);
        g.polylines = placed.lines;
        g.text = c.cells;
        out.unlinked.push(g);
    }
    #[cfg(feature = "json-import")]
    validate(&serde_json::to_value(&out)?)?;
    Ok(out)
}

fn id_set(semantic: &Semantic) -> Result<Vec<String>> {
    let mut ids: Vec<_> = semantic.ids.values().flatten().cloned().collect();
    let encoded = super::to_value(semantic)?;
    for table in ["length", "angle", "density", "other"] {
        ids.extend(
            encoded["units"][table]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|unit| unit["id"].as_str().map(str::to_string)),
        );
    }
    ids.sort();
    Ok(ids)
}
/// Refuse stale presentation links before use.
pub fn validate_links(presentation: &Presentation, semantic: &Semantic) -> Result<()> {
    if presentation.semantic_ids != id_set(semantic)? {
        return Err(Error(
            "presentation belongs to a different PMI id set".into(),
        ));
    }
    if presentation
        .items
        .keys()
        .any(|id| !presentation.semantic_ids.contains(id))
    {
        return Err(Error("presentation contains an unknown semantic id".into()));
    }
    Ok(())
}
