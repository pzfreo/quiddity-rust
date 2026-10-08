//! `quiddity <file.step>`: recognise a STEP file's features and print them, with their
//! fingerprints, as JSON.
//!
//! `quiddity correspond <old> <new>`: match two revisions and print the correspondence as JSON.
//! Each revision is a recognition result written by `quiddity <file.step>`, or a STEP file
//! (recognised on the spot).
//!
//! `quiddity parts <file.step>`: the file's distinct parts (index, name, face and edge counts,
//! placements) and the binding (the STEP text's sha256 and the PMI reader version) that face
//! and edge numbers refer to.
//!
//! `quiddity pmi read <file.step> [--part N]`: the semantic PMI of every part (or part N) and
//! the reader's findings, in the versioned JSON form of `quiddity::pmi_json`.
//!
//! `quiddity pmi check <file.step> <pmi.json>`: decodes a PMI document and checks it against
//! the file (binding, part indices and names, face and edge numbers, the model's invariants);
//! prints the document in its canonical form, or refuses naming the JSON path.
//!
//! `quiddity pmi write <file.step> <pmi.json> -o <out.step> [--mode add|replace|remove]
//! [--presentation refuse|remove]`: writes the document's parts (all in one `pmi::write`) into
//! the file as AP242 semantic PMI — beside each part's PMI (`add`, the default), in place of it
//! (`replace`), or removing it (`remove`, each part's `pmi` empty) — keeping every other byte.
//! The document must be bound to this file and reader (`pmi read`, `pmi check`). Replace and
//! remove take the presentation of the PMI they remove with it (`--presentation remove`, the
//! default) or refuse naming it (`--presentation refuse`). The output is written to a temporary
//! file beside `out.step` (gzipped when its name ends in `.gz`), read back with `pmi::read` and
//! compared semantically with what was written (add: the part's PMI before plus exactly the
//! items written; replace and remove: exactly the items written, and nothing the reader consumed
//! for the part survives; every other part as before; no finding the input did not have), and
//! renamed to `out.step` only then; prints the write's report as JSON. On any refusal nothing is
//! created.
//!
//! Exit status: 0 with the JSON on stdout; 1 with the error on stderr when a file cannot be
//! read (an unreadable, truncated, incomplete or unsupported STEP file is refused, never
//! recognised as empty), a PMI document is refused, or a write is refused or does not verify;
//! 2 with the usage on stderr for bad arguments. `-h`/`--help` prints the usage on stdout and
//! exits 0.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use quiddity::correspondence::{self, Fingerprints};
use quiddity::kernel::p21;
use quiddity::kernel::pmi::write::{Mode, PresentationPolicy, differences_as_stated};
use quiddity::kernel::pmi::{PartId, PartPmi, PmiRead};
use quiddity::kernel::step::{PartDefinition, read_part_definitions};
use quiddity::pmi_json::{self, Binding};

const USAGE: &str = "usage: quiddity <file.step>
       quiddity correspond <old.json|old.step> <new.json|new.step>
       quiddity parts <file.step>
       quiddity pmi read <file.step> [--part N]
       quiddity pmi check <file.step> <pmi.json>
       quiddity pmi write <file.step> <pmi.json> -o <out.step> [--mode add|replace|remove]
                          [--presentation refuse|remove]";

const COMMANDS: [&str; 3] = ["correspond", "parts", "pmi"];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [flag] if flag == "-h" || flag == "--help" => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        [command, old, new] if command == "correspond" => correspond(old, new),
        [command, path] if command == "parts" => parts(path),
        [command, read, path] if command == "pmi" && read == "read" => pmi_read(path, None),
        [command, read, path, flag, n]
            if command == "pmi" && read == "read" && flag == "--part" =>
        {
            match n.parse::<usize>() {
                Ok(n) => pmi_read(path, Some(n)),
                Err(_) => {
                    eprintln!("quiddity: --part takes a part index, not {n:?}\n{USAGE}");
                    return ExitCode::from(2);
                }
            }
        }
        [command, check, path, json] if command == "pmi" && check == "check" => {
            pmi_check(path, json)
        }
        [command, write, rest @ ..] if command == "pmi" && write == "write" => {
            match WriteArgs::parse(rest) {
                Ok(w) => pmi_write(&w),
                Err(e) => {
                    eprintln!("quiddity: {e}\n{USAGE}");
                    return ExitCode::from(2);
                }
            }
        }
        [path] if !COMMANDS.contains(&path.as_str()) => recognise(path),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("quiddity: {error}");
            ExitCode::FAILURE
        }
    }
}

/// A STEP file's part, warning on stderr of geometry that did not resolve: the solids holding
/// it are not valid, so their features are not recognised.
fn read_part(path: &str) -> Result<quiddity::kernel::Part, String> {
    let part = quiddity::read_step_file(Path::new(path)).map_err(|e| format!("{path}: {e}"))?;
    let (faces, edges) = (part.unresolved_faces(), part.unresolved_edges());
    if !faces.is_empty() || !edges.is_empty() {
        eprintln!(
            "quiddity: {path}: warning: geometry did not resolve (faces {faces:?}, edges \
             {edges:?}); the solids holding it are not recognised"
        );
    }
    Ok(part)
}

fn recognise(path: &str) -> Result<String, String> {
    let part = read_part(path)?;
    let recognition = correspondence::recognise(&part);
    Ok(serde_json::to_string_pretty(&recognition).expect("records serialise"))
}

/// A revision's fingerprints: from a recognition result's `fingerprints` (or a bare fingerprints
/// document), or by recognising a STEP file.
fn fingerprints(path: &str) -> Result<Fingerprints, String> {
    let lower = path.to_lowercase();
    let step = [".step", ".stp", ".step.gz", ".stp.gz"]
        .iter()
        .any(|e| lower.ends_with(e));
    if step {
        let part = read_part(path)?;
        return Ok(correspondence::recognise(&part).fingerprints);
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let mut value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{path}: not JSON ({e})"))?;
    if let Some(inner) = value.get_mut("fingerprints") {
        value = inner.take();
    }
    serde_json::from_value(value)
        .map_err(|e| format!("{path}: no fingerprints ({e}); write it with `quiddity <file.step>`"))
}

fn correspond(old: &str, new: &str) -> Result<String, String> {
    let c = correspondence::correspond(&fingerprints(old)?, &fingerprints(new)?)?;
    Ok(serde_json::to_string_pretty(&c).expect("a correspondence serialises"))
}

/// Whether a file name says it is gzipped.
fn gzipped(path: &str) -> bool {
    path.to_lowercase().ends_with(".gz")
}

/// A STEP file's text, gunzipped when the name ends in `.gz`.
fn step_bytes(path: &str) -> Result<Vec<u8>, String> {
    let raw = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    if !gzipped(path) {
        return Ok(raw);
    }
    let mut out = Vec::new();
    std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(&raw[..]), &mut out)
        .map_err(|e| format!("{path}: not gzip ({e})"))?;
    Ok(out)
}

/// A STEP file's text and its distinct parts.
fn part_definitions(path: &str) -> Result<(Vec<u8>, Vec<PartDefinition>), String> {
    let bytes = step_bytes(path)?;
    let defs = read_part_definitions(&bytes).map_err(|e| format!("{path}: {e}"))?;
    Ok((bytes, defs))
}

fn parts(path: &str) -> Result<String, String> {
    let (bytes, defs) = part_definitions(path)?;
    let json = pmi_json::parts_json(&defs, &Binding::of(&bytes));
    Ok(serde_json::to_string_pretty(&json).expect("JSON serialises"))
}

fn pmi_read(path: &str, part: Option<usize>) -> Result<String, String> {
    let (bytes, defs) = part_definitions(path)?;
    if let Some(n) = part
        && n >= defs.len()
    {
        return Err(format!(
            "{path}: the file has {} parts (0 to {}); there is no part {n}",
            defs.len(),
            defs.len().saturating_sub(1)
        ));
    }
    let binding = Binding::of(&bytes);
    let doc = p21::Document::parse(bytes).map_err(|e| format!("{path}: {e}"))?;
    let read = quiddity::kernel::pmi::read(&doc, &defs).map_err(|e| format!("{path}: {e}"))?;
    let document = pmi_json::read_document(&read, &defs, binding, part);
    Ok(
        serde_json::to_string_pretty(&pmi_json::document_to_json(&document))
            .expect("JSON serialises"),
    )
}

fn pmi_check(path: &str, json: &str) -> Result<String, String> {
    let (bytes, defs) = part_definitions(path)?;
    let text = std::fs::read_to_string(json).map_err(|e| format!("{json}: {e}"))?;
    let value = pmi_json::parse(&text).map_err(|e| format!("{json}: not JSON ({e})"))?;
    let document = pmi_json::document_from_json(&value).map_err(|e| format!("{json}: {e}"))?;
    pmi_json::check_against(&document, &bytes, &defs).map_err(|e| format!("{json}: {e}"))?;
    Ok(
        serde_json::to_string_pretty(&pmi_json::document_to_json(&document))
            .expect("JSON serialises"),
    )
}

/// `pmi write`'s arguments.
struct WriteArgs {
    step: String,
    json: String,
    out: String,
    mode: Mode,
    policy: PresentationPolicy,
}

impl WriteArgs {
    /// From the arguments after `pmi write`; a usage error names what is wrong.
    fn parse(args: &[String]) -> Result<WriteArgs, String> {
        fn once<T>(slot: &mut Option<T>, flag: &str, v: T) -> Result<(), String> {
            if slot.replace(v).is_some() {
                return Err(format!("{flag} is given twice"));
            }
            Ok(())
        }
        let mut positional = Vec::new();
        let (mut out, mut mode, mut policy) = (None, None, None);
        let mut it = args.iter();
        while let Some(a) = it.next() {
            let mut value = || {
                it.next()
                    .cloned()
                    .ok_or_else(|| format!("{a} takes a value"))
            };
            match a.as_str() {
                "-o" | "--output" => once(&mut out, "-o", value()?)?,
                "--mode" => {
                    let v = value()?;
                    let m = match v.as_str() {
                        "add" => Mode::Add,
                        "replace" => Mode::Replace,
                        "remove" => Mode::Remove,
                        _ => return Err(format!("--mode takes add, replace or remove, not {v:?}")),
                    };
                    once(&mut mode, a, m)?;
                }
                "--presentation" => {
                    let v = value()?;
                    let p = match v.as_str() {
                        "refuse" => PresentationPolicy::Refuse,
                        "remove" => PresentationPolicy::RemovePresentation,
                        _ => {
                            return Err(format!(
                                "--presentation takes refuse or remove, not {v:?}"
                            ));
                        }
                    };
                    once(&mut policy, a, p)?;
                }
                s if s.starts_with('-') => return Err(format!("unknown option {s:?}")),
                _ => positional.push(a.clone()),
            }
        }
        let [step, json] = <[String; 2]>::try_from(positional)
            .map_err(|_| "pmi write takes a STEP file and a PMI document".to_string())?;
        let out = out.ok_or("pmi write needs -o <out.step>")?;
        let mode = mode.unwrap_or(Mode::Add);
        if mode == Mode::Add && policy.is_some() {
            return Err("--presentation applies to replace and remove, not add".into());
        }
        Ok(WriteArgs {
            step,
            json,
            out,
            mode,
            policy: policy.unwrap_or(PresentationPolicy::RemovePresentation),
        })
    }
}

fn pmi_write(w: &WriteArgs) -> Result<String, String> {
    let (bytes, defs) = part_definitions(&w.step)?;
    let json = &w.json;
    let text = std::fs::read_to_string(json).map_err(|e| format!("{json}: {e}"))?;
    let value = pmi_json::parse(&text).map_err(|e| format!("{json}: not JSON ({e})"))?;
    let document = pmi_json::document_from_json(&value).map_err(|e| format!("{json}: {e}"))?;
    pmi_json::check_against(&document, &bytes, &defs).map_err(|e| format!("{json}: {e}"))?;
    if document.parts.is_empty() {
        return Err(format!("{json}: names no part; there is nothing to write"));
    }
    let input = Binding::of(&bytes);
    let step = &w.step;
    let doc = p21::Document::parse(bytes).map_err(|e| format!("{step}: {e}"))?;
    let before = quiddity::kernel::pmi::read(&doc, &defs).map_err(|e| format!("{step}: {e}"))?;
    // A standard is a set member, not an item: adding one the part already states (or stating
    // one twice) is that standard, so it is not written again.
    let pmi: Vec<(PartId, PartPmi)> = document
        .parts
        .iter()
        .map(|p| {
            let mut pmi = p.pmi.clone();
            if w.mode == Mode::Add {
                let mut stated = before
                    .parts
                    .get(p.part.0)
                    .map(|old| old.standards.clone())
                    .unwrap_or_default();
                pmi.standards.retain(|s| {
                    let new = !stated.contains(s);
                    if new {
                        stated.push(s.clone());
                    }
                    new
                });
            }
            (p.part, pmi)
        })
        .collect();
    let (edit, report) = quiddity::kernel::pmi::write(&doc, &defs, &pmi, w.mode, w.policy)
        .map_err(|e| format!("{step}: write refused: {e}"))?;
    let written = doc
        .apply(&edit)
        .map_err(|e| format!("{step}: write refused: {e}"))?;

    // Written beside the destination, read back from there, and renamed only when it verifies.
    let out = &w.out;
    // An existing destination is replaced in place: through a symbolic link to its target, and
    // keeping its permissions.
    let dest = Path::new(out);
    let target: PathBuf = match std::fs::symlink_metadata(dest) {
        Ok(m) if m.file_type().is_symlink() => {
            std::fs::canonicalize(dest).map_err(|e| format!("{out}: {e}"))?
        }
        _ => dest.to_path_buf(),
    };
    let name = target
        .file_name()
        .ok_or_else(|| format!("{out}: not a file name"))?
        .to_string_lossy();
    let tmp: PathBuf = target.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    let verified = (|| -> Result<(Binding, Vec<quiddity::kernel::pmi::Finding>), String> {
        let gz = gzipped(out);
        let file_bytes = if gz {
            let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            std::io::Write::write_all(&mut enc, &written.bytes).map_err(|e| e.to_string())?;
            enc.finish().map_err(|e| e.to_string())?
        } else {
            written.bytes.clone()
        };
        std::fs::write(&tmp, &file_bytes).map_err(|e| format!("{out}: {e}"))?;
        let raw = std::fs::read(&tmp).map_err(|e| format!("{out}: {e}"))?;
        let back = if gz {
            let mut v = Vec::new();
            std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(&raw[..]), &mut v)
                .map_err(|e| format!("{out}: {e}"))?;
            v
        } else {
            raw
        };
        if back != written.bytes {
            return Err("the file on disk is not what was written".into());
        }
        let defs1 = read_part_definitions(&back).map_err(|e| e.to_string())?;
        let doc1 = p21::Document::parse(back).map_err(|e| e.to_string())?;
        let after = quiddity::kernel::pmi::read(&doc1, &defs1).map_err(|e| e.to_string())?;
        verify(&before, &defs, &pmi, w.mode, &doc1, &defs1, &after)?;
        Ok((Binding::of(doc1.bytes()), after.findings))
    })();
    let (output, findings) = match verified {
        Ok(v) => v,
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            return Err(format!(
                "{out}: not written: the output does not verify: {e}"
            ));
        }
    };
    if let Ok(m) = std::fs::metadata(&target)
        && let Err(e) = std::fs::set_permissions(&tmp, m.permissions())
    {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("{out}: {e}"));
    }
    if let Err(e) = std::fs::rename(&tmp, &target) {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("{out}: {e}"));
    }
    let mut parts: Vec<PartId> = pmi.iter().map(|(p, _)| *p).collect();
    parts.sort();
    let json = pmi_json::write_report_json(
        w.mode, w.policy, &parts, &input, &output, &report, &findings,
    );
    Ok(serde_json::to_string_pretty(&json).expect("JSON serialises"))
}

/// The item keys of a part's PMI as `differences_as_stated` names them ("tolerance only in the
/// second: …"): every reference resolved, values as stated.
fn item_keys(p: &PartPmi) -> Vec<String> {
    differences_as_stated(&PartPmi::default(), p)
}

/// Checks the output read back (`after`) against the input read (`before`) and what was
/// written: each written part reads back as written (replace, remove) or as its PMI before plus
/// exactly the items written (add; a feature or datum equal to one the part has is that one;
/// standards already stated are not written),
/// and for replace and remove nothing the reader consumed for it survives (its supplemental
/// geometry excepted, which is the part's); every other part reads as before; the parts and
/// their faces and edges are unchanged; and no finding is new.
fn verify(
    before: &PmiRead,
    defs: &[PartDefinition],
    written: &[(PartId, PartPmi)],
    mode: Mode,
    after_doc: &p21::Document,
    after_defs: &[PartDefinition],
    after: &PmiRead,
) -> Result<(), String> {
    let mut problems: Vec<String> = Vec::new();
    if after_defs.len() != defs.len() {
        return Err(format!(
            "{} parts read back, not {}",
            after_defs.len(),
            defs.len()
        ));
    }
    for (i, (d0, d1)) in defs.iter().zip(after_defs).enumerate() {
        if d0.name != d1.name
            || d0.faces.len() != d1.faces.len()
            || d0.edges.len() != d1.edges.len()
        {
            problems.push(format!("part {i}'s name, faces or edges changed"));
        }
    }
    for i in 0..defs.len() {
        let old = &before.parts[i];
        let read = &after.parts[i];
        match written.iter().find(|(p, _)| p.0 == i) {
            None => problems.extend(
                differences_as_stated(old, read)
                    .into_iter()
                    .map(|d| format!("part {i} (not written): {d}")),
            ),
            Some((_, new)) if mode == Mode::Add => {
                let shared: std::collections::BTreeSet<String> = item_keys(old)
                    .into_iter()
                    .filter(|k| {
                        k.starts_with("feature only in the second: ")
                            || k.starts_with("datum only in the second: ")
                    })
                    .collect();
                let mut expected: Vec<String> = item_keys(new)
                    .into_iter()
                    .filter(|k| !shared.contains(k))
                    .collect();
                for d in differences_as_stated(old, read) {
                    if !d.contains(" only in the second: ") {
                        problems.push(format!("part {i}: lost: {d}"));
                    } else if let Some(at) = expected.iter().position(|k| *k == d) {
                        expected.remove(at);
                    } else {
                        problems.push(format!("part {i}: not written: {d}"));
                    }
                }
                problems.extend(
                    expected
                        .into_iter()
                        .map(|k| format!("part {i}: written but not read back: {k}")),
                );
            }
            Some((_, new)) => {
                problems.extend(
                    differences_as_stated(new, read)
                        .into_iter()
                        .map(|d| format!("part {i}: {d}")),
                );
                let prov = &before.provenance.parts[i];
                let geometry: std::collections::BTreeSet<u64> =
                    prov.geometry.iter().flatten().copied().collect();
                for id in prov.all() {
                    if !geometry.contains(&id) && after_doc.get(id).is_some() {
                        problems.push(format!(
                            "part {i}: #{id}, which the reader consumed, survives"
                        ));
                    }
                }
            }
        }
    }
    for f in &after.findings {
        if !before.findings.contains(f) {
            problems.push(format!(
                "a finding the input did not have: {} {:?} {}: {}",
                f.kind.as_str(),
                f.ids,
                f.entity,
                f.detail
            ));
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("; "))
    }
}
