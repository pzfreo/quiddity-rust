//! Regenerate the maintainer's #137 spindle semantics; never reuse its saved ink.
#![cfg(all(feature = "presentation", feature = "json-import"))]
use haecceity::{
    geom::{self, Bounds, V3},
    json::{
        self,
        presentation::{self, Graphic},
    },
    p21::Document,
    pmi::{self, PartId},
    presentation as notation, step,
};
use std::{io::Read, path::Path};

fn load(path: &str) -> Document {
    let raw = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join(path)).unwrap();
    let mut bytes = vec![];
    flate2::read::GzDecoder::new(raw.as_slice())
        .read_to_end(&mut bytes)
        .unwrap();
    Document::parse(bytes).unwrap()
}
fn centre(lines: &[Vec<V3>]) -> V3 {
    let mut b = Bounds::empty();
    for &p in lines.iter().flatten() {
        b.add(p);
    }
    b.centre()
}
fn text_origin(g: &Graphic, collection: &str, linear: bool) -> V3 {
    // This fixture has stroked text, not TEXT_LITERAL. Read the actual strokes
    // via presentation::read, exactly as view-only pmiScene does. The #137
    // serialization has ten dimension/arrow lines before linear lettering,
    // four datum-frame edges before its letter, and a final leader otherwise.
    if linear {
        centre(&g.polylines[10..])
    } else if collection == "tolerances" {
        // Logical origin of the characteristic cell: opposite frame corners.
        geom::scale(geom::add(g.polylines[0][0], g.polylines[4][1]), 0.5)
    } else {
        let first = if collection == "datums" { 4 } else { 0 };
        centre(&g.polylines[first..g.polylines.len() - 1])
    }
}
fn near(a: V3, b: V3) {
    assert!(geom::dist(a, b) < 2e-6, "{a:?} != {b:?}");
}

#[test]
fn regenerated_spindle_keeps_its_saved_callouts_near_their_features() {
    let doc = load("tests/fixtures/presentation/spindle-q137.step.gz");
    let defs = step::read_part_definitions(doc.bytes()).unwrap();
    let read = pmi::read(&doc, &defs).unwrap();
    // As Specify does: the source of regeneration is the read semantic model.
    let semantic = json::identify(read.parts[0].clone(), Some(&read.identifiers.parts[0])).unwrap();
    let part = step::read_part(doc.bytes(), &defs[0]).unwrap();
    let saved = presentation::read(&doc, &read, PartId(0), &semantic).unwrap();
    let generated = presentation::generated(&semantic, &part).unwrap();
    presentation::validate(&serde_json::to_value(&generated).unwrap()).unwrap();
    let measured = notation::linear::measurements(&part, &defs[0], &semantic.pmi);
    // These pin the interpretation of the saved glyph strokes, not the new
    // generator's positions. All coordinates are model-space millimetres.
    let dimension_origins = [
        [40.42752, 0., -4.7912],
        [-1.0026666667, 0., 0.],
        [-1.7045333333, 0., -0.95],
        [18.8, 0., 7.8101333333],
        [16.8, 0., 7.1082666667],
        [13.6, 0., 6.4064],
        [2.5, 0., 5.7045333333],
        [0.75, 0., 5.0026666667],
        [38.6026666667, 0., 0.],
    ];
    let tolerance_origins = [[3.509344, 0., -5.5432], [36.324608, 0., -5.5432]];
    let datum_origins = [[36.1, 2.107131, -4.6408], [0., 2.107131, -4.6408]];
    for collection in ["dimensions", "tolerances", "datums"] {
        for (i, key) in semantic.ids[collection].iter().enumerate() {
            let file = saved.items[key]
                .iter()
                .find(|g| !g.polylines.is_empty())
                .unwrap();
            let actual = &generated.items[key][0];
            let linear = collection == "dimensions" && measured[i].is_some();
            let file_origin = text_origin(file, collection, linear);
            near(
                file_origin,
                match collection {
                    "dimensions" => dimension_origins[i],
                    "tolerances" => tolerance_origins[i],
                    _ => datum_origins[i],
                },
            );
            let placed = &actual.text_placements[0];
            near(placed.origin, text_origin(actual, collection, linear));
            assert_eq!(placed.stroked, Some(true));
            if linear {
                // Preserve the later shared outer-plane rule (Y=-4 instead of
                // the file's Y=0) and the larger readable font. Six mm allows
                // that 4-mm depth shift and at most sqrt(20) mm of in-view drift.
                let limit = 6.;
                let distance = geom::dist(placed.origin, file_origin);
                println!(
                    "{}: saved={file_origin:?}, regenerated={:?}, drift={distance:.6} mm <= {limit} mm",
                    actual.name, placed.origin
                );
                assert!(
                    distance <= limit,
                    "{}: drift {distance} exceeds {limit} mm",
                    actual.name
                );
            } else {
                let feature = match collection {
                    "dimensions" => match semantic.pmi.dimensions[i].kind {
                        pmi::DimensionKind::Size { feature, .. } => feature,
                        _ => unreachable!(),
                    },
                    "tolerances" => match semantic.pmi.tolerances[i].target {
                        pmi::ToleranceTarget::Feature(f) => f,
                        _ => unreachable!(),
                    },
                    _ => semantic.pmi.datums[i].feature().unwrap(),
                };
                let site = notation::placement::feature_site(&part, &semantic.pmi, feature)
                    .unwrap()
                    .0;
                let saved_distance = geom::dist(file_origin, site);
                // A long source face can put its old glyph far from the chosen
                // interior site. Also require an absolute two-capital-height
                // bound, so the saved distance cannot excuse a floating flag.
                let near_feature =
                    2. * notation::placement::text_height(part.bounds().max_extent());
                let limit = saved_distance.min(near_feature);
                let distance = geom::dist(placed.origin, site);
                println!(
                    "{}: saved={file_origin:?}, regenerated={:?}, feature distance={distance:.6} mm <= min(saved={saved_distance:.6}, 2h={near_feature:.6})={limit:.6} mm",
                    actual.name, placed.origin
                );
                assert!(
                    distance <= limit + 2e-6,
                    "{}: feature distance {distance} exceeds saved {limit}",
                    actual.name
                );
            }
        }
    }
    assert_eq!(generated.views.len(), generated.planes.len());
}

#[test]
fn session_measurements_preserve_step_edge_identity_on_all_four_parts() {
    for (path, producer) in [
        ("tests/fixtures/presentation/spindle-q137.step.gz", false),
        (
            "../../tests/fixtures/ap242/nist/nist_ctc_01_asme1_ap242-e1.stp.gz",
            false,
        ),
        (
            "../../tests/fixtures/ap242/presentation-placement/plate.step.gz",
            false,
        ),
        ("tests/fixtures/xcaf/pmi_plate.geom.step.gz", true),
    ] {
        let doc = load(path);
        let defs = step::read_part_definitions(doc.bytes()).unwrap();
        let read = pmi::read(&doc, &defs).unwrap();
        let p = if producer {
            json::from_str(
                &std::fs::read_to_string(
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/fixtures/xcaf/pmi_plate.pmi.json"),
                )
                .unwrap(),
                &json::Anchors::read(&doc, &defs[0]),
            )
            .unwrap()
            .pmi
        } else {
            read.parts[0].clone()
        };
        let part = step::read_part(doc.bytes(), &defs[0]).unwrap();
        let from_definition = notation::linear::measurements(&part, &defs[0], &p);
        let in_session = notation::linear::measurements_for_part(&part, &p);
        for (i, d) in p.dimensions.iter().enumerate() {
            if d.nominal.is_none() {
                assert!(
                    in_session[i].is_none(),
                    "{path}: unstated nominal must not make a zero-span ladder"
                );
            }
        }
        for (a, b) in from_definition.iter().zip(&in_session) {
            assert_eq!(
                a.as_ref().map(|m| m.feet),
                b.as_ref().map(|m| m.feet),
                "{path}: semantic edge identity"
            );
        }
        let generated = presentation::generated(&json::identify(p, None).unwrap(), &part).unwrap();
        presentation::validate(&serde_json::to_value(&generated).unwrap()).unwrap();
    }
}
