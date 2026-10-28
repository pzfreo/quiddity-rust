//! Geometry-only Fusion import (optional `f3d` feature).
//!
//! cadmpeg decodes ASM lengths to millimetres. Its STEP exporter preserves body transforms
//! and product occurrences; the existing STEP reader then flattens those placements.

use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::path::Path;

use cadmpeg_codec_f3d::F3dCodec;
use cadmpeg_codec_step::StepCodec;
use cadmpeg_core::decode::{DecodeArena, DecodeContext, DecodePolicy};
use cadmpeg_ir::codec::write::{EncodeInput, Encoder, target::TargetRequest};
use cadmpeg_ir::ids::{OccurrenceId, ProductDefinitionId};
use cadmpeg_ir::products::{
    Occurrence, OccurrenceParent, ProductDefinition, ProductDefinitionKind, PrototypeReference,
};
use cadmpeg_ir::report::{Severity, loss::LossNote};
use cadmpeg_ir::schema::EntitySchema;
use cadmpeg_ir::topology::BodyKind;
use cadmpeg_ir::transform::Transform;
use cadmpeg_ir::{CadIr, Codec, DecodeOptions};

use crate::Part;

type Error = Box<dyn std::error::Error>;

/// Read a Fusion archive, returning the same geometry type as [`crate::step::read_step_file`].
pub fn read_f3d_file(path: &Path) -> Result<Part, Error> {
    read_f3d(&std::fs::read(path)?)
}

/// Read a Fusion archive's bytes. Only solid bodies not explicitly hidden are imported.
pub fn read_f3d(bytes: &[u8]) -> Result<Part, Error> {
    Ok(crate::step::read_step(&to_step(bytes)?)?)
}

/// Convert visible Fusion solids to in-memory STEP for consumers of STEP part definitions.
///
/// This carries geometry and placements only, with no PMI or design history. Blocking decode
/// or export losses are refused. Non-blocking losses are accepted, as in cadmpeg's converter.
pub fn to_step(bytes: &[u8]) -> Result<Vec<u8>, Error> {
    let decoded = F3dCodec.decode(&mut Cursor::new(bytes), &DecodeOptions::default())?;
    refuse_blocking(&decoded.report().losses, "decode")?;
    let (ir, _, _) = decoded.into_parts();
    export_solids(ir)
}

fn refuse_blocking(losses: &[LossNote], stage: &str) -> Result<(), Error> {
    let blocking: Vec<_> = losses
        .iter()
        .filter(|loss| loss.severity == Severity::Blocking)
        .map(|loss| format!("{}: {}", loss.code, loss.message))
        .collect();
    if !blocking.is_empty() {
        return Err(std::io::Error::other(format!(
            "F3D {stage}: {} blocking losses: {}",
            blocking.len(),
            blocking.join("; ")
        ))
        .into());
    }
    Ok(())
}

fn export_solids(mut ir: CadIr) -> Result<Vec<u8>, Error> {
    select_solids(&mut ir)?;
    body_placements_as_occurrences(&mut ir)?;
    let plan = StepCodec::default().plan(
        EncodeInput {
            ir: &ir,
            fidelity: None,
        },
        TargetRequest::Inherit,
    )?;
    refuse_blocking(&plan.report().losses, "STEP export")?;
    let mut bytes = Vec::new();
    plan.write_to(&mut bytes)?;
    Ok(bytes)
}

/// cadmpeg writes body placements as MAPPED_ITEMs with AXIS2_PLACEMENT_3D targets.
/// Our STEP scene does not resolve those. Express exactly the same rigid motion as a child
/// occurrence instead, which both exporters/readers support, without changing STEP import.
fn body_placements_as_occurrences(ir: &mut CadIr) -> Result<(), Error> {
    if !ir
        .model
        .bodies
        .iter()
        .any(|b| b.transform.is_some_and(|t| t != Transform::identity()))
    {
        return Ok(());
    }
    fn product(
        index: usize,
        name: Option<String>,
        bodies: Vec<cadmpeg_ir::ids::BodyId>,
    ) -> Result<ProductDefinition, Error> {
        Ok(ProductDefinition {
            id: ProductDefinitionId::mint(format!("haecceity:f3d:product#{index}"))?,
            kind: ProductDefinitionKind::Part,
            source_name: None,
            label: name,
            description: None,
            part_number: None,
            bom_properties: Default::default(),
            bodies,
            sketches: Vec::new(),
            native_ref: None,
        })
    }
    fn occurrence(
        index: usize,
        definition: ProductDefinitionId,
        parent: OccurrenceParent,
        transform: Transform,
    ) -> Result<Occurrence, Error> {
        Ok(Occurrence {
            id: OccurrenceId::mint(format!("haecceity:f3d:occurrence#{index}"))?,
            prototype: PrototypeReference::Local { definition },
            parent,
            ordinal: u32::try_from(index)?,
            transform,
            linked_prototype: None,
            scale: [cadmpeg_ir::scalar::FiniteReal::ONE; 3],
            name: None,
            visible: None,
            link: None,
            native_ref: None,
        })
    }
    let mut products = std::mem::take(&mut ir.model.product_definitions);
    let mut occurrences = std::mem::take(&mut ir.model.occurrences);
    if products.is_empty() {
        let root = product(
            0,
            None,
            ir.model.bodies.iter().map(|b| b.id.clone()).collect(),
        )?;
        occurrences.push(occurrence(
            0,
            root.id.clone(),
            OccurrenceParent::Root {},
            Transform::identity(),
        )?);
        products.push(root);
    }
    for body in &mut ir.model.bodies {
        let Some(transform) = body.transform.take() else {
            continue;
        };
        if transform == Transform::identity() {
            continue;
        }
        if !transform.is_proper_rigid() {
            return Err(std::io::Error::other(format!(
                "F3D body {} has a non-rigid placement",
                body.id
            ))
            .into());
        }
        let mut parents = Vec::new();
        for owner in &mut products {
            if !owner.bodies.contains(&body.id) {
                continue;
            }
            owner.bodies.retain(|id| *id != body.id);
            let mut found: Vec<_> = occurrences.iter().filter(|o| matches!(
                &o.prototype, PrototypeReference::Local { definition } if *definition == owner.id
            )).map(|o| o.id.clone()).collect();
            if found.is_empty() {
                let root = occurrence(
                    occurrences.len(),
                    owner.id.clone(),
                    OccurrenceParent::Root {},
                    Transform::identity(),
                )?;
                found.push(root.id.clone());
                occurrences.push(root);
            }
            parents.extend(
                found
                    .into_iter()
                    .map(|occurrence| OccurrenceParent::Occurrence { occurrence }),
            );
        }
        if parents.is_empty() {
            parents.push(OccurrenceParent::Root {});
        }
        let child = product(products.len(), body.name.clone(), vec![body.id.clone()])?;
        for parent in parents {
            occurrences.push(occurrence(
                occurrences.len(),
                child.id.clone(),
                parent,
                transform,
            )?);
        }
        products.push(child);
    }
    ir.model.product_definitions = products;
    ir.model.occurrences = occurrences;
    Ok(())
}

/// Retain the reference closure of selected bodies. Keeping orphan regions would export
/// sketches as wireframes; keeping orphan carriers would export independent curves/surfaces.
/// Use cadmpeg's typed reference visitor so pcurves and procedural supports stay attached.
fn select_solids(ir: &mut CadIr) -> Result<(), Error> {
    let hidden = ir
        .model
        .bodies
        .iter()
        .filter(|b| b.kind == BodyKind::Solid && b.visible == Some(false))
        .count();
    let mut pending: Vec<_> = ir
        .model
        .bodies
        .iter()
        .filter(|b| b.kind == BodyKind::Solid && b.visible != Some(false))
        .map(|b| b.id.as_str().to_owned())
        .collect();
    if pending.is_empty() {
        return Err(std::io::Error::other(if hidden > 0 {
            format!("F3D has no visible solid bodies ({hidden} hidden solids); refusing to guess")
        } else {
            "F3D has no solid bodies (empty or wire/sheet-only design)".into()
        })
        .into());
    }
    let arena = DecodeArena::new();
    let policy = DecodePolicy::desktop();
    let (ctx, _) = DecodeContext::from_root_bytes(b"", &arena, &policy)?;
    let mut references = HashMap::new();
    // Product definitions and occurrences are kept separately below: traversing their body
    // references would reintroduce hidden solids. Everything else starts empty.
    macro_rules! geometry {
        ($($field:ident),+ $(,)?) => {{
            $(for entity in &ir.model.$field {
                let mut refs = Vec::new();
                entity.visit_references(&ctx, &mut |id| { refs.push(id.to_owned()); Ok(()) })?;
                references.insert(entity.identity().to_owned(), refs);
            })+
            let mut keep = HashSet::new();
            while let Some(id) = pending.pop() {
                if keep.insert(id.clone()) {
                    if let Some(refs) = references.get(&id) { pending.extend(refs.iter().cloned()); }
                }
            }
            let mut model = cadmpeg_ir::document::Model::default();
            $(model.$field = std::mem::take(&mut ir.model.$field).into_iter()
                .filter(|entity| keep.contains(entity.identity())).collect();)+
            model.product_definitions = std::mem::take(&mut ir.model.product_definitions);
            for product in &mut model.product_definitions {
                product.bodies.retain(|id| keep.contains(id.as_str()));
                product.sketches.clear();
            }
            model.occurrences = std::mem::take(&mut ir.model.occurrences);
            ir.model = model;
        }};
    }
    geometry!(
        bodies,
        regions,
        shells,
        faces,
        loops,
        coedges,
        edges,
        vertices,
        points,
        surfaces,
        curves,
        pcurves,
        procedural_surfaces,
        procedural_curves
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadmpeg_ir::report::loss::LossTaxonomy;
    use serde_json::json;

    const CUBE: &[u8] = include_bytes!("../tests/fixtures/generated_unit_cube.f3d");

    fn cube_ir() -> CadIr {
        F3dCodec
            .decode(&mut Cursor::new(CUBE), &DecodeOptions::default())
            .unwrap()
            .into_parts()
            .0
    }

    #[test]
    fn cube_file_and_bytes_are_six_faces_and_ten_millimetres() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/generated_unit_cube.f3d");
        for part in [read_f3d(CUBE).unwrap(), read_f3d_file(&path).unwrap()] {
            assert_eq!(part.faces.len(), 6);
            assert_eq!(part.solids.len(), 1);
            let bounds = part.bounds();
            for k in 0..3 {
                assert!(bounds.min[k].abs() < 1e-8, "{bounds:?}");
                assert!((bounds.max[k] - 10.0).abs() < 1e-8, "{bounds:?}");
            }
            assert!((part.solid_mass(0).unwrap().0 - 1000.0).abs() < 1e-6);
        }
    }

    #[test]
    fn all_hidden_archive_refuses_with_count() {
        let decoded = F3dCodec
            .decode(&mut Cursor::new(CUBE), &DecodeOptions::default())
            .unwrap();
        let (mut ir, _, _) = decoded.into_parts();
        ir.native = Default::default();
        ir.source = None;
        ir.model.bodies[0].visible = Some(false);
        let mut bytes = Vec::new();
        F3dCodec
            .plan(
                EncodeInput {
                    ir: &ir,
                    fidelity: None,
                },
                TargetRequest::Inherit,
            )
            .unwrap()
            .write_to(&mut bytes)
            .unwrap();
        let err = read_f3d(&bytes).unwrap_err().to_string();
        assert!(err.contains("1 hidden solids"), "{err}");
    }

    #[test]
    fn no_solids_and_hidden_bodies_are_never_substituted() {
        let mut ir = cube_ir();
        ir.model.bodies[0].kind = BodyKind::Wire;
        assert!(
            select_solids(&mut ir)
                .unwrap_err()
                .to_string()
                .contains("no solid bodies")
        );
        assert!(
            select_solids(&mut CadIr::empty())
                .unwrap_err()
                .to_string()
                .contains("no solid bodies")
        );
    }

    #[test]
    fn hidden_topology_sketch_wires_and_free_geometry_are_not_export_roots() {
        let mut ir = cube_ir();
        let mut hidden =
            CadIr::from_json(&ir.to_canonical_json().unwrap().replace("f3d:", "hidden:")).unwrap();
        hidden.model.bodies[0].visible = Some(false);
        macro_rules! append_geometry {
            ($($field:ident),+) => { $(ir.model.$field.append(&mut hidden.model.$field);)+ };
        }
        append_geometry!(
            bodies,
            regions,
            shells,
            faces,
            loops,
            coedges,
            edges,
            vertices,
            points,
            surfaces,
            curves,
            pcurves,
            procedural_surfaces,
            procedural_curves
        );
        let mut wire = ir.model.bodies[0].clone();
        wire.id = cadmpeg_ir::ids::BodyId::mint("test:model:body#wire").unwrap();
        wire.kind = BodyKind::Wire;
        let region = cadmpeg_ir::topology::Region {
            id: cadmpeg_ir::ids::RegionId::mint("test:model:region#wire").unwrap(),
            body: wire.id.clone(),
            shells: vec![cadmpeg_ir::ids::ShellId::mint("test:model:shell#wire").unwrap()],
        };
        wire.regions = vec![region.id.clone()];
        ir.model
            .shells
            .push(cadmpeg_ir::topology::Shell::with_wire_edge(
                region.shells[0].clone(),
                region.id.clone(),
                ir.model.edges[0].id.clone(),
            ));
        ir.model.regions.push(region);
        ir.model.bodies.push(wire);
        let mut free = ir.model.curves[0].clone();
        free.id = cadmpeg_ir::ids::CurveId::mint("test:model:curve#free").unwrap();
        ir.model.curves.push(free);
        let step = export_solids(ir).unwrap();
        assert!(!String::from_utf8_lossy(&step).contains("EDGE_BASED_WIREFRAME_MODEL"));
        let part = crate::read_step(&step).unwrap();
        assert_eq!(part.faces.len(), 6);
        assert_eq!(part.solids.len(), 1);
    }

    #[test]
    fn encoded_wire_only_archive_refuses() {
        let mut ir = cube_ir();
        ir.native = Default::default();
        ir.source = None;
        ir.model.bodies[0].kind = BodyKind::Wire;
        let edge = ir.model.edges[0].clone();
        ir.model.edges.retain(|e| e.id == edge.id);
        ir.model
            .vertices
            .retain(|v| v.id == edge.start || v.id == edge.end);
        ir.model
            .points
            .retain(|p| ir.model.vertices.iter().any(|v| v.point == p.id));
        ir.model
            .curves
            .retain(|c| edge.carrier.curve() == Some(&c.id));
        ir.model.shells = vec![cadmpeg_ir::topology::Shell::with_wire_edge(
            ir.model.shells[0].id.clone(),
            ir.model.regions[0].id.clone(),
            edge.id,
        )];
        ir.model.faces.clear();
        ir.model.loops.clear();
        ir.model.coedges.clear();
        ir.model.surfaces.clear();
        let mut bytes = Vec::new();
        F3dCodec
            .plan(
                EncodeInput {
                    ir: &ir,
                    fidelity: None,
                },
                TargetRequest::Inherit,
            )
            .unwrap()
            .write_to(&mut bytes)
            .unwrap();
        let err = read_f3d(&bytes).unwrap_err().to_string();
        assert!(err.contains("no solid bodies"), "{err}");
    }

    #[test]
    fn blocking_losses_refuse_and_nonblocking_losses_are_allowed() {
        let loss = LossNote::new(
            LossTaxonomy::GeometryNotTransferred,
            "test missing geometry",
        );
        assert!(
            refuse_blocking(&[loss.clone().with_severity(Severity::Warning)], "decode").is_ok()
        );
        let err = refuse_blocking(&[loss.with_severity(Severity::Blocking)], "decode")
            .unwrap_err()
            .to_string();
        assert!(err.contains("test missing geometry"), "{err}");
    }

    #[test]
    fn body_and_occurrence_placements_survive_selection() {
        let mut ir = cube_ir();
        ir.model.bodies[0].transform = Some(
            serde_json::from_value(json!([[1, 0, 0, 2], [0, 1, 0, 3], [0, 0, 1, 4]])).unwrap(),
        );
        let body = ir.model.bodies[0].id.as_str();
        ir.model.product_definitions = vec![
            serde_json::from_value(json!({
                "id": "test:model:product#component", "kind": "part", "bodies": [body]
            }))
            .unwrap(),
        ];
        ir.model.occurrences = vec![serde_json::from_value(json!({
            "id": "test:model:occurrence#instance", "prototype": {"scope": "local", "definition": "test:model:product#component"},
            "parent": {"kind": "root"}, "ordinal": 0, "scale": [1, 1, 1],
            "transform": [[1,0,0,20], [0,1,0,30], [0,0,1,40]]
        })).unwrap()];
        let step = export_solids(ir).unwrap();
        let part = crate::read_step(&step).unwrap();
        assert_eq!(part.faces.len(), 6);
        let bounds = part.bounds();
        for (k, expected) in [22.0, 33.0, 44.0].iter().enumerate() {
            assert!((bounds.min[k] - expected).abs() < 1e-8, "{bounds:?}");
            assert!((bounds.max[k] - expected - 10.0).abs() < 1e-8, "{bounds:?}");
        }
    }
}
