//! Read file-owned callouts without regenerating their shape or dropping unlinked graphics.
use super::{Error, Result, Semantic};
use crate::{
    express, geom,
    p21::{Attribute as A, Document, RawEntity},
    pmi::{PartId, PmiRead},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Graphic {
    pub source: Source,
    pub name: String,
    pub polylines: Vec<Vec<[f64; 3]>>,
    pub triangles: Vec<[[f64; 3]; 3]>,
    pub text: Vec<String>,
    pub step_ids: Vec<u64>,
    pub unsupported: Vec<String>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Generated,
    File,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Presentation {
    pub schema: String,
    pub length_unit: String,
    pub items: BTreeMap<String, Vec<Graphic>>,
    pub unlinked: Vec<Graphic>,
}
impl Default for Presentation {
    fn default() -> Self {
        Self {
            schema: "haecceity.presentation/1".into(),
            length_unit: "millimetre".into(),
            items: BTreeMap::new(),
            unlinked: vec![],
        }
    }
}
impl Graphic {
    fn new(source: Source, name: String) -> Self {
        Self {
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
fn is(doc: &Document, id: u64, t: &str) -> bool {
    names(doc, id)
        .iter()
        .any(|n| express::TARGET.is_a(n, t) || express::AP242_ED1.is_a(n, t))
}
fn attr<'a>(doc: &'a Document, id: u64, entity: &str, name: &str) -> Option<&'a A> {
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
fn reference(a: Option<&A>) -> Option<u64> {
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
    let origin = point(doc, reference(attr(doc, id, "placement", "location"))?)?;
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
    depth: usize,
) {
    if depth > 64 {
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
    let at = |p| geom::scale(transform.at(p), scale);
    let kids = children(doc, id);
    if !kids.is_empty() {
        for r in kids {
            draw(doc, r, transform, scale, g, seen, depth + 1);
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
        g.text.push(text(attr(doc, id, "text_literal", "literal")));
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
    let carriers: BTreeMap<_, _> = read
        .json_carriers
        .iter()
        .filter(|((p, _, _), _)| *p == part)
        .filter_map(|((_, c, i), entity)| {
            semantic
                .ids
                .get(c)
                .and_then(|ids| ids.get(*i))
                .map(|id| (*entity, id.clone()))
        })
        .collect();
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
            if let Some(key) = carriers.get(&definition) {
                links.entry(item).or_default().insert(key.clone());
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
    let mut out = Presentation::default();
    for root in roots {
        let mut graphic = Graphic::new(
            Source::File,
            text(attr(doc, root, "representation_item", "name")),
        );
        let scale = scale_for(doc, root);
        draw(
            doc,
            root,
            Transform::identity(),
            scale.unwrap_or(1.),
            &mut graphic,
            &mut BTreeSet::new(),
            0,
        );
        if scale.is_none() {
            graphic.polylines.clear();
            graphic.triangles.clear();
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
        if keys.is_empty() {
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
pub fn validate(v: &Value) -> Result<()> {
    let schema: Value = serde_json::from_str(super::PRESENTATION_SCHEMA)?;
    super::schema::check(v, &schema, &schema, "")
}

/// Generate current haecceity notation in model-space XY at the origin. Clients
/// can use `presentation::placement` to arrange these graphics in another frame.
#[cfg(feature = "presentation")]
pub fn generated(semantic: &Semantic) -> Result<Presentation> {
    use crate::presentation;
    let mut out = Presentation::default();
    let mut add = |collection: &str, index: usize, callout: presentation::Callout| -> Result<()> {
        let key = semantic
            .ids
            .get(collection)
            .and_then(|ids| ids.get(index))
            .ok_or_else(|| Error(format!("missing id {collection}/{index}")))?;
        let mut g = Graphic::new(Source::Generated, callout.notation());
        g.polylines = callout
            .polylines()
            .into_iter()
            .map(|line| line.into_iter().map(|[x, y]| [x, y, 0.]).collect())
            .collect();
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
        g.polylines = c
            .polylines()
            .into_iter()
            .map(|l| l.into_iter().map(|[x, y]| [x, y, 0.]).collect())
            .collect();
        g.text = c.cells;
        out.unlinked.push(g);
    }
    validate(&serde_json::to_value(&out)?)?;
    Ok(out)
}
