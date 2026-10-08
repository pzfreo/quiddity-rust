//! General tolerances: ISO 2768 classes as a general tolerance's text states them, and ISO
//! 2768-1's table of permissible deviations for linear dimensions, so that a consumer can
//! evaluate a class.

use super::model::Decimal;

/// ISO 2768-1 tolerance class for linear and angular dimensions (Table 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LinearClass {
    /// f, fine.
    Fine,
    /// m, medium.
    Medium,
    /// c, coarse.
    Coarse,
    /// v, very coarse.
    VeryCoarse,
}

/// ISO 2768-2 tolerance class for geometrical tolerances (H, K, L).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GeometricClass {
    H,
    K,
    L,
}

/// The ISO 2768 classes a general tolerance states, as in `ISO 2768-m` or `ISO 2768-mK`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Iso2768 {
    pub linear: LinearClass,
    pub geometric: Option<GeometricClass>,
}

impl Iso2768 {
    /// The classes `text` states: `ISO 2768`, a hyphen or space, the linear class letter (f, m,
    /// c, v) and optionally the geometric class letter (H, K, L), e.g. `ISO 2768-mK`. Anything
    /// else is not recognised (`None`); the text is kept as stated either way.
    #[must_use]
    pub fn recognise(text: &str) -> Option<Iso2768> {
        let rest = text.trim().strip_prefix("ISO")?.trim_start();
        let rest = rest.strip_prefix("2768")?;
        let rest = rest
            .strip_prefix('-')
            .or_else(|| rest.strip_prefix(' '))?
            .trim_start();
        let mut chars = rest.chars();
        let linear = match chars.next()? {
            'f' => LinearClass::Fine,
            'm' => LinearClass::Medium,
            'c' => LinearClass::Coarse,
            'v' => LinearClass::VeryCoarse,
            _ => return None,
        };
        let geometric = match chars.next() {
            None => None,
            Some('H') => Some(GeometricClass::H),
            Some('K') => Some(GeometricClass::K),
            Some('L') => Some(GeometricClass::L),
            Some(_) => return None,
        };
        if chars.next().is_some() {
            return None;
        }
        Some(Iso2768 { linear, geometric })
    }
}

/// ISO 2768-1:1989 Table 1, "Permissible deviations for linear dimensions except for broken
/// edges": the nominal size ranges in millimetres (over the lower bound, up to and including
/// the upper; the first range is 0.5 up to and including 3) and the deviation `±` for classes
/// f, m, c and v. `None` where the standard gives no value (class f over 2000 mm, class v from
/// 0.5 to 3 mm).
pub const ISO_2768_1_LINEAR: [(&str, &str, [Option<&str>; 4]); 8] = [
    ("0.5", "3", [Some("0.05"), Some("0.1"), Some("0.2"), None]),
    (
        "3",
        "6",
        [Some("0.05"), Some("0.1"), Some("0.3"), Some("0.5")],
    ),
    (
        "6",
        "30",
        [Some("0.1"), Some("0.2"), Some("0.5"), Some("1")],
    ),
    (
        "30",
        "120",
        [Some("0.15"), Some("0.3"), Some("0.8"), Some("1.5")],
    ),
    (
        "120",
        "400",
        [Some("0.2"), Some("0.5"), Some("1.2"), Some("2.5")],
    ),
    (
        "400",
        "1000",
        [Some("0.3"), Some("0.8"), Some("2"), Some("4")],
    ),
    (
        "1000",
        "2000",
        [Some("0.5"), Some("1.2"), Some("3"), Some("6")],
    ),
    ("2000", "4000", [None, Some("2"), Some("4"), Some("8")]),
];

impl LinearClass {
    fn column(self) -> usize {
        match self {
            LinearClass::Fine => 0,
            LinearClass::Medium => 1,
            LinearClass::Coarse => 2,
            LinearClass::VeryCoarse => 3,
        }
    }

    /// The permissible deviation (±, millimetres) of a linear dimension of nominal size
    /// `nominal_mm` (Table 1); `None` below 0.5 mm (the standard says such deviations are
    /// indicated beside the nominal size), above 4000 mm, or where the table gives none.
    #[must_use]
    pub fn linear_deviation(self, nominal_mm: f64) -> Option<Decimal> {
        let n = nominal_mm.abs();
        ISO_2768_1_LINEAR
            .iter()
            .enumerate()
            .find(|(i, (lo, hi, _))| {
                let lo: f64 = lo.parse().unwrap_or(f64::NAN);
                let hi: f64 = hi.parse().unwrap_or(f64::NAN);
                (if *i == 0 { n >= lo } else { n > lo }) && n <= hi
            })
            .and_then(|(_, (_, _, d))| d[self.column()])
            .and_then(|d| Decimal::parse(d).ok())
    }
}
