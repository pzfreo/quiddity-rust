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
    // The STEP writer must retain the compact ladder too. This old file's
    // private description links block Replace under the current ownership rules;
    // preserve them and identify only the newly added callouts from Add.
    // Inspect actual serialized lettering, not the generated metadata.
    let (edit, _) = pmi::write(
        &doc,
        &defs,
        &[(PartId(0), semantic.pmi.clone())],
        pmi::write::Mode::Add,
        pmi::write::PresentationPolicy::RemovePresentation,
    )
    .unwrap();
    let rewritten = Document::parse(doc.apply(&edit).unwrap().bytes).unwrap();
    let rewritten_defs = step::read_part_definitions(rewritten.bytes()).unwrap();
    let rewritten_read = pmi::read(&rewritten, &rewritten_defs).unwrap();
    let rewritten_semantic = json::identify(
        rewritten_read.parts[0].clone(),
        Some(&rewritten_read.identifiers.parts[0]),
    )
    .unwrap();
    let written =
        presentation::read(&rewritten, &rewritten_read, PartId(0), &rewritten_semantic).unwrap();
    for (i, m) in measured.iter().enumerate() {
        if m.is_none() {
            continue;
        }
        let file = &saved.items[&semantic.ids["dimensions"][i]][0];
        let name = &generated.items[&semantic.ids["dimensions"][i]][0].name;
        let actual = written
            .items
            .values()
            .flatten()
            .chain(&written.unlinked)
            .find(|g| {
                &g.name == name
                    && g.id
                        .as_deref()
                        .and_then(|id| id.strip_prefix("step:"))
                        .and_then(|id| id.parse::<u64>().ok())
                        .is_some_and(|id| id > doc.max_id())
            })
            .unwrap_or_else(|| panic!("missing newly written {name}"));
        let distance = geom::dist(
            text_origin(actual, "dimensions", true),
            text_origin(file, "dimensions", true),
        );
        assert!(
            distance <= 6.,
            "written {}: drift {distance} exceeds 6 mm",
            actual.name
        );
    }
    assert_eq!(generated.views.len(), generated.planes.len());
}

#[test]
fn session_measurements_and_callout_clearance_on_all_four_parts() {
    let mut overlaps = vec![];
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
        let semantic = json::identify(p, None).unwrap();
        let generated = presentation::generated(&semantic, &part).unwrap();
        overlaps.extend(callout_gaps(path, &generated, &semantic, &in_session));
        presentation::validate(&serde_json::to_value(&generated).unwrap()).unwrap();
    }
    assert!(overlaps.is_empty(), "{}", overlaps.join("\n"));
}

// Actual stroke boxes, without leaders or measured witness/arrow lines. Compare
// parallel planes in projection even at different depths. Also compare related
// callouts: choosing a different plane must not hide a same-feature collision.
fn callout_gaps(
    path: &str,
    generated: &presentation::Presentation,
    semantic: &json::Semantic,
    measured: &[Option<notation::linear::Measurement>],
) -> Vec<String> {
    struct Ink<'a> {
        graphic: &'a Graphic,
        normal: V3,
        along: V3,
        points: Vec<V3>,
        feature: Option<pmi::FeatureId>,
        size: bool,
        control: bool,
    }
    let mut boxes: Vec<Ink<'_>> = vec![];
    for (collection, ids) in &semantic.ids {
        for (i, id) in ids.iter().enumerate() {
            for g in generated.items.get(id).into_iter().flatten() {
                if boxes.iter().any(|old| old.graphic.id == g.id) {
                    continue;
                }
                let plane = generated
                    .planes
                    .iter()
                    .find(|p| Some(&p.id) == g.plane_id.as_ref())
                    .unwrap();
                let start = if collection == "dimensions" && measured[i].is_some() {
                    10
                } else {
                    0
                };
                let end = if start == 0
                    && matches!(collection.as_str(), "dimensions" | "tolerances" | "datums")
                {
                    g.polylines.len() - 1
                } else {
                    g.polylines.len()
                };
                let dimension_feature = |i: usize| match semantic.pmi.dimensions[i].kind {
                    pmi::DimensionKind::Size { feature, .. } => Some(feature),
                    _ => None,
                };
                let feature = match collection.as_str() {
                    "dimensions" if start == 0 => dimension_feature(i),
                    "tolerances" => match semantic.pmi.tolerances[i].target {
                        pmi::ToleranceTarget::Feature(f)
                        | pmi::ToleranceTarget::Relation { relating: f, .. } => Some(f),
                        pmi::ToleranceTarget::Dimension(d) => dimension_feature(d.0),
                        _ => None,
                    },
                    _ => None,
                };
                let points = g.polylines[start..end]
                    .iter()
                    .flatten()
                    .copied()
                    .collect::<Vec<_>>();
                if points.is_empty() {
                    assert!(g.name.is_empty(), "{path}: missing ink for {}", g.name);
                    continue; // CTC-01's unstated nominal values draw no lettering.
                }
                if path.contains("spindle") && (g.name == "⌀3.3 H11" || g.name == "⌖ ⌀0.3Ⓜ A")
                {
                    let mut b = Bounds::empty();
                    for &p in &points {
                        b.add(p);
                    }
                    println!("{path}: {} model ink {b:?}", g.name);
                }
                boxes.push(Ink {
                    graphic: g,
                    normal: plane.normal,
                    along: plane.x_axis,
                    points,
                    feature,
                    size: collection == "dimensions" && start == 0,
                    control: collection == "tolerances",
                });
            }
        }
    }
    for g in &generated.unlinked {
        let plane = generated
            .planes
            .iter()
            .find(|p| Some(&p.id) == g.plane_id.as_ref())
            .unwrap();
        boxes.push(Ink {
            graphic: g,
            normal: plane.normal,
            along: plane.x_axis,
            points: g.polylines.iter().flatten().copied().collect(),
            feature: None,
            size: false,
            control: false,
        });
    }
    let mut minimum = f64::INFINITY;
    let mut problems = vec![];
    let mut pairs = 0;
    for (i, a) in boxes.iter().enumerate() {
        for b in &boxes[i + 1..] {
            let related = a.feature.is_some() && a.feature == b.feature;
            let parallel = geom::dot(a.normal, b.normal).abs() > 1. - 1e-8;
            if !parallel && !related {
                continue;
            }
            if related && !parallel {
                problems.push(format!(
                    "{path}: related callouts must share a plane: {} / {}",
                    a.graphic.name, b.graphic.name
                ));
            }
            pairs += 1;
            let up = geom::cross(a.normal, a.along);
            let bounds = |points: &[V3]| {
                let mut b = Bounds::empty();
                for &p in points {
                    b.add([geom::dot(p, a.along), geom::dot(p, up), 0.]);
                }
                b
            };
            let x = bounds(&a.points);
            let y = bounds(&b.points);
            let gap = (0..2)
                .map(|k| (x.min[k] - y.max[k]).max(y.min[k] - x.max[k]))
                .fold(f64::NEG_INFINITY, f64::max);
            minimum = minimum.min(gap);
            if related
                && ((a.size && b.control && x.min[1] < y.max[1] - 2e-6)
                    || (b.size && a.control && y.min[1] < x.max[1] - 2e-6))
            {
                problems.push(format!(
                    "{path}: size must stack above its frame: {} / {}",
                    a.graphic.name, b.graphic.name
                ));
            }
            if gap < -2e-6 {
                problems.push(format!(
                    "{path}: {} {x:?} overlaps {} {y:?}: gap={gap:.6}",
                    a.graphic.name, b.graphic.name
                ));
            }
            if related && a.graphic.name == "⌀3.3 H11" && b.graphic.name == "⌖ ⌀0.3Ⓜ A" {
                println!("{path}: bore stack gap={gap:.6} mm");
            }
        }
    }
    assert!(pairs > 0, "{path}: no callout pairs checked");
    println!("{path}: {pairs} callout pairs, minimum gap={minimum:.6} mm");
    problems
}
