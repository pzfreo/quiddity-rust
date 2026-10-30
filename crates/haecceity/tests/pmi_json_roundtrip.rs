#![cfg(feature = "json")]
use haecceity::{
    json::{self, Anchors},
    p21::Document,
    pmi::{self, PartId},
    step::{read_part, read_part_definitions},
};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
};
fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    if !dir.is_dir() {
        return;
    }
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            files(&p, out)
        } else {
            let s = p.to_string_lossy().to_ascii_lowercase();
            if [".step", ".stp", ".step.gz", ".stp.gz"]
                .iter()
                .any(|ext| s.ends_with(ext))
            {
                out.push(p);
            }
        }
    }
}
fn bytes(p: &Path) -> Vec<u8> {
    let raw = std::fs::read(p).unwrap();
    if p.extension().is_some_and(|x| x == "gz") {
        let mut b = vec![];
        flate2::read::GzDecoder::new(raw.as_slice())
            .read_to_end(&mut b)
            .unwrap();
        b
    } else {
        raw
    }
}
fn items(p: &pmi::PartPmi) -> usize {
    p.features.len()
        + p.datum_targets.len()
        + p.datums.len()
        + p.dimensions.len()
        + p.tolerances.len()
        + p.tolerance_relations.len()
        + p.general.len()
        + p.threads.len()
        + p.knurls.len()
        + p.holes.len()
        + p.notes.len()
        + p.surface_textures.len()
        + p.attributes.len()
        + p.geometry.len()
        + p.derived_geometry.len()
        + p.standards.len()
        + usize::from(p.material.is_some())
        + usize::from(p.decimal_places.is_some())
}
#[test]
#[ignore = "strict #142 acceptance: existing source/writer refusals must remain visible; run with --ignored"]
fn nist_and_corpus_roundtrip() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let corpus = std::env::var_os("QUIDDITY_CORPUS")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest.join("../../../quiddity/tests/corpus"));
    if std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_some() {
        assert!(corpus.is_dir(), "QUIDDITY_CORPUS_REQUIRED: corpus missing");
    }
    let mut paths = vec![];
    files(&corpus, &mut paths);
    let corpus_files = paths.len();
    files(
        &manifest.join("../../tests/fixtures/ap242/nist"),
        &mut paths,
    );
    if let Some(dir) = std::env::var_os("HAECCEITY_NIST_PMI") {
        files(Path::new(&dir), &mut paths);
    }
    // The acceptance set is CTC/FTC; STC fixtures exercise read-only model
    // extensions separately (including AP242 ed4 hole definitions).
    paths.retain(|p| {
        !p.file_name()
            .unwrap()
            .to_string_lossy()
            .contains("nist_stc_")
    });
    if let Ok(filter) = std::env::var("HAECCEITY_JSON_FILTER") {
        paths.retain(|p| p.to_string_lossy().contains(&filter));
    }
    paths.sort();
    paths.dedup();
    assert!(!paths.is_empty());
    let (mut parts_count, mut item_count, mut json_parts, mut written_parts, mut written_items) =
        (0, 0, 0, 0, 0);
    let mut errors = vec![];
    let mut blocked = BTreeMap::<String, usize>::new();
    let mut samples = (false, false, false);
    for path in &paths {
        eprintln!("roundtrip {}", path.display());
        let input = bytes(path);
        let doc = Document::parse(input.clone()).unwrap();
        let parts = match read_part_definitions(&input) {
            Ok(parts) => parts,
            Err(e) => {
                errors.push(format!("{}: no PartPmi geometry: {e}", path.display()));
                continue;
            }
        };
        let read = pmi::read(&doc, &parts).unwrap();
        parts_count += parts.len();
        item_count += read.parts.iter().map(items).sum::<usize>();
        let mut semantic = vec![];
        for (i, p) in read.parts.iter().enumerate() {
            let result = json::identify(p.clone(), Some(&read.identifiers.parts[i]))
                .and_then(|s| json::to_value(&s))
                .and_then(|v| json::from_value(&v, &Anchors::read(&doc, &parts[i])));
            match result {
                Ok(decoded) => {
                    assert_eq!(
                        &decoded.pmi,
                        p,
                        "{} part {i}: exact JSON model roundtrip",
                        path.display()
                    );
                    json_parts += 1;
                    semantic.push((PartId(i), decoded));
                }
                Err(e) => errors.push(format!("{} part {i}: {e}", path.display())),
            }
        }
        if semantic.len() != parts.len() {
            continue;
        }
        match json::write(
            &doc,
            &parts,
            &semantic,
            pmi::write::PresentationPolicy::RemovePresentation,
        ) {
            Err(e) => {
                eprintln!("STEP BLOCKER {}: {e}", path.display());
                *blocked.entry(e.to_string()).or_default() += 1;
            }
            Ok((output, report)) => {
                let after = Document::parse(output.clone()).unwrap();
                let definitions = read_part_definitions(&output).unwrap();
                let reread = pmi::read(&after, &definitions).unwrap();
                for (i, p) in read.parts.iter().enumerate() {
                    let differences = pmi::write::differences(p, &reread.parts[i]);
                    if !differences.is_empty() {
                        errors.push(format!(
                            "{} part {i}: STEP differences {differences:?}",
                            path.display()
                        ));
                    }
                    written_parts += 1;
                    written_items += items(p);
                }
                // Writer already checks made instances and WHERE/UNIQUE rules. Compare full
                // documents as well, allowing only violations that predate the edit or the
                // writer's explicitly reported existing practice/schema conflicts.
                if output != input {
                    let before_schema = haecceity::express::validate_document(doc.graph());
                    let added_schema = haecceity::express::validate_document(after.graph())
                        .into_iter()
                        .filter(|v| v.id > doc.max_id())
                        .collect::<Vec<_>>();
                    assert!(
                        added_schema.is_empty(),
                        "{}: newly written schema violations: {added_schema:?}; before {}",
                        path.display(),
                        before_schema.len()
                    );
                    let rules = haecceity::express_rules::check_all(&after);
                    let new_rules = rules.iter().filter(|v| {
                        v.id > doc.max_id()
                            && !report
                                .practice_schema_conflicts
                                .iter()
                                .any(|p| p.rule == v.rule)
                    });
                    for v in new_rules {
                        errors.push(format!(
                            "{}: newly written rule violation {v:?}",
                            path.display()
                        ));
                    }
                    // Geometry output is the same STEP B-rep. Validate new output against input
                    // per solid, so existing invalid geometry is reported rather than hidden.
                    for (i, part) in parts.iter().enumerate() {
                        let before = read_part(&input, part).unwrap();
                        let after_part = read_part(&output, &definitions[i]).unwrap();
                        assert_eq!(before.solids.len(), after_part.solids.len());
                        for solid in 0..before.solids.len() {
                            assert_eq!(
                                before.solid_is_valid(solid),
                                after_part.solid_is_valid(solid)
                            );
                            assert_eq!(
                                before.solid_is_geometrically_valid(solid),
                                after_part.solid_is_geometrically_valid(solid)
                            );
                        }
                    }
                }
                if let Some(dir) = std::env::var_os("HAECCEITY_JSON_SAMPLES") {
                    let name = path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .to_ascii_lowercase();
                    let sample = if !samples.0 && name.contains("ctc_01") && name.contains("ap242")
                    {
                        samples.0 = true;
                        Some("CTC-01.step")
                    } else if !samples.1 && name.contains("ftc_06") && name.contains("ap242") {
                        samples.1 = true;
                        Some("FTC-06.step")
                    } else if !samples.2 && path.starts_with(&corpus) && !name.contains("nist") {
                        samples.2 = true;
                        Some("corpus.step")
                    } else {
                        None
                    };
                    if let Some(sample) = sample {
                        std::fs::create_dir_all(&dir).unwrap();
                        std::fs::write(Path::new(&dir).join(sample), output).unwrap();
                    }
                }
            }
        }
    }
    eprintln!(
        "JSON ROUNDTRIP: {} files ({corpus_files} corpus), {parts_count} parts, {item_count} items; {json_parts} exact JSON parts; {written_parts} written STEP parts, {written_items} items",
        paths.len()
    );
    for (reason, count) in &blocked {
        eprintln!("STEP WRITE BLOCKED ({count} files): {reason}");
    }
    for e in &errors {
        eprintln!("ROUNDTRIP ERROR: {e}");
    }
    assert!(
        errors.is_empty(),
        "{} JSON or STEP roundtrip errors",
        errors.len()
    );
    assert!(
        blocked.is_empty(),
        "{} distinct STEP writer blockers (no items removed)",
        blocked.len()
    );
}

/// Exact PartPmi mapping is independent of the pre-existing writer refusals.
/// File identifiers are checked separately; duplicate source identifiers are an
/// explicit export refusal, while the underlying model can still be authored
/// with caller-chosen deterministic ids.
#[test]
fn nist_and_corpus_model_json_roundtrip() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut paths = vec![];
    if let Some(dir) = std::env::var_os("QUIDDITY_CORPUS") {
        assert!(
            Path::new(&dir).is_dir(),
            "QUIDDITY_CORPUS is not a directory"
        );
        files(Path::new(&dir), &mut paths);
    } else {
        assert!(
            std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none(),
            "required corpus unset"
        );
    }
    files(
        &manifest.join("../../tests/fixtures/ap242/nist"),
        &mut paths,
    );
    if let Some(dir) = std::env::var_os("HAECCEITY_NIST_PMI") {
        files(Path::new(&dir), &mut paths);
    }
    paths.retain(|p| {
        !p.file_name()
            .unwrap()
            .to_string_lossy()
            .contains("nist_stc_")
    });
    paths.sort();
    paths.dedup();
    let (mut count, mut item_count, mut source_refusals, mut no_geometry) = (0, 0, 0, 0);
    for path in &paths {
        let input = bytes(path);
        let doc = Document::parse(input.clone()).unwrap();
        let parts = match read_part_definitions(&input) {
            Ok(p) => p,
            Err(e) => {
                assert!(
                    path.to_string_lossy().contains("-tg"),
                    "{}: {e}",
                    path.display()
                );
                no_geometry += 1;
                continue;
            }
        };
        let read = pmi::read(&doc, &parts).unwrap();
        for (i, p) in read.parts.iter().enumerate() {
            let semantic = match json::identify(p.clone(), Some(&read.identifiers.parts[i])) {
                Ok(s) => s,
                Err(e) => {
                    assert!(
                        e.to_string().contains("duplicate or empty item id"),
                        "{}: {e}",
                        path.display()
                    );
                    source_refusals += 1;
                    json::identify(p.clone(), None).unwrap()
                }
            };
            let encoded = json::to_value(&semantic)
                .unwrap_or_else(|e| panic!("{} part {i}: {e}", path.display()));
            let decoded = json::from_value(&encoded, &Anchors::read(&doc, &parts[i]))
                .unwrap_or_else(|e| panic!("{} part {i}: {e}", path.display()));
            assert_eq!(&decoded.pmi, p, "{} part {i}", path.display());
            count += 1;
            item_count += items(p);
        }
    }
    eprintln!(
        "EXACT JSON MODEL ROUNDTRIP: {} files, {count} parts, {item_count} items; {source_refusals} source-id refusals, {no_geometry} tessellation-only files without PartPmi geometry",
        paths.len()
    );
}
