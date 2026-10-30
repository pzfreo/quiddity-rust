//! Public-domain Hershey Futura Light lettering (A. V. Hershey / James Hurt; acknowledgements
//! and the original ASCII font data are in `crate::stroke_font`). ISO notation symbols use
//! the same explicit characteristic geometry as the shared control frames.
use super::gdt;
pub type P = [f64; 2];
#[derive(Clone, Debug)]
pub struct Laid {
    pub lines: Vec<Vec<P>>,
    pub min: P,
    pub max: P,
    pub missing: Vec<char>,
}
pub struct Font;

fn ring(c: P, r: f64) -> Vec<P> {
    (0..=32)
        .map(|i| {
            let t = (i as f64 * 360. / 32.).to_radians();
            [c[0] + r * t.cos(), c[1] + r * t.sin()]
        })
        .collect()
}
/// A glyph at capital height 21. Unsupported characters are explicit, never substituted.
pub fn glyph(c: char) -> Option<(f64, Vec<Vec<P>>)> {
    if let Some((advance, lines)) = crate::stroke_font::glyph(c) {
        return Some((
            advance,
            lines
                .into_iter()
                .map(|s| s.into_iter().map(|(x, y)| [x, y]).collect())
                .collect(),
        ));
    }
    let mut lines = Vec::new();
    match c {
        '⌀' | 'ø' | 'Ø' => {
            lines.push(ring([10.5, 10.5], 9.));
            lines.push(vec![[0., 0.], [21., 21.]]);
        }
        '±' => {
            lines.push(vec![[1., 14.], [20., 14.]]);
            lines.push(vec![[10.5, 5.], [10.5, 23.]]);
            lines.push(vec![[1., 0.], [20., 0.]]);
        }
        '°' => lines.push(ring([8., 19.], 4.)),
        'Ⓜ' | 'Ⓛ' | 'Ⓕ' | 'Ⓟ' => {
            let letter = match c {
                'Ⓜ' => 'M',
                'Ⓛ' => 'L',
                'Ⓕ' => 'F',
                _ => 'P',
            };
            let (advance, ink) = glyph(letter)?;
            lines.push(ring([10.5, 10.5], 10.5));
            lines.extend(ink.into_iter().map(|l| {
                l.into_iter()
                    .map(|p| {
                        [
                            10.5 + (p[0] - advance / 2.) * 0.65,
                            10.5 + (p[1] - 10.5) * 0.65,
                        ]
                    })
                    .collect()
            }));
        }
        '−' => lines.push(vec![[1., 10.5], [20., 10.5]]),
        '×' => {
            lines.push(vec![[2., 2.], [19., 19.]]);
            lines.push(vec![[2., 19.], [19., 2.]]);
        }
        _ => return gdt::glyph(c),
    }
    Some((24., lines))
}
impl Font {
    pub fn supports(&self, c: char) -> bool {
        c == '\n' || glyph(c).is_some()
    }
    /// Single-stroke ink centred on its bounds. Missing characters are recorded for validation.
    pub fn text(&self, s: &str, size: f64) -> Laid {
        let mut lines = Vec::new();
        let mut missing = Vec::new();
        let scale = size / 21.;
        for (row, text) in s.split('\n').enumerate() {
            let mut pen = 0.;
            for c in text.chars() {
                if let Some((advance, ink)) = glyph(c) {
                    lines.extend(ink.into_iter().map(|l| {
                        l.into_iter()
                            .map(|p| [(p[0] + pen) * scale, p[1] * scale - row as f64 * 1.6 * size])
                            .collect::<Vec<_>>()
                    }));
                    pen += advance;
                } else {
                    missing.push(c);
                }
            }
        }
        let mut min = [f64::INFINITY; 2];
        let mut max = [f64::NEG_INFINITY; 2];
        for p in lines.iter().flatten() {
            for k in 0..2 {
                min[k] = min[k].min(p[k]);
                max[k] = max[k].max(p[k]);
            }
        }
        if lines.is_empty() {
            min = [0.; 2];
            max = [0.; 2];
        }
        let centre = [(min[0] + max[0]) / 2., (min[1] + max[1]) / 2.];
        for p in lines.iter_mut().flatten() {
            for k in 0..2 {
                p[k] -= centre[k];
            }
        }
        for k in 0..2 {
            min[k] -= centre[k];
            max[k] -= centre[k];
        }
        Laid {
            lines,
            min,
            max,
            missing,
        }
    }
}
