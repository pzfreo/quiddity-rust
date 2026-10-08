//! `quiddity serve`: recognition and correspondence over JSON lines, one request per line on
//! the input and one response per line on the output, in order.
//!
//! Requests (every field required, no others):
//!
//! - `{"id": ID, "op": "recognise", "step": PATH}` → `{"id": ID, "result": R}`, `R` the JSON
//!   `quiddity <file.step>` writes ([`RecognitionOutput`]): each family's records, their
//!   `fingerprints` and the versioned recognition `document`.
//! - `{"id": ID, "op": "correspond", "old": REV, "new": REV}` → `{"id": ID, "result": C}`, `C`
//!   the correspondence `quiddity correspond` writes. A revision is a path, to a STEP file
//!   (recognised on the spot) or to a JSON file `quiddity <file.step>` wrote, or a recognise
//!   `result` (or bare fingerprints) inline.
//!
//! `ID` is a string or an integer, echoed as given. Paths are relative to the server's working
//! directory. A success carries `"warnings"` (after `result`) when a STEP file's geometry did
//! not all resolve: the CLI's stderr warning, which here has no stderr to go to.
//!
//! A failure is `{"id": ID, "error": {"code": CODE, "message": TEXT}}`, `ID` `null` when the
//! request has none to echo. Codes: `bad_json` (the line is not UTF-8 JSON), `bad_request`
//! (not an object, a repeated key, an `id` that is not a string or integer, a field of the wrong
//! type, an unknown field), `missing_field`, `unknown_op`, `unreadable` (a file cannot be
//! read), `broken_step` (the kernel refuses the STEP file: [`StepError`]'s parse, unsupported
//! and incomplete refusals), `bad_document` (a revision without valid fingerprints, or
//! fingerprints `correspond` refuses) and `internal` (a panic, caught per request; the server
//! keeps serving). Blank lines are skipped.
//!
//! Output is deterministic: the same request gives the same bytes.
//!
//! Python quiddity has no such service. Its nearest contract, the correspondence API
//! (`quiddity/correspondence_api.json`: opaque receipts issued from one evidence view and
//! resolved against another), has a different shape from this port's correspondence, which
//! matches two whole recognitions (`docs/correspondence.md`); the Rust correspondence and
//! recognition document are kept, as the CLI writes them. What is taken from Python's API is
//! its strictness: closed request shapes, unknown fields refused, failures explicit.

use std::io::{self, BufRead, Write};
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;

use serde::Serialize;
use serde_json::{Map, Value};

use crate::correspondence::{self, Fingerprints, Recognition};
use crate::kernel::brep::Part;
use crate::kernel::step::StepError;
use crate::recognition::{self, RecognitionDocument};

/// What `quiddity <file.step>` writes and `recognise` returns.
#[derive(Serialize)]
pub struct RecognitionOutput {
    #[serde(flatten)]
    pub recognition: Recognition,
    pub document: RecognitionDocument,
}

/// A refused request: its error code and message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Failure {
    pub code: &'static str,
    pub message: String,
}

impl Failure {
    fn new(code: &'static str, message: impl Into<String>) -> Failure {
        Failure {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// A STEP file's part, with a warning when geometry did not resolve: the solids holding it are
/// not valid, so their features are not recognised.
pub fn read_part(path: &str) -> Result<(Part, Option<String>), Failure> {
    let part = crate::kernel::step::read_step_file(Path::new(path)).map_err(|e| {
        let code = if e.is::<StepError>() {
            "broken_step"
        } else {
            "unreadable"
        };
        Failure::new(code, format!("{path}: {e}"))
    })?;
    let (faces, edges) = (part.unresolved_faces(), part.unresolved_edges());
    let warning = (!faces.is_empty() || !edges.is_empty()).then(|| {
        format!(
            "{path}: warning: geometry did not resolve (faces {faces:?}, edges {edges:?}); the \
             solids holding it are not recognised"
        )
    });
    Ok((part, warning))
}

/// Recognise the STEP file at *path*: the output and any warning.
pub fn recognise_file(path: &str) -> Result<(RecognitionOutput, Option<String>), Failure> {
    let (part, warning) = read_part(path)?;
    let recognition = correspondence::recognise(&part);
    let document = recognition::document(&recognition, part.faces.len());
    let output = RecognitionOutput {
        recognition,
        document,
    };
    Ok((output, warning))
}

/// Whether *path* names a STEP file (by its extension, as the CLI decides).
fn is_step(path: &str) -> bool {
    let lower = path.to_lowercase();
    [".step", ".stp", ".step.gz", ".stp.gz"]
        .iter()
        .any(|e| lower.ends_with(e))
}

/// A revision's fingerprints from JSON: a recognition result's `fingerprints`, or a bare
/// fingerprints document. *source* names it in the message.
pub fn fingerprints_value(mut value: Value, source: &str) -> Result<Fingerprints, Failure> {
    if let Some(inner) = value.get_mut("fingerprints") {
        value = inner.take();
    }
    serde_json::from_value(value).map_err(|e| {
        Failure::new(
            "bad_document",
            format!("{source}: no fingerprints ({e}); write it with `quiddity <file.step>`"),
        )
    })
}

/// A revision's fingerprints from the file at *path*: a STEP file recognised, or a JSON file
/// `quiddity <file.step>` wrote. With any warning.
pub fn fingerprints_file(path: &str) -> Result<(Fingerprints, Option<String>), Failure> {
    if is_step(path) {
        let (part, warning) = read_part(path)?;
        return Ok((correspondence::recognise(&part).fingerprints, warning));
    }
    let text = std::fs::read_to_string(path)
        .map_err(|e| Failure::new("unreadable", format!("{path}: {e}")))?;
    let value = serde_json::from_str(&text)
        .map_err(|e| Failure::new("bad_document", format!("{path}: not JSON ({e})")))?;
    Ok((fingerprints_value(value, path)?, None))
}

/// Match two revisions' fingerprints.
pub fn correspond(
    old: &Fingerprints,
    new: &Fingerprints,
) -> Result<correspondence::Correspondence, Failure> {
    correspondence::correspond(old, new).map_err(|e| Failure::new("bad_document", e))
}

/// A request's `id`, checked: a string or an integer.
fn request_id(request: &Map<String, Value>) -> Result<Value, Failure> {
    match request.get("id") {
        None => Err(Failure::new("missing_field", "the request has no `id`")),
        Some(id @ Value::String(_)) => Ok(id.clone()),
        Some(id @ Value::Number(n)) if n.is_i64() || n.is_u64() => Ok(id.clone()),
        Some(_) => Err(Failure::new(
            "bad_request",
            "`id` must be a string or an integer",
        )),
    }
}

/// *request*'s field *name*, which must be there.
fn field<'a>(request: &'a Map<String, Value>, name: &str) -> Result<&'a Value, Failure> {
    request
        .get(name)
        .ok_or_else(|| Failure::new("missing_field", format!("the request has no `{name}`")))
}

/// Refuse fields other than `id`, `op` and *fields*.
fn closed(request: &Map<String, Value>, op: &str, fields: &[&str]) -> Result<(), Failure> {
    for key in request.keys() {
        if key != "id" && key != "op" && !fields.contains(&key.as_str()) {
            return Err(Failure::new(
                "bad_request",
                format!(
                    "`{op}` takes no `{key}` (its fields: {})",
                    fields.join(", ")
                ),
            ));
        }
    }
    Ok(())
}

/// A path field's value.
fn path_field<'a>(request: &'a Map<String, Value>, name: &str) -> Result<&'a str, Failure> {
    field(request, name)?
        .as_str()
        .ok_or_else(|| Failure::new("bad_request", format!("`{name}` must be a path (a string)")))
}

/// A correspond revision: a path, or a recognise result (or bare fingerprints) inline.
fn revision(
    request: &Map<String, Value>,
    name: &str,
    warnings: &mut Vec<String>,
) -> Result<Fingerprints, Failure> {
    match field(request, name)? {
        Value::String(path) => {
            let (fingerprints, warning) = fingerprints_file(path)?;
            warnings.extend(warning);
            Ok(fingerprints)
        }
        value @ Value::Object(_) => fingerprints_value(value.clone(), &format!("`{name}`")),
        _ => Err(Failure::new(
            "bad_request",
            format!("`{name}` must be a path or a recognise result"),
        )),
    }
}

/// Carry out a request whose `id` checked: the result's JSON and any warnings.
fn execute(request: &Map<String, Value>) -> Result<(String, Vec<String>), Failure> {
    let op = field(request, "op")?
        .as_str()
        .ok_or_else(|| Failure::new("bad_request", "`op` must be a string"))?;
    let mut warnings = Vec::new();
    let json = match op {
        "recognise" => {
            closed(request, op, &["step"])?;
            let (output, warning) = recognise_file(path_field(request, "step")?)?;
            warnings.extend(warning);
            serde_json::to_string(&output).expect("records serialise")
        }
        "correspond" => {
            closed(request, op, &["old", "new"])?;
            let old = revision(request, "old", &mut warnings)?;
            let new = revision(request, "new", &mut warnings)?;
            serde_json::to_string(&correspond(&old, &new)?).expect("a correspondence serialises")
        }
        _ => {
            return Err(Failure::new(
                "unknown_op",
                format!("unknown op `{op}` (known: recognise, correspond)"),
            ));
        }
    };
    Ok((json, warnings))
}

/// Run *f*, a panic caught and turned into an `internal` failure.
fn guarded<T>(f: impl FnOnce() -> Result<T, Failure>) -> Result<T, Failure> {
    panic::catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|payload| {
        let what = payload
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "a panic without a message".into());
        Err(Failure::new("internal", format!("panicked: {what}")))
    })
}

/// A failure's response line, `id` first as in a success (a `Value` would sort it after
/// `error`).
fn error_line(id: &Value, failure: &Failure) -> String {
    let error = serde_json::to_string(failure).expect("a failure serialises");
    format!("{{\"id\":{id},\"error\":{error}}}")
}

/// The first key the JSON object *line* repeats at its top level, if any.
fn repeated_key(line: &str) -> Option<String> {
    struct Keys(Option<String>);
    impl<'de> serde::Deserialize<'de> for Keys {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            struct Visit;
            impl<'de> serde::de::Visitor<'de> for Visit {
                type Value = Keys;
                fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                    f.write_str("a JSON object")
                }
                fn visit_map<A: serde::de::MapAccess<'de>>(
                    self,
                    mut map: A,
                ) -> Result<Keys, A::Error> {
                    let mut seen = std::collections::BTreeSet::new();
                    let mut repeated = None;
                    while let Some(key) = map.next_key::<String>()? {
                        map.next_value::<serde::de::IgnoredAny>()?;
                        if !seen.insert(key.clone()) && repeated.is_none() {
                            repeated = Some(key);
                        }
                    }
                    Ok(Keys(repeated))
                }
            }
            d.deserialize_map(Visit)
        }
    }
    serde_json::from_str::<Keys>(line).ok().and_then(|k| k.0)
}

/// The response line (without its newline) to one request line, or `None` for a blank line.
pub fn respond(line: &str) -> Option<String> {
    if line.trim().is_empty() {
        return None;
    }
    let request: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            return Some(error_line(
                &Value::Null,
                &Failure::new("bad_json", format!("the request is not JSON ({e})")),
            ));
        }
    };
    let Value::Object(request) = request else {
        return Some(error_line(
            &Value::Null,
            &Failure::new("bad_request", "the request is not a JSON object"),
        ));
    };
    // `Value` keeps the last of a repeated key; the request's own keys must be unique.
    if let Some(key) = repeated_key(line) {
        return Some(error_line(
            &Value::Null,
            &Failure::new("bad_request", format!("the request repeats `{key}`")),
        ));
    }
    let id = match request_id(&request) {
        Ok(id) => id,
        Err(failure) => return Some(error_line(&Value::Null, &failure)),
    };
    Some(match guarded(|| execute(&request)) {
        Ok((result, warnings)) => {
            // The result is written as serialised, keeping its fields' order (a `Value` would
            // sort them).
            let mut out = format!("{{\"id\":{id},\"result\":{result}");
            if !warnings.is_empty() {
                out.push_str(",\"warnings\":");
                out.push_str(&Value::from(warnings).to_string());
            }
            out.push('}');
            out
        }
        Err(failure) => error_line(&id, &failure),
    })
}

/// Serve requests from *input* to *output* until the input ends, flushing after each response.
/// A line that is not UTF-8 is refused as `bad_json` and serving goes on; only an I/O error
/// stops it.
pub fn serve(mut input: impl BufRead, mut output: impl Write) -> io::Result<()> {
    let mut raw = Vec::new();
    loop {
        raw.clear();
        if input.read_until(b'\n', &mut raw)? == 0 {
            return Ok(());
        }
        // The line ending, `\n` or `\r\n`, as `BufRead::lines` strips it.
        if raw.last() == Some(&b'\n') {
            raw.pop();
            if raw.last() == Some(&b'\r') {
                raw.pop();
            }
        }
        let response = match std::str::from_utf8(&raw) {
            Ok(line) => respond(line),
            Err(e) => Some(error_line(
                &Value::Null,
                &Failure::new("bad_json", format!("the request is not UTF-8 ({e})")),
            )),
        };
        if let Some(response) = response {
            writeln!(output, "{response}")?;
            output.flush()?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_is_caught_as_an_internal_failure() {
        let caught = guarded::<()>(|| panic!("boom {}", 7));
        assert_eq!(caught, Err(Failure::new("internal", "panicked: boom 7")));
        let caught = guarded::<()>(|| panic!("static"));
        assert_eq!(caught, Err(Failure::new("internal", "panicked: static")));
        assert_eq!(guarded(|| Ok(3)), Ok(3));
    }
}
