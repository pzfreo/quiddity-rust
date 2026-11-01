//! Bounded descriptive text. ISO 10303-43's compound/list representation items
//! supply ordering; the child name identifies our concatenation convention.
//! CAx-IF UDA 1.8 §7 permits several representation items when their interpretation
//! is clear, but specifies no string size limit. 8192 encoded bytes is our
//! interoperability limit, not an EXPRESS or CAx-IF restriction.
use super::PartProvenance;
use crate::p21::{self, Attribute as A, Document, P21Error, RawEntity};

pub(crate) const MAX_LITERAL_BYTES: usize = 8192;
pub(crate) const CHUNK_NAME: &str = "haecceity text chunk/1";

pub(crate) fn literal_len(text: &str) -> usize {
    let escaped = p21::escape(text);
    escaped.len() + escaped.bytes().filter(|&c| c == b'\'').count() + 2
}

/// Split decoded Unicode, never encoded escapes. Budget the worst-case encoding
/// of each scalar, including the escape run delimiters (X4 may widen a BMP run).
pub(crate) fn split(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut start, mut size) = (0, 2);
    for (i, c) in text.char_indices() {
        let cost = match c {
            '\'' | '\\' => 2,
            ' '..='~' => 1,
            _ => 16,
        };
        if size + cost > MAX_LITERAL_BYTES {
            out.push(&text[start..i]);
            start = i;
            size = 2;
        }
        size += cost;
    }
    out.push(&text[start..]);
    out
}

fn simple<'a>(doc: &'a Document, id: u64, entity: &str) -> Option<&'a [A]> {
    match doc.get(id)? {
        RawEntity::Simple {
            name, attributes, ..
        } if name.eq_ignore_ascii_case(entity) => Some(attributes),
        _ => None,
    }
}

pub(crate) fn chunks(doc: &Document, id: u64) -> Option<Vec<u64>> {
    let a = simple(doc, id, "compound_representation_item")?;
    let A::Typed { type_name, value } = a.get(1)? else {
        return None;
    };
    if !type_name.eq_ignore_ascii_case("list_representation_item") {
        return None;
    }
    let A::List(items) = value.as_ref() else {
        return None;
    };
    if items.is_empty() {
        return None;
    }
    items
        .iter()
        .map(|item| {
            let A::EntityRef(id) = item else { return None };
            let a = simple(doc, *id, "descriptive_representation_item")?;
            match (a.first(), a.get(1)) {
                (Some(A::String(name)), Some(A::String(_))) if name == CHUNK_NAME => Some(*id),
                _ => None,
            }
        })
        .collect()
}

/// Both legacy single descriptive items and the bounded ordered form.
pub(crate) fn read(doc: &Document, id: u64) -> Option<String> {
    if let Some(a) = simple(doc, id, "descriptive_representation_item") {
        return match a.get(1)? {
            A::String(s) => Some(s.clone()),
            _ => None,
        };
    }
    if let Some(RawEntity::Complex { parts, .. }) = doc.get(id)
        && let Some(part) = parts.iter().find(|p| {
            p.name
                .eq_ignore_ascii_case("descriptive_representation_item")
        })
        && let Some(A::String(text)) = part.attributes.first()
    {
        return Some(text.clone());
    }
    let mut text = String::new();
    for id in chunks(doc, id)? {
        let a = simple(doc, id, "descriptive_representation_item")?;
        let A::String(s) = a.get(1)? else { return None };
        text.push_str(s);
    }
    Some(text)
}

/// A replaced item owns its chunks too, so repeated writes neither leak records
/// nor leave dangling references. Shared unrelated compound items are untouched.
pub(crate) fn extend_provenance(doc: &Document, p: &mut PartProvenance) {
    let extend = |ids: &mut Vec<u64>| {
        let children: Vec<_> = ids
            .iter()
            .filter_map(|&id| chunks(doc, id))
            .flatten()
            .collect();
        if children.is_empty() {
            return;
        }
        ids.extend(children);
        ids.sort_unstable();
        ids.dedup();
    };
    for collection in [
        &mut p.standards,
        &mut p.features,
        &mut p.datum_targets,
        &mut p.datums,
        &mut p.dimensions,
        &mut p.tolerances,
        &mut p.tolerance_relations,
        &mut p.general,
        &mut p.threads,
        &mut p.knurls,
        &mut p.notes,
        &mut p.surface_textures,
        &mut p.attributes,
        &mut p.geometry,
    ] {
        for ids in collection {
            extend(ids);
        }
    }
    extend(&mut p.decimal_places);
    extend(&mut p.material);
    #[cfg(feature = "json")]
    extend(&mut p.transport);
}

/// Backstop for every output path, including unchanged input and header strings.
/// An indivisible source name/identifier cannot be split without changing its
/// schema meaning: refuse explicitly rather than return unreadable output.
pub(crate) fn check_literals(bytes: &[u8]) -> Result<(), P21Error> {
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < bytes.len() && &bytes[i..i + 2] != b"*/" {
                    i += 1;
                }
                i += 2;
            }
            b'"' | b'<' => {
                // Binary literals and edition-3 URI/anchor tokens can contain
                // apostrophes, which do not start strings there.
                let end = if bytes[i] == b'<' { b'>' } else { b'"' };
                i += 1;
                while i < bytes.len() && bytes[i] != end {
                    i += 1;
                }
                i += 1;
            }
            b'\'' => {
                let start = i;
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\'' {
                        i += 1;
                        if bytes.get(i) != Some(&b'\'') {
                            break;
                        }
                    }
                    i += 1;
                }
                let size = i - start;
                if size > MAX_LITERAL_BYTES {
                    return Err(P21Error::Encode(format!(
                        "string literal at byte {start} is {size} encoded bytes (limit {MAX_LITERAL_BYTES}); an unchanged source string cannot be split losslessly in its schema attribute"
                    )));
                }
            }
            _ => i += 1,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_budget_quotes_backslashes_and_widened_unicode_runs() {
        for value in [
            "a".repeat(MAX_LITERAL_BYTES - 2),
            "a".repeat(MAX_LITERAL_BYTES - 1),
            "'\\".repeat(5000),
            "é".repeat(3000) + "😀",
            "a\né⌀😀'\\".repeat(2000),
        ] {
            let chunks = split(&value);
            assert_eq!(chunks.concat(), value);
            assert!(chunks.iter().all(|s| literal_len(s) <= MAX_LITERAL_BYTES));
            assert!(chunks.iter().all(|s| !s.is_empty()));
        }
        assert_eq!(
            literal_len(&"a".repeat(MAX_LITERAL_BYTES - 2)),
            MAX_LITERAL_BYTES
        );
    }

    #[test]
    fn literal_backstop_ignores_comments_and_binary_and_counts_doubled_quotes() {
        let long = "a".repeat(MAX_LITERAL_BYTES);
        assert!(
            check_literals(
                format!("/* '{long}' */ \"{long}\" <https://example.test/'{long}> 'ok''text'")
                    .as_bytes()
            )
            .is_ok()
        );
        assert!(check_literals(format!("'{long}'").as_bytes()).is_err());
        assert!(
            check_literals(format!("'{}'", "''".repeat(MAX_LITERAL_BYTES / 2)).as_bytes()).is_err()
        );
        assert!(
            check_literals(format!("'{}'", "a".repeat(MAX_LITERAL_BYTES - 2)).as_bytes()).is_ok()
        );
    }
}

#[cfg(test)]
mod read_tests {
    use super::*;

    #[test]
    fn list_order_not_instance_order_and_ordinary_compounds_are_not_text() {
        let doc = Document::parse(
            br"ISO-10303-21;
HEADER;
FILE_DESCRIPTION(('test'),'2;1');
FILE_NAME('test','',(''),(''),'','','');
FILE_SCHEMA(('T'));
ENDSEC;
DATA;
#1=COMPOUND_REPRESENTATION_ITEM('payload',LIST_REPRESENTATION_ITEM((#4,#3)));
#2=COMPOUND_REPRESENTATION_ITEM('ordinary',LIST_REPRESENTATION_ITEM((#5)));
#3=DESCRIPTIVE_REPRESENTATION_ITEM('haecceity text chunk/1','second');
#4=DESCRIPTIVE_REPRESENTATION_ITEM('haecceity text chunk/1','first');
#5=DESCRIPTIVE_REPRESENTATION_ITEM('unmarked','ordinary');
#6=(DESCRIPTIVE_REPRESENTATION_ITEM('complex')REPRESENTATION_ITEM('value'));
#7=COMPOUND_REPRESENTATION_ITEM('unordered',SET_REPRESENTATION_ITEM((#4,#3)));
ENDSEC;
END-ISO-10303-21;
"
            .to_vec(),
        )
        .unwrap();
        assert_eq!(read(&doc, 1).as_deref(), Some("firstsecond"));
        assert_eq!(read(&doc, 2), None);
        assert_eq!(read(&doc, 5).as_deref(), Some("ordinary"));
        assert_eq!(read(&doc, 6).as_deref(), Some("complex"));
        assert_eq!(read(&doc, 7), None);
    }
}
