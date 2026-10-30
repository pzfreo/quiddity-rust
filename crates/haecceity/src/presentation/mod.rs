//! One owner of requirement notation and callout geometry in a local 2D frame.
//!
//! Ported from pzfreo/draftwright-rust at aaa7a73: `src/dims/tolerance.rs`,
//! `src/dims/gdt.rs`, `src/pmi.rs` (qualifiers/limits/scope), and `src/text.rs` (Paul Fremantle; port authorized by the copyright owner for #137, same project family).
//! Sheet layout, leaders, lanes and drawn-precision rounding stay in
//! Draftwright. Values here retain requirement precision. Text uses public-domain Hershey
//! strokes plus explicit ISO symbols; no font parser or embedded outline font is needed.

use crate::pmi::*;

mod gdt;
pub mod linear;
pub mod notes;
pub mod placement;
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
    /// Draftwright's leader-bend scope symbol: one ring for all-around, two for all-over.
    pub leader_rings: u8,
}

pub fn font() -> &'static text::Font {
    &text::Font
}

impl Callout {
    /// Full, readable notation, shared by presentation names and drawing clients.
    pub fn notation(&self) -> String {
        self.cells.join(" ")
    }

    /// Single-stroke text, frame lines and explicit symbol arcs; no filled text surfaces.
    pub fn polylines(&self) -> Vec<Vec<[f64; 2]>> {
        let mut lines: Vec<_> = self.strokes.iter().map(|s| s.to_vec()).collect();
        for &[x, y, r, a, b] in &self.arcs {
            lines.push(
                (0..=32)
                    .map(|i| {
                        let t = (a + (b - a) * i as f64 / 32.).to_radians();
                        [x + r * t.cos(), y + r * t.sin()]
                    })
                    .collect(),
            );
        }
        for t in &self.letters {
            for line in font().text(&t.text, t.size).lines {
                lines.push(
                    line.into_iter()
                        .map(|p| [p[0] + t.at[0], p[1] + t.at[1]])
                        .collect(),
                );
            }
        }
        lines
    }
}

fn decimal(v: &Value) -> String {
    tolerance::number(v.decimal())
}

// Convert only when the requirement actually uses different units, never to sheet precision.
fn in_units(v: &Value, nominal: &Value) -> Decimal {
    if v.quantity.cmp_same_unit(&nominal.quantity).is_some() {
        v.decimal().clone()
    } else {
        let scale = match &nominal.quantity {
            Quantity::Length(l) => l.unit.millimetres(),
            Quantity::Angle(a) => a.unit.radians(),
        };
        if scale.is_finite() && scale != 0.0 {
            Decimal::parse(&(v.si() / scale).to_string()).unwrap_or_else(|_| v.decimal().clone())
        } else {
            v.decimal().clone()
        }
    }
}

/// Text and CAx-IF annotation type for a dimension. Limits remain limits; a fit remains its
/// ISO 286 class when no limits are stated. Zero deviations have no sign (ISO 129-1).
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
                    .map_or_else(|| v.decimal().clone(), |n| in_units(v, n))
            };
            let u = value(b.upper());
            let l = value(b.lower());
            let band = tolerance::band_text(&u, &l);
            text.push_str(&format!(" {band}"));
        }
        // Draftwright src/pmi.rs dimension_label: stated fit limits print as a range,
        // exactly like explicit limits; do not compute unstated limits from a class.
        DimTolerance::Limits(b)
        | DimTolerance::Fit {
            limits: Some(b), ..
        } => {
            text = format!(
                "{prefix}{}{suffix} - {prefix}{}{suffix}",
                decimal(b.lower()),
                decimal(b.upper())
            );
        }
        DimTolerance::Fit { class, .. } => text.push_str(&format!(" {class}")),
        _ => {}
    }
    // Ported from Draftwright aaa7a73 src/pmi.rs:1934–1941.
    if let Some(suffix) = d.qualifier.as_ref().and_then(|q| match q {
        Qualifier::Maximum => Some("MAX"),
        Qualifier::Minimum => Some("MIN"),
        Qualifier::Average => Some("AVG"),
        Qualifier::Other(_) => None,
    }) {
        text.push_str(&format!(" {suffix}"));
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
    let ink = font().text(value, 3.0);
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
    let mut c = gdt::control_frame(
        name,
        &t.magnitude.as_ref().map_or(String::new(), decimal),
        zone,
        material,
        &datums,
    );
    // Draftwright pmi.rs support()/tolerance() and dims/gdt.rs leader scope rings.
    fn around(p: &PartPmi, f: FeatureId, depth: usize) -> bool {
        if depth > p.features.len() {
            return false;
        }
        match &p.features[f.0] {
            Feature::Group {
                kind: GroupKind::AllAround,
                ..
            } => true,
            Feature::Group { members, .. } | Feature::Derived { from: members, .. } => {
                members.iter().any(|f| around(p, *f, depth + 1))
            }
            Feature::Items(_) => false,
        }
    }
    c.leader_rings = if t.target == ToleranceTarget::WholePart
        || t.auxiliary.contains(&AuxiliaryClassification::AllOver)
    {
        2
    } else {
        let features = match &t.target {
            ToleranceTarget::Feature(f) => vec![*f],
            ToleranceTarget::Relation {
                relating, related, ..
            } => vec![*relating, *related],
            ToleranceTarget::Dimension(d) => match p.dimensions[d.0].kind {
                DimensionKind::Size { feature, .. } => vec![feature],
                DimensionKind::Location { from, to, .. } => vec![from, to],
            },
            ToleranceTarget::WholePart => vec![],
        };
        u8::from(features.iter().any(|f| around(p, *f, 0)))
    };
    c
}

/// A boxed datum letter about the local origin (Draftwright's datum frame).
pub fn datum(letter: &str) -> Callout {
    gdt::datum_frame(letter)
}
