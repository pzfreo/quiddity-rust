//! The AP242 PMI writer's round trips: read → replace (every part with what was read) →
//! read gives the same PMI, for every NIST AP242 file and every specify-core input.

mod common;

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use haecceity::p21::Document;
use haecceity::pmi::write::{ItemRef, Mode, PresentationPolicy, WriteError, differences};
use haecceity::pmi::{self, *};
use haecceity::step::{PartDefinition, read_part_definitions};

fn file_bytes(path: &Path) -> Vec<u8> {
    let raw = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    if path.extension().is_some_and(|e| e == "gz") {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(raw.as_slice())
            .read_to_end(&mut out)
            .unwrap();
        out
    } else {
        raw
    }
}

/// The part's PMI without the given items and everything that depends on them, indices
/// renumbered.
fn strip(p: &PartPmi, refused: &[ItemRef]) -> PartPmi {
    let has = |r: ItemRef| refused.contains(&r);
    let mut gone_f: Vec<bool> = (0..p.features.len())
        .map(|i| has(ItemRef::Feature(i)))
        .collect();
    loop {
        let mut changed = false;
        for (i, f) in p.features.iter().enumerate() {
            if gone_f[i] {
                continue;
            }
            let members = match f {
                Feature::Group { members, .. } | Feature::Derived { from: members, .. } => {
                    members.clone()
                }
                Feature::Items(_) => Vec::new(),
            };
            if members.iter().any(|m| gone_f[m.0]) {
                gone_f[i] = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let f_ok = |f: FeatureId| !gone_f[f.0];
    let gone_t: Vec<bool> = p
        .datum_targets
        .iter()
        .enumerate()
        .map(|(i, t)| {
            has(ItemRef::DatumTarget(i))
                || t.on().is_some_and(|f| !f_ok(f))
                || matches!(t.shape(), TargetShape::Area(f) | TargetShape::Curve(f) if !f_ok(*f))
        })
        .collect();
    let gone_d: Vec<bool> = p
        .datums
        .iter()
        .enumerate()
        .map(|(i, d)| {
            has(ItemRef::Datum(i))
                || d.feature().is_some_and(|f| !f_ok(f))
                || d.targets().iter().any(|t| gone_t[t.0])
        })
        .collect();
    let gone_n: Vec<bool> = p
        .dimensions
        .iter()
        .enumerate()
        .map(|(i, d)| {
            has(ItemRef::Dimension(i))
                || match &d.kind {
                    DimensionKind::Size { feature, path, .. } => {
                        !f_ok(*feature) || path.is_some_and(|x| !f_ok(x))
                    }
                    DimensionKind::Location { from, to, path, .. } => {
                        !f_ok(*from) || !f_ok(*to) || path.is_some_and(|x| !f_ok(x))
                    }
                }
        })
        .collect();
    let gone_g: Vec<bool> = p
        .tolerances
        .iter()
        .enumerate()
        .map(|(i, t)| {
            has(ItemRef::Tolerance(i))
                || match &t.target {
                    ToleranceTarget::Feature(f) => !f_ok(*f),
                    ToleranceTarget::Dimension(d) => gone_n[d.0],
                    ToleranceTarget::Relation {
                        relating, related, ..
                    } => !f_ok(*relating) || !f_ok(*related),
                    ToleranceTarget::WholePart => false,
                }
                || t.datums.as_ref().is_some_and(|s| {
                    s.compartments()
                        .iter()
                        .any(|c| c.references.iter().any(|r| gone_d[r.datum.0]))
                })
                || t.zone.as_ref().is_some_and(|z| {
                    z.projected.iter().filter_map(|p| p.end).any(|f| !f_ok(f))
                        || z.non_uniform.iter().flatten().any(|f| !f_ok(*f))
                        || z.affected_plane.is_some_and(|f| !f_ok(f))
                })
        })
        .collect();
    let index = |gone: &[bool]| -> Vec<Option<usize>> {
        let mut n = 0;
        gone.iter()
            .map(|g| {
                if *g {
                    None
                } else {
                    n += 1;
                    Some(n - 1)
                }
            })
            .collect()
    };
    let (fi, ti, di, ni, gi) = (
        index(&gone_f),
        index(&gone_t),
        index(&gone_d),
        index(&gone_n),
        index(&gone_g),
    );
    let f = |x: FeatureId| FeatureId(fi[x.0].unwrap());
    let owner_ok = |o: &Option<NoteOwner>| match o {
        None => true,
        Some(NoteOwner::Feature(x)) => f_ok(*x),
        Some(NoteOwner::Dimension(x)) => !gone_n[x.0],
        Some(NoteOwner::Tolerance(x)) => !gone_g[x.0],
        Some(NoteOwner::DatumTarget(x)) => !gone_t[x.0],
    };
    let owner = |o: &Option<NoteOwner>| {
        o.map(|o| match o {
            NoteOwner::Feature(x) => NoteOwner::Feature(f(x)),
            NoteOwner::Dimension(x) => NoteOwner::Dimension(DimensionId(ni[x.0].unwrap())),
            NoteOwner::Tolerance(x) => NoteOwner::Tolerance(ToleranceId(gi[x.0].unwrap())),
            NoteOwner::DatumTarget(x) => NoteOwner::DatumTarget(DatumTargetId(ti[x.0].unwrap())),
        })
    };
    let features = p
        .features
        .iter()
        .enumerate()
        .filter(|(i, _)| !gone_f[*i])
        .map(|(_, x)| match x {
            Feature::Items(a) => Feature::Items(a.clone()),
            Feature::Group { members, kind } => Feature::Group {
                members: members.iter().map(|m| f(*m)).collect(),
                kind: kind.clone(),
            },
            Feature::Derived { kind, from } => Feature::Derived {
                kind: *kind,
                from: from.iter().map(|m| f(*m)).collect(),
            },
        })
        .collect();
    let datum_targets = p
        .datum_targets
        .iter()
        .enumerate()
        .filter(|(i, _)| !gone_t[*i])
        .map(|(_, t)| {
            let shape = match t.shape() {
                TargetShape::Area(x) => TargetShape::Area(f(*x)),
                TargetShape::Curve(x) => TargetShape::Curve(f(*x)),
                s => s.clone(),
            };
            DatumTarget::new(
                t.number(),
                shape,
                t.placement().cloned(),
                t.movable().cloned(),
                t.on().map(f),
            )
            .unwrap()
        })
        .collect();
    let datums = p
        .datums
        .iter()
        .enumerate()
        .filter(|(i, _)| !gone_d[*i])
        .map(|(_, d)| {
            Datum::new(
                d.label().clone(),
                d.feature().map(f),
                d.targets()
                    .iter()
                    .map(|t| DatumTargetId(ti[t.0].unwrap()))
                    .collect(),
            )
            .unwrap()
        })
        .collect();
    let dimensions = p
        .dimensions
        .iter()
        .enumerate()
        .filter(|(i, _)| !gone_n[*i])
        .map(|(_, d)| {
            let mut d = d.clone();
            d.kind = match d.kind {
                DimensionKind::Size {
                    feature,
                    kind,
                    path,
                    angle,
                } => DimensionKind::Size {
                    feature: f(feature),
                    kind,
                    path: path.map(f),
                    angle,
                },
                DimensionKind::Location {
                    from,
                    to,
                    kind,
                    path,
                    directed,
                    angle,
                } => DimensionKind::Location {
                    from: f(from),
                    to: f(to),
                    kind,
                    path: path.map(f),
                    directed,
                    angle,
                },
            };
            d
        })
        .collect();
    let tolerances = p
        .tolerances
        .iter()
        .enumerate()
        .filter(|(i, _)| !gone_g[*i])
        .map(|(_, t)| {
            let mut t = t.clone();
            t.target = match t.target {
                ToleranceTarget::Feature(x) => ToleranceTarget::Feature(f(x)),
                ToleranceTarget::Dimension(x) => {
                    ToleranceTarget::Dimension(DimensionId(ni[x.0].unwrap()))
                }
                ToleranceTarget::Relation {
                    relating,
                    related,
                    name,
                } => ToleranceTarget::Relation {
                    relating: f(relating),
                    related: f(related),
                    name,
                },
                ToleranceTarget::WholePart => ToleranceTarget::WholePart,
            };
            if let Some(s) = &t.datums {
                t.datums = Some(
                    DatumSystem::new(
                        s.compartments()
                            .iter()
                            .map(|c| Compartment {
                                references: c
                                    .references
                                    .iter()
                                    .map(|r| DatumReference {
                                        datum: DatumId(di[r.datum.0].unwrap()),
                                        modifiers: r.modifiers.clone(),
                                    })
                                    .collect(),
                                modifiers: c.modifiers.clone(),
                            })
                            .collect(),
                    )
                    .unwrap(),
                );
            }
            if let Some(z) = &mut t.zone {
                if let Some(pz) = &mut z.projected {
                    pz.end = pz.end.map(f);
                }
                if let Some(nu) = &mut z.non_uniform {
                    *nu = nu.iter().map(|x| f(*x)).collect();
                }
                z.affected_plane = z.affected_plane.map(f);
            }
            t
        })
        .collect();
    let tolerance_relations = p
        .tolerance_relations
        .iter()
        .enumerate()
        .filter(|(i, r)| {
            !has(ItemRef::ToleranceRelation(*i)) && !gone_g[r.relating.0] && !gone_g[r.related.0]
        })
        .map(|(_, r)| ToleranceRelation {
            kind: r.kind,
            relating: ToleranceId(gi[r.relating.0].unwrap()),
            related: ToleranceId(gi[r.related.0].unwrap()),
        })
        .collect();
    PartPmi {
        standards: p
            .standards
            .iter()
            .enumerate()
            .filter(|(i, _)| !has(ItemRef::Standard(*i)))
            .map(|(_, s)| s.clone())
            .collect(),
        decimal_places: p.decimal_places.filter(|_| !has(ItemRef::DecimalPlaces)),
        features,
        datum_targets,
        datums,
        dimensions,
        tolerances,
        tolerance_relations,
        general: p
            .general
            .iter()
            .enumerate()
            .filter(|(i, _)| !has(ItemRef::General(*i)))
            .map(|(_, g)| g.clone())
            .collect(),
        threads: p
            .threads
            .iter()
            .enumerate()
            .filter(|(i, t)| {
                !has(ItemRef::Thread(*i))
                    && f_ok(t.feature)
                    && t.partial_area.is_none_or(f_ok)
                    && t.runout.is_none_or(f_ok)
            })
            .map(|(_, t)| {
                let mut t = t.clone();
                t.feature = f(t.feature);
                t.partial_area = t.partial_area.map(f);
                t.runout = t.runout.map(f);
                t
            })
            .collect(),
        knurls: p
            .knurls
            .iter()
            .enumerate()
            .filter(|(i, k)| !has(ItemRef::Knurl(*i)) && f_ok(k.feature))
            .map(|(_, k)| {
                let mut k = k.clone();
                k.feature = f(k.feature);
                k
            })
            .collect(),
        material: p.material.clone().filter(|_| !has(ItemRef::Material)),
        notes: p
            .notes
            .iter()
            .enumerate()
            .filter(|(i, n)| !has(ItemRef::Note(*i)) && owner_ok(&n.on))
            .map(|(_, n)| Note {
                kind: n.kind.clone(),
                text: n.text.clone(),
                on: owner(&n.on),
            })
            .collect(),
        attributes: p
            .attributes
            .iter()
            .enumerate()
            .filter(|(i, a)| !has(ItemRef::Attribute(*i)) && owner_ok(&a.on))
            .map(|(_, a)| AttributeSet {
                name: a.name.clone(),
                on: owner(&a.on),
                items: a.items.clone(),
            })
            .collect(),
        geometry: p.geometry.clone(),
    }
}

/// `(kind of item, reason)` of a refusal, without the item's index: the pin key.
fn refusal_key(item: ItemRef, why: &str) -> String {
    let kind = format!("{item:?}");
    let kind = kind.split('(').next().unwrap_or("").to_string();
    // Numbers in reasons (counts, face numbers) and datum labels vary per item: keep the
    // words.
    let why = match why.strip_prefix("datum ").and_then(|r| r.split_once(' ')) {
        Some((_, rest)) if kind == "Datum" => format!("datum … {rest}"),
        _ => why.to_string(),
    };
    let why: String = why
        .chars()
        .map(|c| if c.is_ascii_digit() { 'N' } else { c })
        .collect();
    format!("{kind}: {why}")
}

struct Outcome {
    /// Refused items by key, with counts.
    refused: BTreeMap<String, usize>,
    /// The PMI written per part (the read PMI without what was refused).
    written: Vec<PartPmi>,
    result: Result<(Vec<u8>, pmi::write::WriteReport), WriteError>,
}

/// Replace every part with its read PMI; refused items are removed (with what depends on
/// them) and the write repeated, until it is written or fails otherwise.
fn replace_all(
    doc: &Document,
    parts: &[PartDefinition],
    read: &PmiRead,
    policy: PresentationPolicy,
) -> Outcome {
    let mut written: Vec<PartPmi> = read.parts.clone();
    let mut refused = BTreeMap::new();
    for _ in 0..8 {
        let pmi: Vec<(PartId, PartPmi)> = written
            .iter()
            .enumerate()
            .map(|(i, p)| (PartId(i), p.clone()))
            .collect();
        match pmi::write(doc, parts, &pmi, Mode::Replace, policy) {
            Err(WriteError::Refused(list)) => {
                for r in &list {
                    *refused.entry(refusal_key(r.item, &r.why)).or_insert(0) += 1;
                }
                for (i, p) in written.iter_mut().enumerate() {
                    let items: Vec<ItemRef> = list
                        .iter()
                        .filter(|r| r.part == PartId(i))
                        .map(|r| r.item)
                        .collect();
                    if !items.is_empty() {
                        *p = strip(p, &items);
                    }
                }
            }
            Err(e) => {
                return Outcome {
                    refused,
                    written,
                    result: Err(e),
                };
            }
            Ok((edit, report)) => {
                let bytes = doc.apply(&edit).expect("the edit applies").bytes;
                return Outcome {
                    refused,
                    written,
                    result: Ok((bytes, report)),
                };
            }
        }
    }
    panic!("refusals did not settle");
}

/// NIST's MBE PMI test models: `HAECCEITY_NIST_PMI`, else the AP242 track's download.
fn nist_files() -> Vec<PathBuf> {
    let dir = std::env::var("HAECCEITY_NIST_PMI")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(
                "/private/tmp/claude-501/-Users-paul-repos-quiddity-rust/\
                 4f6aac0c-d5a0-4c82-bc67-2d2552947882/scratchpad/ap242-nist/x/NIST-PMI-STEP-Files",
            )
        });
    if !dir.is_dir() {
        assert!(
            std::env::var_os("HAECCEITY_NIST_PMI_REQUIRED").is_none(),
            "NIST PMI files required but {} is missing",
            dir.display()
        );
        eprintln!("NIST files skipped: {} is missing", dir.display());
        return Vec::new();
    }
    let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "stp"))
        .collect();
    out.sort();
    out
}

fn specify_files() -> Vec<PathBuf> {
    let dir = common::fixtures().join("ap242/specify");
    let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().ends_with(".step.gz"))
        .collect();
    out.sort();
    out.push(common::fixtures().join("ap242/assembly/assembly.step"));
    // The NIST models committed with the reader's fixtures (one of them, STC-09, in edition
    // 4, the target), so the round trip runs without the download too.
    let nist = common::fixtures().join("ap242/nist");
    let mut committed: Vec<PathBuf> = std::fs::read_dir(&nist)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().ends_with(".stp.gz"))
        .collect();
    committed.sort();
    out.extend(committed);
    out
}

fn stem(p: &Path) -> String {
    let n = p.file_name().unwrap().to_string_lossy().to_string();
    n.trim_end_matches(".gz")
        .trim_end_matches(".step")
        .trim_end_matches(".stp")
        .to_string()
}

/// The pin key of a write that failed other than by refusal.
fn error_key(e: &WriteError) -> String {
    match e {
        WriteError::Removal(r) => {
            let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
            for b in &r.blockers {
                *kinds
                    .entry(format!("{} {}", b.family.as_str(), b.entity))
                    .or_insert(0) += 1;
            }
            for e in &r.emptied {
                *kinds.entry(format!("emptied {}", e.attribute)).or_insert(0) += 1;
            }
            format!("removal refused: {kinds:?}")
        }
        WriteError::Edition { violations, .. } => {
            format!("edition: {} violations", violations.len())
        }
        e => format!("{e}"),
    }
}

fn load_pins() -> serde_json::Value {
    serde_json::from_str(
        &std::fs::read_to_string(common::fixtures().join("known_pmi_write.json")).unwrap(),
    )
    .unwrap()
}

/// Every file of `files`, read → replace (each part with what was read, refused items removed
/// with what depends on them) → read: the same PMI per part, by meaning (`differences`, which
/// compares every value's text and unit, so inch values keep their stated digits); no finding
/// about what was written; every other finding as before; no instance the reader consumed for
/// a replaced part survives (supplemental geometry is the part's and is kept). Refusals and
/// other failures are pinned per file with a reason (`known_pmi_write.json` "roundtrip").
fn roundtrip(
    files: &[PathBuf],
    problems: &mut Vec<String>,
    actual: &mut serde_json::Map<String, serde_json::Value>,
) {
    for path in files {
        let stem = stem(path);
        let bytes = file_bytes(path);
        let Ok(parts) = read_part_definitions(&bytes) else {
            actual.insert(stem, serde_json::json!({"error": "no part definitions"}));
            continue;
        };
        let doc = Document::parse(bytes).unwrap();
        let r0 = pmi::read(&doc, &parts).unwrap();
        let mut entry = serde_json::Map::new();
        let o = replace_all(&doc, &parts, &r0, PresentationPolicy::RemovePresentation);
        if !o.refused.is_empty() {
            entry.insert("refused".into(), serde_json::to_value(&o.refused).unwrap());
        }
        match &o.result {
            Err(e) => {
                entry.insert("error".into(), error_key(e).into());
            }
            Ok((bytes, report)) => {
                let parts1 = read_part_definitions(bytes).unwrap();
                let doc1 = Document::parse(bytes.clone()).unwrap();
                let r1 = pmi::read(&doc1, &parts1).unwrap();
                for (i, p) in o.written.iter().enumerate() {
                    for d in differences(p, &r1.parts[i]) {
                        problems.push(format!("{stem} part {i}: {d}"));
                    }
                }
                let max = doc.max_id();
                for f in &r1.findings {
                    if f.ids.iter().any(|&id| id > max) {
                        problems.push(format!("{stem}: a finding on what was written: {f:?}"));
                    } else if !r0.findings.contains(f) {
                        problems.push(format!("{stem}: a new finding: {f:?}"));
                    }
                }
                for (i, prov) in r0.provenance.parts.iter().enumerate() {
                    let geometry: std::collections::BTreeSet<u64> =
                        prov.geometry.iter().flatten().copied().collect();
                    for id in prov.all() {
                        if !geometry.contains(&id) && doc1.get(id).is_some() {
                            problems.push(format!("{stem} part {i}: consumed #{id} survives"));
                        }
                    }
                }
                assert_eq!(
                    doc1.file_schema().unwrap(),
                    doc.file_schema().unwrap(),
                    "{stem}: an AP242 file keeps its schema"
                );
                entry.insert(
                    "presentation removed".into(),
                    report.presentation_removed.len().into(),
                );
                // The same with policy Refuse: refused, naming the presentation, or written.
                let pmi: Vec<(PartId, PartPmi)> = o
                    .written
                    .iter()
                    .enumerate()
                    .map(|(i, p)| (PartId(i), p.clone()))
                    .collect();
                let refuse = match pmi::write(
                    &doc,
                    &parts,
                    &pmi,
                    Mode::Replace,
                    PresentationPolicy::Refuse,
                ) {
                    Ok(_) => "written".to_string(),
                    Err(WriteError::Removal(r)) => {
                        let presentation = r
                            .blockers
                            .iter()
                            .filter(|b| {
                                b.family != haecceity::express::Family::Other
                                    && b.family != haecceity::express::Family::SemanticPmi
                            })
                            .count();
                        format!(
                            "refused: {presentation} presentation blockers of {}",
                            r.blockers.len() + r.emptied.len()
                        )
                    }
                    Err(e) => error_key(&e),
                };
                entry.insert("refuse policy".into(), refuse.into());
            }
        }
        actual.insert(stem, serde_json::Value::Object(entry));
    }
}

fn compare_pins(
    section: &str,
    actual: &serde_json::Map<String, serde_json::Value>,
    problems: &mut Vec<String>,
) {
    let pins = load_pins();
    let reasons = pins["reasons"].as_object().unwrap();
    let pinned = pins[section].as_object().unwrap();
    for (stem, got) in actual {
        match pinned.get(stem) {
            Some(p) if p == got => {}
            Some(p) => problems.push(format!(
                "{section} {stem}: changed\n  pinned {p}\n  actual {got}"
            )),
            None => problems.push(format!("{section} {stem}: not pinned: {got}")),
        }
        let mut keys: Vec<String> = got
            .get("refused")
            .and_then(|r| r.as_object())
            .map(|r| r.keys().cloned().collect())
            .unwrap_or_default();
        if let Some(e) = got.get("error").and_then(|e| e.as_str()) {
            keys.push(e.split(':').next().unwrap_or("").to_string());
        }
        for k in keys {
            if !reasons
                .get(&k)
                .and_then(|r| r.as_str())
                .is_some_and(|r| !r.is_empty())
            {
                problems.push(format!("{section} {stem}: no reason for {k:?}"));
            }
        }
    }
    let _ = std::fs::write(
        std::env::temp_dir().join(format!(
            "pmi_write_{section}_{}.json",
            actual.keys().next().map_or("none", String::as_str)
        )),
        serde_json::to_string_pretty(&serde_json::Value::Object(actual.clone())).unwrap(),
    );
}

#[test]
fn read_replace_read_keeps_specify_inputs_and_committed_nist_files() {
    let mut problems = Vec::new();
    let mut actual = serde_json::Map::new();
    roundtrip(&specify_files(), &mut problems, &mut actual);
    compare_pins("roundtrip", &actual, &mut problems);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn read_replace_read_keeps_nist_files() {
    let mut problems = Vec::new();
    let mut actual = serde_json::Map::new();
    roundtrip(&nist_files(), &mut problems, &mut actual);
    compare_pins("roundtrip", &actual, &mut problems);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
