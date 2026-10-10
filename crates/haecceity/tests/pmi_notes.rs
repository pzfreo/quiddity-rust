//! What the PMI reader holds for a note's presentation and an attribute set's property: the
//! callouts that present a note (PMI practice §7.3) and the faces they are attached to, and an
//! attribute set's property definition description and `general_property`. The expected values
//! are read off the Part 21 text of the committed fixtures by hand; each case cites its chain.

mod common;

use std::io::Read;

use haecceity::p21::Document;
use haecceity::pmi::{
    self, Anchor, AttributeSet, Callout, FaceIndex, Feature, FindingKind, GeneralProperty, Note,
    NoteOwner, PmiRead,
};
use haecceity::step::{PartDefinition, read_part_definitions};

fn file_bytes(rel: &str) -> Vec<u8> {
    let path = common::fixtures().join("ap242").join(rel);
    let raw = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(raw.as_slice())
        .read_to_end(&mut out)
        .unwrap();
    out
}

fn read_bytes(bytes: Vec<u8>) -> (PmiRead, Vec<PartDefinition>) {
    let parts = read_part_definitions(&bytes).expect("part definitions");
    let doc = Document::parse(bytes).expect("document");
    let r = pmi::read(&doc, &parts).expect("pmi");
    for p in &r.parts {
        p.validate().unwrap();
    }
    (r, parts)
}

fn read(rel: &str) -> (PmiRead, Vec<PartDefinition>) {
    read_bytes(file_bytes(rel))
}

/// The bolt with `extra` instances added before its `ENDSEC;` (its part definition is #5, its
/// shape #4, its shape representation #10 in context #399; #17, #105 and #197 are faces of its
/// solid; #407 is datum A; #451 an annotation occurrence; #539 its draughting model).
fn bolt_with(extra: &str) -> (PmiRead, Vec<PartDefinition>) {
    let text = String::from_utf8(file_bytes("specify/bolt_thread_knurl.step.gz")).unwrap();
    let at = text.rfind("ENDSEC;").unwrap();
    read_bytes(format!("{}{extra}\n{}", &text[..at], &text[at..]).into_bytes())
}

/// The part whose shape is `shape`.
fn part_of(defs: &[PartDefinition], shape: u64) -> usize {
    defs.iter().position(|d| d.shape == shape).unwrap()
}

/// The note (and its index) of part `p` whose property definition is `pdef`.
fn note(r: &PmiRead, p: usize, pdef: u64) -> &Note {
    let i = r.provenance.parts[p]
        .notes
        .iter()
        .position(|ids| ids.contains(&pdef))
        .unwrap_or_else(|| panic!("no note of #{pdef}"));
    &r.parts[p].notes[i]
}

/// The attribute set of part `p` whose property definition is `pdef`.
fn attribute(r: &PmiRead, p: usize, pdef: u64) -> &AttributeSet {
    let i = r.provenance.parts[p]
        .attributes
        .iter()
        .position(|ids| ids.contains(&pdef))
        .unwrap_or_else(|| panic!("no attribute set of #{pdef}"));
    &r.parts[p].attributes[i]
}

/// The faces of a callout's features, as the file's `ADVANCED_FACE` ids.
fn callout_faces(r: &PmiRead, defs: &[PartDefinition], p: usize, c: &Callout) -> Vec<u64> {
    let mut faces = Vec::new();
    for f in &c.features {
        let Feature::Items(items) = &r.parts[p].features[f.0] else {
            panic!("callout feature {} is not made of items", f.0);
        };
        for a in items {
            let Anchor::Face(FaceIndex(i)) = a else {
                panic!("callout feature {} has {a:?}", f.0);
            };
            faces.push(defs[p].faces[*i]);
        }
    }
    faces.sort_unstable();
    faces
}

/// specify-core's notes on faces: each thread or knurl note's callout is related by a
/// `draughting_model_item_association` to the note's own shape aspect, which identifies its face
/// (`geometric_item_specific_usage`). draftwright-rust reads the same faces from the raw file
/// (`PartReader::presentation`: the part's callouts named like the note's representation, their
/// aspects' items); Python draftwright too, except that it matches callouts across the file, so
/// on the assembly each part's 'internal thread' note takes both parts' callouts (two faces:
/// ambiguous, none kept), where the port and haecceity keep each part's own.
#[test]
fn specify_core_notes_carry_their_callouts_and_faces() {
    // (file, part shape, note's property definition, callout name, face): the chain is note
    // → aspect (anchored by `notes_on_faces`) → usage → face, and aspect ← association → callout.
    let cases: [(&str, u64, u64, &str, u64); 7] = [
        // #444 'external thread'; aspect #446, usage #447 → #17; association #540 → #452.
        (
            "bolt_thread_knurl",
            4,
            444,
            "External thread requirement",
            17,
        ),
        // #475 'knurl'; aspect #477, usage #478 → #197; association #541 → #483.
        ("bolt_thread_knurl", 4, 475, "Knurl requirement", 197),
        // The plate (#34): #1526; aspect #1528, usage #1529 → #905; association #1607 → #1534.
        (
            "assembly_plate_pin",
            34,
            1526,
            "Internal thread requirement",
            905,
        ),
        // The pin (#971): #1621; aspect #1623, usage #1624 → #1138; association #1692 → #1629.
        (
            "assembly_plate_pin",
            971,
            1621,
            "Internal thread requirement",
            1138,
        ),
        // #689; aspect #691, usage #692 → #573; association #818 → #697.
        (
            "thumbwheel_thread_knurl",
            4,
            689,
            "Internal thread requirement",
            573,
        ),
        // #722; aspect #724, usage #725 → #479; association #819 → #730.
        ("thumbwheel_thread_knurl", 4, 722, "Knurl requirement", 479),
        // #3157; aspect #3159, usage #3160 → #2617; association #3217 → #3165.
        (
            "string_post_tapped",
            4,
            3157,
            "Internal thread requirement",
            2617,
        ),
    ];
    for (file, shape, pdef, name, face) in cases {
        let (r, defs) = read(&format!("specify/{file}.step.gz"));
        let p = part_of(&defs, shape);
        let n = note(&r, p, pdef);
        assert_eq!(n.callouts.len(), 1, "{file} #{pdef}: {:?}", n.callouts);
        let c = &n.callouts[0];
        assert_eq!(c.name, name, "{file} #{pdef}");
        assert_eq!(callout_faces(&r, &defs, p, c), [face], "{file} #{pdef}");
        // The callout's feature is the note's own.
        assert_eq!(
            n.on,
            Some(NoteOwner::Feature(c.features[0])),
            "{file} #{pdef}"
        );
    }
    // The thumbwheel's external thread: #749; aspect #751, usage #752 → #138; association
    // #820 → #757.
    let (r, defs) = read("specify/thumbwheel_thread_knurl.step.gz");
    let n = note(&r, 0, 749);
    assert_eq!(n.callouts[0].name, "External thread requirement");
    assert_eq!(callout_faces(&r, &defs, 0, &n.callouts[0]), [138]);
    // Part notes nothing presents (#780 surface texture, #784 edge condition, #788 general
    // tolerances: no association names them, and they are on the part).
    for pdef in [780, 784, 788] {
        assert!(note(&r, 0, pdef).callouts.is_empty(), "#{pdef}");
    }
    // Every note of every specify-core file has at most the callouts above.
    for file in [
        "assembly_plate_pin",
        "bolt_thread_knurl",
        "bracket_positions",
        "nist_ctc_01_merge",
        "spool_fits",
        "string_post_tapped",
        "thumbwheel_thread_knurl",
    ] {
        let (r, _) = read(&format!("specify/{file}.step.gz"));
        for (p, part) in r.parts.iter().enumerate() {
            for n in &part.notes {
                let presented = matches!(
                    n.kind.as_str(),
                    "internal thread" | "external thread" | "knurl"
                );
                assert_eq!(
                    n.callouts.len(),
                    usize::from(presented),
                    "{file} part {p}: {n:?}"
                );
            }
        }
    }
}

/// An attribute set holds its property definition's description and its general property:
/// specify-core's structured requirements are described 'pmi-assist' and associated with
/// `general_property('', 'user defined attribute', $)`; NIST CTC-02's editable text has no
/// description (`$`) and its general property is named as the attribute; the writer's attribute
/// sets are described 'user defined attribute', their general property named as the attribute.
#[test]
fn attribute_sets_carry_their_description_and_general_property() {
    let uda = |name: &str| GeneralProperty {
        id: String::new(),
        name: name.into(),
        description: None,
    };
    // (file, property definition, its name; associations in each case's comment): each
    // property definition's description is 'pmi-assist', each general property
    // ('', 'user defined attribute', $).
    let cases: [(&str, u64, &str); 6] = [
        // #468 ← association #470 → #469.
        ("bolt_thread_knurl", 468, "external thread"),
        // #495 ← #497 → #496.
        ("bolt_thread_knurl", 495, "knurl"),
        // #715 → #716, #742 → #743, #773 → #774.
        ("thumbwheel_thread_knurl", 715, "internal thread"),
        ("thumbwheel_thread_knurl", 742, "knurl"),
        ("thumbwheel_thread_knurl", 773, "external thread"),
        // #3183 → #3184.
        ("string_post_tapped", 3183, "internal thread"),
    ];
    for (file, pdef, name) in cases {
        let (r, _) = read(&format!("specify/{file}.step.gz"));
        let a = attribute(&r, 0, pdef);
        assert_eq!(a.name, name, "{file} #{pdef}");
        assert_eq!(
            a.description.as_deref(),
            Some("pmi-assist"),
            "{file} #{pdef}"
        );
        assert_eq!(
            a.general_property,
            Some(uda("user defined attribute")),
            "{file} #{pdef}"
        );
    }
    // The assembly's two parts: plate #1551, pin #1647, each 'pmi-assist'.
    let (r, defs) = read("specify/assembly_plate_pin.step.gz");
    for (shape, pdef) in [(34, 1551), (971, 1647)] {
        let a = attribute(&r, part_of(&defs, shape), pdef);
        assert_eq!(a.description.as_deref(), Some("pmi-assist"), "#{pdef}");
        assert_eq!(a.general_property, Some(uda("user defined attribute")));
    }
    // NIST CTC-02: #22163 PROPERTY_DEFINITION('semantic text',$,#1554) ← #21932 → #21937
    // GENERAL_PROPERTY('','semantic text',$).
    let (r, _) = read("nist/nist_ctc_02_asme1_ap242-e2.stp.gz");
    let a = attribute(&r, 0, 22163);
    assert_eq!(a.description, None);
    assert_eq!(a.general_property, Some(uda("semantic text")));
    // The writer's: every_kind's #6673 PROPERTY_DEFINITION('inspection','user defined
    // attribute',#6519) ← #6676 → #6672 GENERAL_PROPERTY('','inspection',$); #6679 'semantic
    // text' ← #6682 → #6678 GENERAL_PROPERTY('','semantic text',$).
    let (r, _) = read("write/every_kind.step.gz");
    for (pdef, name) in [(6673, "inspection"), (6679, "semantic text")] {
        let a = attribute(&r, 0, pdef);
        assert_eq!(a.description.as_deref(), Some("user defined attribute"));
        assert_eq!(a.general_property, Some(uda(name)), "#{pdef}");
    }
}

/// A callout related to the note's property definition is the note's, with the faces of the
/// part's shape aspects it is also related to; what cannot be held is reported, not dropped.
#[test]
fn a_callout_on_the_property_definition_and_what_is_not_held() {
    let (r, defs) = bolt_with(
        "#2000=DESCRIPTIVE_REPRESENTATION_ITEM('coating','Anodise');
#2001=REPRESENTATION('coating requirement',(#2000),#399);
#2002=PROPERTY_DEFINITION('manufacturing requirement','coating',#5);
#2003=PROPERTY_DEFINITION_REPRESENTATION(#2002,#2001);
#2004=SHAPE_ASPECT('coated','',#4,.T.);
#2005=GEOMETRIC_ITEM_SPECIFIC_USAGE('coated','',#2004,#10,#105);
#2006=DRAUGHTING_CALLOUT('Coating',(#451));
#2007=DRAUGHTING_MODEL_ITEM_ASSOCIATION('','',#2002,#539,#2006);
#2008=DRAUGHTING_MODEL_ITEM_ASSOCIATION('','',#2004,#539,#2006);
#2009=DRAUGHTING_MODEL_ITEM_ASSOCIATION('','',#407,#539,#2006);
#2010=DRAUGHTING_MODEL_ITEM_ASSOCIATION('','',#2002,#539,#10);
#2011=DRAUGHTING_CALLOUT('Coating, bare',(#451));
#2012=DRAUGHTING_MODEL_ITEM_ASSOCIATION('','',#2002,#539,#2011);",
    );
    let n = note(&r, 0, 2002);
    assert_eq!(n.kind, "coating");
    assert_eq!(n.on, None);
    // #2006: related to the note (#2007), to aspect #2004 (#2008, usage #2005 → #105), and to
    // datum A (#2009, no feature: reported); #2011: to the note alone.
    assert_eq!(n.callouts.len(), 2, "{:?}", n.callouts);
    assert_eq!(n.callouts[0].name, "Coating");
    assert_eq!(callout_faces(&r, &defs, 0, &n.callouts[0]), [105]);
    assert_eq!(n.callouts[1].name, "Coating, bare");
    assert!(n.callouts[1].features.is_empty());
    let found = |subject: u64| {
        r.findings
            .iter()
            .find(|f| f.ids.first() == Some(&subject))
            .unwrap_or_else(|| panic!("no finding on #{subject}: {:#?}", r.findings))
    };
    // #2009 relates the callout to datum A, which is not a feature.
    let f = found(2009);
    assert_eq!(f.kind, FindingKind::NotModelled);
    assert_eq!(f.ids, [2009, 2006, 407]);
    // #2010 relates the note to a shape representation, which is no callout.
    let f = found(2010);
    assert_eq!(f.kind, FindingKind::NotModelled);
    assert_eq!(f.ids, [2010, 10, 2002]);
    // The existing notes are as before.
    assert_eq!(
        note(&r, 0, 444).callouts[0].name,
        "External thread requirement"
    );
}

#[test]
fn a_general_property_that_is_not_one_is_reported() {
    let (r, _) = bolt_with(
        "#2020=DESCRIPTIVE_REPRESENTATION_ITEM('gauge','plug');
#2021=REPRESENTATION('inspection',(#2020),#399);
#2022=PROPERTY_DEFINITION('inspection','pmi-assist',#5);
#2023=PROPERTY_DEFINITION_REPRESENTATION(#2022,#2021);
#2024=GENERAL_PROPERTY('g1','inspection','first');
#2025=GENERAL_PROPERTY_ASSOCIATION('',$,#2024,#2022);
#2026=GENERAL_PROPERTY('g2','inspection',$);
#2027=GENERAL_PROPERTY_ASSOCIATION('',$,#2026,#2022);
#2030=DESCRIPTIVE_REPRESENTATION_ITEM('jig','J-4');
#2031=REPRESENTATION('fixture',(#2030),#399);
#2032=PROPERTY_DEFINITION('fixture','',#5);
#2033=PROPERTY_DEFINITION_REPRESENTATION(#2032,#2031);
#2034=GENERAL_PROPERTY_ASSOCIATION('',$,#2030,#2032);
#2040=DESCRIPTIVE_REPRESENTATION_ITEM('gauge','ring');
#2041=REPRESENTATION('check',(#2040),#399);
#2042=PROPERTY_DEFINITION('check','',#5);
#2043=PROPERTY_DEFINITION_REPRESENTATION(#2042,#2041);
#2044=GENERAL_PROPERTY('g3','check','a ring gauge');
#2045=GENERAL_PROPERTY_ASSOCIATION('',$,#2044,#2042);",
    );
    // Two associations (general_property_association WR1): read, its general property not held.
    let a = attribute(&r, 0, 2022);
    assert_eq!(a.description.as_deref(), Some("pmi-assist"));
    assert_eq!(a.general_property, None);
    let f = r
        .findings
        .iter()
        .find(|f| f.ids.first() == Some(&2022))
        .unwrap();
    assert_eq!(f.kind, FindingKind::Nonconformance);
    assert_eq!(f.ids, [2022, 2025, 2027]);
    // An association whose base is no general property.
    let a = attribute(&r, 0, 2032);
    assert_eq!(a.description, None);
    assert_eq!(a.general_property, None);
    let f = r
        .findings
        .iter()
        .find(|f| f.ids.first() == Some(&2034))
        .unwrap();
    assert_eq!(f.kind, FindingKind::Unresolved);
    assert_eq!(f.ids, [2034, 2032]);
    // One, stated in full.
    assert_eq!(
        attribute(&r, 0, 2042).general_property,
        Some(GeneralProperty {
            id: "g3".into(),
            name: "check".into(),
            description: Some("a ring gauge".into()),
        })
    );
}
