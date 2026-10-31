//! Bounds-relative regression evidence for generated and serialized notes.
#![cfg(all(feature = "presentation", feature = "json-import"))]
use haecceity::{
    geom::Bounds,
    json,
    p21::Document,
    pmi::{self, PartId},
    presentation::{notes, placement::Layout},
    step,
};
use std::{io::Read, path::Path};

fn load(path: &str) -> Document {
    let bytes = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join(path)).unwrap();
    let mut raw = Vec::new();
    flate2::read::GzDecoder::new(bytes.as_slice())
        .read_to_end(&mut raw)
        .unwrap();
    Document::parse(raw).unwrap()
}
fn ink(lines: &[Vec<[f64; 3]>]) -> Bounds {
    let mut b = Bounds::empty();
    for &p in lines.iter().flatten() {
        b.add(p);
    }
    b
}
#[test]
fn notes_stay_beside_part_in_json_and_written_step() {
    for (name, path) in [
        ("spindle", "tests/fixtures/presentation/spindle.step.gz"),
        (
            "CTC-01",
            "../../tests/fixtures/ap242/nist/nist_ctc_01_asme1_ap242-e1.stp.gz",
        ),
        (
            "plate",
            "../../tests/fixtures/ap242/presentation-placement/plate.step.gz",
        ),
        ("cadgen plate", "tests/fixtures/xcaf/pmi_plate.geom.step.gz"),
    ] {
        let doc = load(path);
        let defs = step::read_part_definitions(doc.bytes()).unwrap();
        let read = pmi::read(&doc, &defs).unwrap();
        let mut semantic = if name == "cadgen plate" {
            json::from_str(
                &std::fs::read_to_string(
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/fixtures/xcaf/pmi_plate.pmi.json"),
                )
                .unwrap(),
                &json::Anchors::read(&doc, &defs[0]),
            )
            .unwrap()
        } else {
            json::identify(read.parts[0].clone(), Some(&read.identifiers.parts[0])).unwrap()
        };
        if notes::general_notes(&semantic.pmi).is_empty() {
            // These files have no part settings: apply the spindle's settings
            // to exercise the same block on all four geometries.
            let source = load("tests/fixtures/presentation/spindle.step.gz");
            let source_defs = step::read_part_definitions(source.bytes()).unwrap();
            let source_read = pmi::read(&source, &source_defs).unwrap();
            semantic.pmi.general = source_read.parts[0].general.clone();
            semantic.pmi.surface_textures.extend(
                source_read.parts[0]
                    .surface_textures
                    .iter()
                    .filter(|t| t.on.is_none())
                    .cloned(),
            );
            semantic.pmi.attributes.extend(
                source_read.parts[0]
                    .attributes
                    .iter()
                    .filter(|a| a.on.is_none())
                    .cloned(),
            );
            semantic = json::identify(semantic.pmi, None).unwrap();
        }
        let part = step::read_part(doc.bytes(), &defs[0]).unwrap();
        let bounds = part.bounds();
        let generated = json::presentation::generated(&semantic, &part).unwrap();
        println!("{name}: bounds {:?}..{:?}", bounds.min, bounds.max);
        let mut note_boxes = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for (key, graphics) in &generated.items {
            for g in graphics {
                if !seen.insert(g.id.clone()) {
                    continue;
                }
                if g.name == "General notes"
                    || semantic
                        .ids
                        .get("notes")
                        .is_some_and(|ids| ids.contains(key))
                {
                    let b = ink(&g.polylines);
                    let gap = 2. * Layout::new(bounds).text_height();
                    assert!((b.min[0] - bounds.max[0] - gap).abs() < 1e-6);
                    assert!(b.max[2] <= bounds.min[2] - gap + 1e-6);
                    assert!((b.min[1] - bounds.centre()[1]).abs() < 1e-6);
                    for other in &note_boxes {
                        let other: &Bounds = other;
                        assert!(
                            b.min[2] >= other.max[2] + gap - 1e-6
                                || other.min[2] >= b.max[2] + gap - 1e-6
                        );
                    }
                    note_boxes.push(b);
                    println!(
                        "  {key}: origin {:?}, ink {:?}..{:?}",
                        g.text_placements[0].origin,
                        ink(&g.polylines).min,
                        ink(&g.polylines).max
                    );
                }
            }
        }
        assert_eq!(unique_notes(&generated).len(), 1);
        // Both paths combine settings; check against the independent layout rule.
        let rows = notes::general_notes(&semantic.pmi);
        if !rows.is_empty() {
            let c = notes::notes_block(&rows);
            let placed = Layout::new(bounds).notes(&c);
            println!(
                "  combined block: {:?}, ink {:?}..{:?}",
                placed.origin,
                ink(&placed.lines).min,
                ink(&placed.lines).max
            );
        }
        assert!(!generated.items.is_empty() || name == "cadgen plate");
        let mut writable = semantic.pmi.clone();
        // CTC-01 omits four nominal values; the writer correctly refuses those.
        writable.dimensions.retain(|d| d.nominal.is_some());
        let (edit, _) = pmi::write(
            &doc,
            &defs,
            &[(PartId(0), writable)],
            pmi::write::Mode::Replace,
            pmi::write::PresentationPolicy::RemovePresentation,
        )
        .unwrap();
        let written = Document::parse(doc.apply(&edit).unwrap().bytes).unwrap();
        let written_defs = step::read_part_definitions(written.bytes()).unwrap();
        let written_read = pmi::read(&written, &written_defs).unwrap();
        let written_semantic = json::identify(
            written_read.parts[0].clone(),
            Some(&written_read.identifiers.parts[0]),
        )
        .unwrap();
        let drawn = json::presentation::read(&written, &written_read, PartId(0), &written_semantic)
            .unwrap();
        let serialized_notes = unique_notes(&drawn);
        assert_eq!(serialized_notes.len(), 1);
        // Compare actual serialized STEP ink with the independent layout call.
        let expected = Layout::new(bounds).notes(&notes::notes_block(&rows));
        let expected = ink(&expected.lines);
        for g in serialized_notes {
            let actual = ink(&g.polylines);
            for k in 0..3 {
                assert!(
                    (actual.min[k] - expected.min[k]).abs() < 1e-6,
                    "{name}: min {actual:?} vs {expected:?}"
                );
                assert!(
                    (actual.max[k] - expected.max[k]).abs() < 1e-6,
                    "{name}: max {actual:?} vs {expected:?}"
                );
            }
            println!(
                "  STEP {}: ink {:?}..{:?}",
                g.name,
                ink(&g.polylines).min,
                ink(&g.polylines).max
            );
        }
    }
}

fn unique_notes(drawn: &json::presentation::Presentation) -> Vec<&json::presentation::Graphic> {
    let mut seen = std::collections::BTreeSet::new();
    drawn
        .items
        .values()
        .flatten()
        .chain(&drawn.unlinked)
        .filter(|g| g.name == "General notes" && seen.insert(g.id.clone()))
        .collect()
}

fn refs(doc: &Document, id: u64) -> Vec<u64> {
    let mut out = Vec::new();
    haecceity::p21::visit_attributes(doc.get(id).unwrap(), &mut |a| {
        if let haecceity::p21::Attribute::EntityRef(id) = a {
            out.push(*id);
        }
    });
    out
}
fn is(doc: &Document, id: u64, kind: &str) -> bool {
    matches!(doc.get(id), Some(haecceity::p21::RawEntity::Simple { name, .. }) if name.eq_ignore_ascii_case(kind))
}
fn strokes(lines: &[Vec<[f64; 3]>]) -> Vec<Vec<[i64; 3]>> {
    let mut lines: Vec<_> = lines
        .iter()
        .map(|line| {
            line.iter()
                .map(|p| p.map(|v| (v * 1e8).round() as i64))
                .collect::<Vec<_>>()
        })
        .collect();
    lines.sort();
    lines
}

#[test]
fn spindle_numbered_notes_share_one_block_and_keep_each_requirement_link() {
    let doc = load("tests/fixtures/presentation/spindle-q137.step.gz");
    let defs = step::read_part_definitions(doc.bytes()).unwrap();
    let read = pmi::read(&doc, &defs).unwrap();
    let source = &read.parts[0];
    let settings = pmi::PartPmi {
        general: source.general.clone(),
        surface_textures: source
            .surface_textures
            .iter()
            .filter(|t| t.on.is_none())
            .cloned()
            .collect(),
        attributes: source
            .attributes
            .iter()
            // Submit the three defaults; the file's additional authored note
            // stays retained, and is covered by the byte-preservation check.
            .filter(|a| {
                a.on.is_none()
                    && a.name == "semantic text"
                    && a.items
                        .iter()
                        .any(|(key, _)| matches!(key.as_str(), "edge condition" | "edges"))
            })
            .cloned()
            .collect(),
        ..pmi::PartPmi::default()
    };
    assert_eq!(
        notes::general_notes(&settings)
            .iter()
            .map(|r| r.text.as_str())
            .collect::<Vec<_>>(),
        [
            "GENERAL TOLERANCES ISO 2768-m",
            "SURFACE FINISH Ra 3.2 UNLESS OTHERWISE STATED",
            "Break sharp edges 0.2 max",
        ]
    );
    let part = step::read_part(doc.bytes(), &defs[0]).unwrap();
    for count in [3, 1, 0] {
        let mut settings = settings.clone();
        if count < 3 {
            settings.surface_textures.clear();
            settings.attributes.clear();
        }
        if count == 0 {
            settings.general.clear();
        }
        let semantic = json::identify(settings.clone(), None).unwrap();
        let generated = json::presentation::generated(&semantic, &part).unwrap();
        json::presentation::validate(&serde_json::to_value(&generated).unwrap()).unwrap();
        let blocks = unique_notes(&generated);
        assert_eq!(blocks.len(), usize::from(count != 0));
        if let Some(block) = blocks.first() {
            assert_eq!(block.text.len(), count + 1);
            assert_eq!(block.text[0], "GENERAL NOTES");
            assert_eq!(
                block
                    .text
                    .iter()
                    .filter(|s| s.as_str() == "GENERAL NOTES")
                    .count(),
                1
            );
            for (i, text) in block.text.iter().skip(1).enumerate() {
                assert!(text.starts_with(&format!("{}  ", i + 1)), "{text}");
            }
            assert_eq!(
                generated
                    .items
                    .values()
                    .filter(|gs| gs.iter().any(|g| g.id == block.id))
                    .count(),
                count
            );
            let rows = notes::general_notes(&settings);
            let expected = Layout::new(part.bounds()).notes(&notes::notes_block(&rows));
            assert_eq!(strokes(&block.polylines), strokes(&expected.lines));
            // All item aliases share one plane and its view lists the block once.
            let view = generated
                .views
                .iter()
                .find(|v| v.graphics.contains(block.id.as_ref().unwrap()))
                .unwrap();
            assert_eq!(view.graphics, [block.id.clone().unwrap()]);
        }
        // Add preserves the old file's private links and every retained entity.
        let (edit, _) = pmi::write(
            &doc,
            &defs,
            &[(PartId(0), settings)],
            pmi::write::Mode::Add,
            pmi::write::PresentationPolicy::Refuse,
        )
        .unwrap();
        let written = Document::parse(doc.apply(&edit).unwrap().bytes).unwrap();
        for id in doc.ids() {
            assert_eq!(
                &doc.bytes()[doc.span(id).unwrap()],
                &written.bytes()[written.span(id).unwrap()]
            );
        }
        let written_defs = step::read_part_definitions(written.bytes()).unwrap();
        let written_read = pmi::read(&written, &written_defs).unwrap();
        let written_semantic = json::identify(
            written_read.parts[0].clone(),
            Some(&written_read.identifiers.parts[0]),
        )
        .unwrap();
        let drawn = json::presentation::read(&written, &written_read, PartId(0), &written_semantic)
            .unwrap();
        let new_blocks: Vec<_> = unique_notes(&drawn)
            .into_iter()
            .filter(|g| {
                g.id.as_ref()
                    .unwrap()
                    .strip_prefix("step:")
                    .unwrap()
                    .parse::<u64>()
                    .unwrap()
                    > doc.max_id()
            })
            .collect();
        assert_eq!(new_blocks.len(), usize::from(count != 0));
        if let Some(block) = new_blocks.first() {
            assert_eq!(
                strokes(&block.polylines),
                strokes(&blocks[0].polylines),
                "JSON/STEP notes ink"
            );
            let root = block
                .id
                .as_ref()
                .unwrap()
                .strip_prefix("step:")
                .unwrap()
                .parse::<u64>()
                .unwrap();
            let occurrences = refs(&written, root);
            assert_eq!(occurrences.len(), count);
            let mut definitions = std::collections::BTreeSet::new();
            for occurrence in occurrences {
                let links: Vec<_> = written
                    .ids()
                    .filter(|&id| {
                        is(&written, id, "draughting_model_item_association")
                            && refs(&written, id).last() == Some(&occurrence)
                    })
                    .collect();
                assert_eq!(links.len(), 1, "each row owns its presentation link");
                let link = refs(&written, links[0]);
                assert!(is(&written, link[0], "property_definition"));
                assert!(definitions.insert(link[0]));
            }
            assert_eq!(definitions.len(), count);
        }
    }
}
