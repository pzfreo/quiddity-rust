#![cfg(feature = "json")]
use haecceity::{
    json::{self, Anchors},
    p21::Document,
    pmi::{self, PartId},
    step::read_part_definitions,
};
use std::{io::Read, path::Path};
fn bytes(path: &Path) -> Vec<u8> {
    let raw = std::fs::read(path).unwrap();
    if path.extension().is_some_and(|e| e == "gz") {
        let mut b = vec![];
        flate2::read::GzDecoder::new(raw.as_slice())
            .read_to_end(&mut b)
            .unwrap();
        b
    } else {
        raw
    }
}
#[test]
fn whole_json_identity_ctc02_ftc07_xcaf_and_corpus() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/ap242");
    for (name, path, input) in [
        (
            "CTC-02",
            root.join("nist/nist_ctc_02_asme1_ap242-e2.stp.gz"),
            None,
        ),
        (
            "FTC-07",
            root.join("nist/nist_ftc_07_asme1_ap242-e2.stp.gz"),
            None,
        ),
        (
            "xcaf plate",
            root.join("xcaf/pmi_plate.geom.step.gz"),
            Some(root.join("xcaf/pmi_plate.pmi.json")),
        ),
        (
            "corpus spool",
            root.join("specify/spool_fits.step.gz"),
            None,
        ),
    ] {
        let doc = Document::parse(bytes(&path)).unwrap();
        let parts = read_part_definitions(doc.bytes()).unwrap();
        let read = pmi::read(&doc, &parts).unwrap();
        let mut requested = vec![];
        let mut expected = vec![];
        for (i, p) in read.parts.iter().enumerate() {
            let anchors = Anchors::read(&doc, &parts[i]);
            let semantic = if let Some(path) = &input {
                json::from_str(&std::fs::read_to_string(path).unwrap(), &anchors).unwrap()
            } else {
                json::identify(p.clone(), Some(&read.identifiers.parts[i])).unwrap()
            };
            expected.push(json::to_value(&semantic).unwrap());
            requested.push((PartId(i), semantic));
        }
        let (output, report) = match json::write(
            &doc,
            &parts,
            &requested,
            pmi::write::PresentationPolicy::RemovePresentation,
        ) {
            Ok(result) => result,
            Err(e) if name == "FTC-07" => {
                assert!(
                    e.to_string()
                        .contains("units of the file are named \"derived_unit\""),
                    "{e}"
                );
                eprintln!("{name}: exact identity refused explicitly: {e}");
                continue;
            }
            Err(e) => panic!("{name}: {e}"),
        };
        let after = Document::parse(output).unwrap();
        let definitions = read_part_definitions(after.bytes()).unwrap();
        let reread = pmi::read(&after, &definitions).unwrap();
        for (i, model) in reread.parts.iter().enumerate() {
            let actual = json::to_value(
                &json::identify(model.clone(), Some(&reread.identifiers.parts[i])).unwrap(),
            )
            .unwrap();
            assert_eq!(actual, expected[i], "{name} part {i}: whole canonical JSON");
            assert!(pmi::write::differences(&requested[i].1.pmi, model).is_empty());
        }
        eprintln!(
            "{name}: whole JSON identity passed, {} parts; {} presentation removals",
            parts.len(),
            report.presentation_removed.len()
        );
    }
}
