//! One owner of requirement notation and callout geometry in a local 2D frame.
//!
//! Ported from pzfreo/draftwright-rust at aaa7a73: `src/dims/tolerance.rs`,
//! `src/dims/gdt.rs`, and `src/text.rs` (Paul Fremantle; port authorized by the copyright owner for #137, same project family).
//! Sheet layout, leaders, lanes, authored wording and drawn-precision rounding stay in
//! Draftwright. Values here retain requirement precision. IBM Plex outlines are OFL licensed
//! (see `fonts/`); characteristic symbols are explicit strokes, independent of font coverage.

use crate::pmi::*;

mod gdt;
pub mod text;
pub mod tolerance;

/// Text centred on its ink bounds, at a local point, with size in drawing units.
#[derive(Clone, Debug)]
pub struct Text {
    pub text: String,
    pub at: [f64; 2],
    pub size: f64,
}

/// Geometry and readable cells of one annotation, before placement in a model or sheet.
#[derive(Default, Clone, Debug)]
pub struct Callout {
    pub strokes: Vec<[[f64; 2]; 2]>,
    /// Centre x/y, radius, start/end angle in degrees.
    pub arcs: Vec<[f64; 5]>,
    pub letters: Vec<Text>,
    /// Text layer including symbols drawn geometrically rather than by the font.
    pub runs: Vec<Text>,
    pub cells: Vec<String>,
    pub size: [f64; 2],
}

pub fn font() -> &'static text::Font {
    static FONT: std::sync::OnceLock<text::Font> = std::sync::OnceLock::new();
    FONT.get_or_init(|| {
        text::Font::new(include_bytes!("../../fonts/IBMPlexMono-Regular.ttf").to_vec())
            .expect("bundled IBM Plex Mono parses")
    })
}

impl Callout {
    /// Outline-only tessellation: closed glyph contours, frame strokes, symbol arcs.
    /// No text fill surfaces are needed. Curves use 32 chords per Bézier / 72 per circle.
    pub fn polylines(&self) -> Vec<Vec<[f64; 2]>> {
        let mut lines: Vec<_> = self.strokes.iter().map(|s| s.to_vec()).collect();
        for &[x, y, r, a, b] in &self.arcs {
            lines.push(
                (0..=72)
                    .map(|i| {
                        let t = (a + (b - a) * i as f64 / 72.0).to_radians();
                        [x + r * t.cos(), y + r * t.sin()]
                    })
                    .collect(),
            );
        }
        for t in &self.letters {
            // Plex uses ø for the diameter glyph, as Draftwright's spherical zone does.
            let ink = t.text.replace('⌀', "ø");
            for c in font().text(&ink, t.size).contours {
                let mut pts = Vec::new();
                for s in c {
                    let n = if matches!(s, text::Segment::Line(..)) {
                        1
                    } else {
                        32
                    };
                    if pts.is_empty() {
                        pts.push(s.at(0.0));
                    }
                    pts.extend((1..=n).map(|i| s.at(i as f64 / n as f64)));
                }
                lines.push(
                    pts.into_iter()
                        .map(|p| [p[0] + t.at[0], p[1] + t.at[1]])
                        .collect(),
                );
            }
        }
        lines
    }
}

fn decimal(v: &Value) -> String {
    v.decimal().as_str().to_string()
}

// Convert only when the requirement actually uses different units, never to sheet precision.
fn in_units(v: &Value, nominal: &Value) -> String {
    if v.quantity.cmp_same_unit(&nominal.quantity).is_some() {
        decimal(v)
    } else {
        let scale = match &nominal.quantity {
            Quantity::Length(l) => l.unit.millimetres(),
            Quantity::Angle(a) => a.unit.radians(),
        };
        if scale.is_finite() && scale != 0.0 {
            (v.si() / scale).to_string()
        } else {
            decimal(v)
        }
    }
}

/// Text and CAx-IF annotation type for a dimension. Limits remain limits; a fit remains its
/// ISO 286 class. Zero deviations have no sign (ISO 129-1).
pub fn dimension_text(d: &Dimension) -> String {
    let (prefix, angular) = match &d.kind {
        DimensionKind::Size { kind, angle, .. } => (
            match kind {
                SizeKind::Diameter
                | SizeKind::ToroidalMinorDiameter
                | SizeKind::ToroidalMajorDiameter
                | SizeKind::ToroidalHighMajorDiameter
                | SizeKind::ToroidalLowMajorDiameter => "⌀",
                SizeKind::SphericalDiameter => "S⌀",
                SizeKind::Radius
                | SizeKind::ToroidalMinorRadius
                | SizeKind::ToroidalMajorRadius
                | SizeKind::ToroidalHighMajorRadius
                | SizeKind::ToroidalLowMajorRadius => "R",
                SizeKind::SphericalRadius => "SR",
                _ => "",
            },
            angle.is_some(),
        ),
        DimensionKind::Location { angle, .. } => ("", angle.is_some()),
    };
    let suffix = if angular
        && d.nominal.as_ref().is_some_and(
            |n| matches!(&n.quantity, Quantity::Angle(a) if a.unit == AngleUnit::Degree),
        ) {
        "°"
    } else {
        ""
    };
    let mut text = format!(
        "{prefix}{}{suffix}",
        d.nominal.as_ref().map_or(String::new(), decimal)
    );
    match &d.tolerance {
        DimTolerance::Deviations(b) => {
            let value = |v: &Value| {
                d.nominal
                    .as_ref()
                    .map_or_else(|| decimal(v), |n| in_units(v, n))
            };
            let u = value(b.upper());
            let l = value(b.lower());
            let band = tolerance::band_text(&u, &l);
            text.push_str(&format!(" {band}"));
        }
        DimTolerance::Limits(b) => {
            text = format!(
                "{prefix}{}{suffix} - {prefix}{}{suffix}",
                decimal(b.lower()),
                decimal(b.upper())
            );
        }
        DimTolerance::Fit { class, .. } => text.push_str(&format!(" {class}")),
        _ => {}
    }
    if d.modifiers.contains(&DimensionModifier::Reference) {
        text = format!("({text})");
    }
    text
}

pub fn dimension_type(d: &Dimension) -> &'static str {
    match &d.kind {
        DimensionKind::Size { angle: Some(_), .. }
        | DimensionKind::Location { angle: Some(_), .. } => "angular dimension",
        DimensionKind::Size {
            kind:
                SizeKind::Diameter
                | SizeKind::SphericalDiameter
                | SizeKind::ToroidalMinorDiameter
                | SizeKind::ToroidalMajorDiameter
                | SizeKind::ToroidalHighMajorDiameter
                | SizeKind::ToroidalLowMajorDiameter,
            ..
        } => "diameter dimension",
        DimensionKind::Size {
            kind:
                SizeKind::Radius
                | SizeKind::SphericalRadius
                | SizeKind::ToroidalMinorRadius
                | SizeKind::ToroidalMajorRadius
                | SizeKind::ToroidalHighMajorRadius
                | SizeKind::ToroidalLowMajorRadius,
            ..
        } => "radial dimension",
        _ => "linear dimension",
    }
}

pub fn dimension(d: &Dimension) -> Callout {
    let value = dimension_text(d);
    let mut c = label(&value);
    if d.tolerance == DimTolerance::Basic {
        let [w, h] = c.size;
        c.strokes.extend([
            [[0., 0.], [w, 0.]],
            [[w, 0.], [w, h]],
            [[w, h], [0., h]],
            [[0., h], [0., 0.]],
        ]);
    }
    c
}

pub fn label(value: &str) -> Callout {
    let ink = font().text(&value.replace('⌀', "ø"), 3.0);
    let size = [ink.max[0] - ink.min[0] + 3.6, 6.0];
    let text = Text {
        text: value.into(),
        at: [size[0] / 2.0, 3.0],
        size: 3.0,
    };
    Callout {
        letters: vec![text.clone()],
        runs: vec![text],
        cells: vec![value.into()],
        size,
        ..Default::default()
    }
}

pub fn tolerance_type(t: &GeometricTolerance) -> &'static str {
    match t.kind {
        ToleranceKind::Roundness => "circularity",
        ToleranceKind::LineProfile => "profile of line",
        ToleranceKind::SurfaceProfile => "profile of surface",
        ToleranceKind::CircularRunout => "circular runout",
        ToleranceKind::TotalRunout => "total runout",
        ToleranceKind::Angularity => "angularity",
        ToleranceKind::Coaxiality => "coaxiality",
        ToleranceKind::Concentricity => "concentricity",
        ToleranceKind::Cylindricity => "cylindricity",
        ToleranceKind::Flatness => "flatness",
        ToleranceKind::Parallelism => "parallelism",
        ToleranceKind::Perpendicularity => "perpendicularity",
        ToleranceKind::Position => "position",
        ToleranceKind::Straightness => "straightness",
        ToleranceKind::Symmetry => "symmetry",
    }
}

pub fn tolerance(t: &GeometricTolerance, p: &PartPmi) -> Callout {
    let name = match t.kind {
        ToleranceKind::LineProfile => "profile_line",
        ToleranceKind::SurfaceProfile => "profile_surface",
        ToleranceKind::CircularRunout => "circular_runout",
        ToleranceKind::TotalRunout => "total_runout",
        _ => tolerance_type(t),
    };
    let zone = t.zone.as_ref().and_then(|z| match z.form {
        ZoneForm::WithinACircle | ZoneForm::WithinACylinder | ZoneForm::CylindricalOrCircular => {
            Some("diameter")
        }
        ZoneForm::Spherical | ZoneForm::WithinASphere => Some("spherical_diameter"),
        _ => None,
    });
    let material = if t
        .modifiers
        .contains(&ToleranceModifier::MaximumMaterialRequirement)
    {
        Some("M")
    } else if t
        .modifiers
        .contains(&ToleranceModifier::LeastMaterialRequirement)
    {
        Some("L")
    } else {
        None
    };
    let datums: Vec<String> = t
        .datums
        .iter()
        .flat_map(|d| d.compartments())
        .map(|c| {
            c.references
                .iter()
                .map(|r| p.datums[r.datum.0].label().to_string())
                .collect::<Vec<_>>()
                .join("-")
        })
        .collect();
    gdt::control_frame(
        name,
        &t.magnitude.as_ref().map_or(String::new(), decimal),
        zone,
        material,
        &datums,
    )
}

/// A boxed datum letter about the local origin (Draftwright's datum frame).
pub fn datum(letter: &str) -> Callout {
    gdt::datum_frame(letter)
}
