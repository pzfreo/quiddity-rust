//! Structural presentation/semantic associations, checked against Part 21 instances.
mod common;

use haecceity::{
    p21::Document,
    pmi::{self, PmiRead, PresentedItem},
    step::read_part_definitions,
};
use std::{io::Read, path::Path};

fn read(bytes: Vec<u8>) -> PmiRead {
    let parts = read_part_definitions(&bytes).unwrap();
    let doc = Document::parse(bytes).unwrap();
    pmi::read(&doc, &parts).unwrap()
}

fn file(path: &Path) -> PmiRead {
    let mut bytes = std::fs::read(path).unwrap();
    if path.extension().is_some_and(|ext| ext == "gz") {
        let mut decoded = Vec::new();
        flate2::read::GzDecoder::new(bytes.as_slice())
            .read_to_end(&mut decoded)
            .unwrap();
        bytes = decoded;
    }
    read(bytes)
}

fn counts(r: &PmiRead) -> (usize, usize, usize) {
    let graphic_only = r.callouts.iter().filter(|c| c.links.is_empty()).count();
    let unresolved = r
        .callouts
        .iter()
        .filter(|c| c.links.iter().any(|l| l.targets.is_empty()))
        .count();
    (r.callouts.len() - graphic_only, graphic_only, unresolved)
}

#[test]
fn graphic_only_unresolved_and_containment() {
    let bytes = br#"ISO-10303-21;
HEADER;
FILE_DESCRIPTION(('callouts'),'2;1');
FILE_NAME('','',(''),(''),'','','');
FILE_SCHEMA(('AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF'));
ENDSEC;
DATA;
#1=DRAUGHTING_CALLOUT('\X2\2300\X0\3.3 H11',(#2));
#2=TESSELLATED_ANNOTATION_OCCURRENCE('member',(),$);
#3=ANNOTATION_PLANE('container',(),$,(#1,#4));
#4=ANNOTATION_TEXT_OCCURRENCE('standalone',(),$);
#5=DRAUGHTING_MODEL_ITEM_ASSOCIATION('','',#6,#7,#2);
#6=PRODUCT('unsupported definition','','',());
#7=DRAUGHTING_MODEL('',(#3),$);
#8=DRAUGHTING_CALLOUT('graphic only',());
#9=DRAUGHTING_MODEL_ITEM_ASSOCIATION_WITH_PLACEHOLDER('','',#10,#7,#4,$);
#10=SHAPE_ASPECT('unread aspect','',$,.T.);
#11=ANNOTATION_PLANE('another container',(),$,(#12));
#12=TESSELLATED_ANNOTATION_OCCURRENCE('plane-held',(),$);
#13=(DRAUGHTING_CALLOUT((#14)) GEOMETRIC_REPRESENTATION_ITEM() REPRESENTATION_ITEM('complex'));
#14=(ANNOTATION_CURVE_OCCURRENCE() ANNOTATION_OCCURRENCE() REPRESENTATION_ITEM('complex member') STYLED_ITEM((),$));
#15=(DRAUGHTING_MODEL_ITEM_ASSOCIATION() ITEM_IDENTIFIED_REPRESENTATION_USAGE('','',#6,#7,#14));
ENDSEC;
END-ISO-10303-21;"#
        .to_vec();
    let doc = Document::parse(bytes).unwrap();
    let r = pmi::read(&doc, &[]).unwrap();
    assert_eq!(
        r.callouts.iter().map(|c| c.id).collect::<Vec<_>>(),
        [1, 4, 8, 12, 13]
    );
    assert_eq!(r.callouts[0].name, "⌀3.3 H11");
    let link = &r.callouts[0].links[0];
    assert_eq!(
        (link.association, link.definition, link.entity.as_str()),
        (5, 6, "product")
    );
    assert!(link.targets.is_empty());
    assert_eq!(r.callouts[1].links[0].entity, "shape_aspect");
    assert_eq!(r.callouts[4].name, "complex");
    assert_eq!(r.callouts[4].links[0].association, 15);
    assert_eq!(counts(&r), (3, 2, 3));
    assert_eq!(r.accounting.presentation, 10);
}

#[test]
fn nist_ctc_associations_resolve_to_own_items() {
    let r = file(&common::fixtures().join("ap242/nist/nist_ctc_01_asme1_ap242-e1.stp.gz"));
    println!(
        "NIST CTC01 fixture linked/graphic-only/unresolved: {:?}",
        counts(&r)
    );
    assert_eq!(r.callouts.len(), 23);
    assert_eq!(counts(&r), (23, 0, 0));
    assert!(r.callouts.iter().all(|c| !c.name.is_empty()));
    // #607 'Linear Size.1': #130 → dimensional_size #120; #129 → aspect #219.
    // #611 'Flatness.1': #138 → flatness_tolerance #57; #137 → aspect #297.
    // #615 'Simple Datum.1': #146 → datum_feature #34; #145 → aspect #303.
    for (id, name, assoc, definition) in [
        (607, "Linear Size.1", 130, 120),
        (611, "Flatness.1", 138, 57),
        (615, "Simple Datum.1", 146, 34),
    ] {
        let c = r.callouts.iter().find(|c| c.id == id).unwrap();
        assert_eq!(c.name, name);
        let link = c.links.iter().find(|l| l.association == assoc).unwrap();
        assert_eq!(link.definition, definition);
        assert_eq!(link.targets.len(), if id == 615 { 2 } else { 1 });
        let target = &link.targets[0];
        let prov = &r.provenance.parts[target.part.0];
        let ids = match target.item {
            PresentedItem::Dimension(i) => &prov.dimensions[i.0],
            PresentedItem::Tolerance(i) => &prov.tolerances[i.0],
            PresentedItem::Feature(i) => &prov.features[i.0],
            ref other => panic!("unexpected target {other:?}"),
        };
        assert!(ids.contains(&definition));
    }
}

/// Clone-root samples are deliberately excluded from git; committed NIST coverage above
/// always runs. Run this audit explicitly when the maintainer's sample files are available.
#[test]
#[ignore = "requires the maintainer's git-excluded samples"]
fn maintainer_samples() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples");
    for name in [
        "19-spindle-semantic-links-only.step",
        "09-NIST-reexported-by-SE.stp",
        "01-NIST-control-ctc01.stp",
    ] {
        let r = file(&root.join(name));
        println!("{name}: {:?}", counts(&r));
        if name.starts_with("19-") {
            assert_eq!(r.callouts.len(), 13);
            assert_eq!(counts(&r), (13, 0, 0));
            assert!(r.callouts.iter().all(|c| !c.name.is_empty()));
            assert_eq!(
                r.callouts.iter().find(|c| c.id == 1469).unwrap().name,
                "⌀3.3 H11"
            );
        }
        for c in &r.callouts {
            println!("#{} {:?}: {:?}", c.id, c.name, c.links);
        }
    }
}

#[test]
fn notes_on_member_occurrences_are_aggregated_without_duplicates() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/presentation/spindle.step.gz");
    let r = file(&path);
    let general = r.callouts.iter().find(|c| c.id == 2052).unwrap();
    assert_eq!(r.callouts.len(), 14);
    assert_eq!(general.name, "General notes");
    // #2073..#2077 identify the five member occurrences, not their parent #2052.
    assert_eq!(
        general
            .links
            .iter()
            .map(|l| l.association)
            .collect::<Vec<_>>(),
        [2073, 2074, 2075, 2076, 2077]
    );
    assert!(general.links.iter().all(|l| !l.targets.is_empty()));
    assert!(
        general
            .links
            .iter()
            .flat_map(|l| &l.targets)
            .any(|t| matches!(t.item, PresentedItem::Attribute(_)))
    );
    let r = file(&common::fixtures().join("ap242/specify/bolt_thread_knurl.step.gz"));
    // Existing note associations continue to hold their own occurrence names.
    let notes: Vec<_> = r
        .parts
        .iter()
        .flat_map(|p| &p.notes)
        .flat_map(|n| &n.callouts)
        .collect();
    assert!(!notes.is_empty());
    for note in notes {
        assert!(!note.name.is_empty());
    }
}

#[cfg(feature = "json-import")]
#[test]
fn transport_aliases_keep_callout_targets_in_returned_model_order() {
    use haecceity::{json, pmi::*};
    let bytes = std::fs::read(common::fixtures().join("ties/cube.step")).unwrap();
    let parts = read_part_definitions(&bytes).unwrap();
    let doc = Document::parse(bytes).unwrap();
    let feature = Feature::Items(vec![Anchor::Face(FaceIndex(0))]);
    let model = PartPmi {
        features: vec![feature.clone(), feature],
        datums: vec![
            Datum::new(DatumLabel::new("A").unwrap(), Some(FeatureId(1)), vec![]).unwrap(),
        ],
        dimensions: [1, 0]
            .into_iter()
            .enumerate()
            .map(|(i, f)| Dimension {
                kind: DimensionKind::Size {
                    feature: FeatureId(f),
                    kind: SizeKind::Thickness,
                    path: None,
                    angle: None,
                },
                nominal: Some(Value::length(
                    Decimal::parse(if i == 0 { "10.0" } else { "20.0" }).unwrap(),
                    LengthUnit::Millimetre,
                )),
                tolerance: DimTolerance::Basic,
                qualifier: None,
                modifiers: vec![],
                principle: None,
            })
            .collect(),
        ..PartPmi::default()
    };
    let semantic = json::identify(model.clone(), None).unwrap();
    let (bytes, _) = json::write(
        &doc,
        &parts,
        &[(PartId(0), semantic)],
        pmi::write::PresentationPolicy::RemovePresentation,
    )
    .unwrap();
    let parts = read_part_definitions(&bytes).unwrap();
    let doc = Document::parse(bytes).unwrap();
    let r = pmi::read(&doc, &parts).unwrap();
    assert_eq!(r.parts[0], model);
    assert!(!r.callouts.is_empty());
    let mut dimensions = 0;
    for link in r.callouts.iter().flat_map(|c| &c.links) {
        for target in &link.targets {
            if let PresentedItem::Dimension(i) = target.item {
                dimensions += 1;
                assert!(
                    r.provenance.parts[target.part.0].dimensions[i.0].contains(&link.definition)
                );
            }
        }
    }
    assert_eq!(dimensions, 2);
    assert!(r.callouts.iter().flat_map(|c| &c.links).any(|l| {
        l.targets
            .iter()
            .filter(|t| matches!(t.item, PresentedItem::Feature(_)))
            .count()
            == 2
    }));
}
