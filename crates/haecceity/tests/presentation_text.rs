//! Hershey coverage replaces the retired IBM Plex outline oracle: the selected renderer
//! deliberately uses single strokes, so an outline-font area comparison no longer applies.
#![cfg(feature = "presentation")]
use haecceity::presentation::{font, text};
#[test]
fn every_notation_character_has_a_real_glyph() {
    let notation = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz .,+-()[]/⌀Øø⌖ⓂⓁⒻⓅ±°⏤⏥○⌭⌒⌓∠⟂∥◎⌯↗⌰SREe×−";
    for c in notation.chars() {
        assert!(font().supports(c), "missing {c}");
        let (_, ink) = text::glyph(c).unwrap();
        if c != ' ' {
            assert!(ink.iter().any(|l| l.len() > 1), "empty glyph {c}");
        }
    }
    assert!(font().text(notation, 3.).missing.is_empty());
    assert!(!font().supports('☃'));
    assert_eq!(font().text("☃", 3.).missing, ['☃']);
}
#[test]
fn ascii_strokes_keep_the_public_domain_font_metrics() {
    let (advance, a) = text::glyph('A').unwrap();
    assert_eq!(advance, 18.);
    assert_eq!(a.len(), 3);
    let small = font().text("A±0.1", 1.);
    let big = font().text("A±0.1", 2.);
    assert_eq!(small.lines.len(), big.lines.len());
    for (a, b) in small.lines.iter().flatten().zip(big.lines.iter().flatten()) {
        assert!((2. * a[0] - b[0]).abs() < 1e-12 && (2. * a[1] - b[1]).abs() < 1e-12);
    }
}
