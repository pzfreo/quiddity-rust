//! Part-relative recognition with an explicit caller-space frame (`quiddity.frames`).
//!
//! [`recognise`](crate::features::recognise) stays caller-space, as Python's entry points do.
//! Framed recognition infers a frame from the part's own geometry ([`infer_part_frame`]),
//! re-reads the part placed in that frame, and recognises it there: the records are in the
//! frame's local coordinates, and the frame says how to read them back in caller space. The
//! working part is the same STEP read with the normalisation composed onto the caller's
//! placement, so its faces are the caller part's faces under the same indices (Python pairs them
//! by `IsSame`; here they are one read order).
//!
//! The origin is the centroid of the part's solids ([`Part::volume_centroid`]), where Python
//! takes `BRepGProp::VolumeProperties` of the whole imported shape at its default rule: that also
//! counts a loose shell's open faces, and is 1e-9 to 1e-8 off on some analytic parts, enough to
//! make a mirror-symmetric part's rounded face offsets look asymmetric
//! (`tests/fixtures/captured/known_frames.json` lists each difference).
//!
//! Python's evidence projection onto caller faces (`_project_recognition_evidence`) is not
//! ported: the face indices already are the caller's.

use serde::Serialize;

use crate::features::{self, Features};
use crate::kernel::brep::Part;
use crate::kernel::geom::{Surface, V3, cross};
use crate::kernel::py;
use crate::kernel::step::{Placement, read_step_file_placed, read_step_placed};

/// `_PARALLEL_COS`: two directions closer than this are one direction line.
const PARALLEL_COS: f64 = 0.999;
/// `_ORTHOGONAL_COS`: two direction lines at least this close to perpendicular make a basis.
const ORTHOGONAL_COS: f64 = 1.0 - PARALLEL_COS;
/// `_COMPONENT_EPS`: components this close to 0 or ±1 are snapped there.
const COMPONENT_EPS: f64 = 1e-12;

/// `FrameGauge`: how much of the returned basis the solid establishes. `Full`: a directed,
/// ordered basis. `Orthogonal`: two perpendicular direction lines, with a sign or an interchange
/// unobservable. `Axial`: one direction line, roll about it unobservable. In the latter two the
/// axes are representatives of the gauge, not material directions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FrameGauge {
    Full,
    Orthogonal,
    Axial,
}

/// `FrameRefusalReason`, plus `UnmeasuredFace` (the port only): the kernel cannot integrate a
/// plane or cylinder face's area, or a solid's volume, where OpenCascade always returns a
/// number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FrameRefusal {
    NoMaterial,
    NoAnalyticDirection,
    NonfiniteGeometry,
    UnmeasuredFace,
}

impl FrameRefusal {
    /// Python's `FrameRefusalReason` value.
    pub fn as_str(self) -> &'static str {
        match self {
            FrameRefusal::NoMaterial => "no-material",
            FrameRefusal::NoAnalyticDirection => "no-analytic-direction",
            FrameRefusal::NonfiniteGeometry => "nonfinite-geometry",
            FrameRefusal::UnmeasuredFace => "unmeasured-face",
        }
    }
}

/// `PartFrame`: the caller-space placement of the local recognition coordinates. A caller-space
/// point `p` is `(dot(p - origin, x), dot(p - origin, y), dot(p - origin, z))` locally.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct PartFrame {
    pub origin: V3,
    pub x: V3,
    pub y: V3,
    pub z: V3,
    pub gauge: FrameGauge,
}

impl PartFrame {
    pub fn to_local(&self, p: V3) -> V3 {
        let d = [0, 1, 2].map(|i| p[i] - self.origin[i]);
        [self.x, self.y, self.z].map(|axis| py::dot(&d, &axis))
    }

    pub fn to_world(&self, p: V3) -> V3 {
        [0, 1, 2].map(|i| self.origin[i] + p[0] * self.x[i] + p[1] * self.y[i] + p[2] * self.z[i])
    }

    /// `_normalization_location`: the rigid motion taking caller space to local coordinates.
    pub fn normalisation(&self) -> Placement {
        [self.x, self.y, self.z].map(|a| [a[0], a[1], a[2], -py::dot(&a, &self.origin)])
    }
}

/// A plane's normals or a cylinder's axes, gathered as one direction line (`_DirectionClass`).
struct DirectionClass {
    direction: V3,
    area: f64,
    face_areas: Vec<f64>,
    /// Each face's centroid offset from the origin along `direction`, with its area.
    face_offsets: Vec<(f64, f64)>,
}

/// `_DirectionClass.signature`, rounded to 9 decimals: the rotation-invariant rank of a class.
type Signature = (f64, Vec<f64>, Vec<[f64; 2]>);

fn descending(a: &[f64; 2], b: &[f64; 2]) -> std::cmp::Ordering {
    py::tuple_order(b, a)
}

impl DirectionClass {
    fn signature(&self) -> Signature {
        let r = |v: f64| py::round_to(v, 9);
        let mut areas: Vec<f64> = self.face_areas.iter().map(|&a| r(a)).collect();
        areas.sort_by(|a, b| py::order(*b, *a));
        let mut offsets: Vec<[f64; 2]> = self
            .face_offsets
            .iter()
            .map(|&(o, a)| [r(o.abs()), r(a)])
            .collect();
        offsets.sort_by(descending);
        (r(self.area), areas, offsets)
    }

    /// `oriented`: the geometry-directed representative, and whether its sign is observable
    /// (the face offsets are not symmetric about the origin).
    fn oriented(&self) -> (V3, bool) {
        let side = |sign: f64| {
            let mut out: Vec<[f64; 2]> = self
                .face_offsets
                .iter()
                .map(|&(o, a)| [py::round_to(sign * o, 9), py::round_to(a, 9)])
                .collect();
            out.sort_by(descending);
            out
        };
        let (forward, reverse) = (side(1.0), side(-1.0));
        let order = forward
            .iter()
            .zip(&reverse)
            .map(|(f, r)| py::tuple_order(f, r))
            .find(|o| o.is_ne());
        match order {
            None => (self.direction, false),
            Some(o) => {
                let sign = if o.is_gt() { 1.0 } else { -1.0 };
                (self.direction.map(|c| sign * c), true)
            }
        }
    }
}

fn signature_order(a: &Signature, b: &Signature) -> std::cmp::Ordering {
    py::order(a.0, b.0)
        .then_with(|| py::tuple_order(&a.1, &b.1))
        .then_with(|| {
            a.2.iter()
                .zip(&b.2)
                .map(|(x, y)| py::tuple_order(x, y))
                .find(|o| o.is_ne())
                .unwrap_or_else(|| a.2.len().cmp(&b.2.len()))
        })
}

/// `_canonical_sign`: the sign that makes the largest component (the last of equals) positive.
fn canonical_sign(v: V3) -> V3 {
    let pivot = (0..3)
        .max_by(|&i, &j| py::order(v[i].abs(), v[j].abs()).then(i.cmp(&j)))
        .unwrap();
    let sign = if v[pivot] < 0.0 { -1.0 } else { 1.0 };
    v.map(|c| sign * c)
}

/// `_clean`: components within 1e-12 of 0 or ±1 snapped there.
fn clean(v: V3) -> V3 {
    v.map(|c| {
        if c.abs() <= COMPONENT_EPS {
            0.0
        } else if (c.abs() - 1.0).abs() <= COMPONENT_EPS {
            1f64.copysign(c)
        } else {
            c
        }
    })
}

fn unit(v: V3) -> Result<V3, FrameRefusal> {
    py::unit(v).ok_or(FrameRefusal::NonfiniteGeometry)
}

/// `_material_origin`: the centre of mass of the part's solids.
fn material_origin(part: &Part) -> Result<V3, FrameRefusal> {
    if part.solids.is_empty() {
        return Err(FrameRefusal::NoMaterial);
    }
    let (mass, centre) = part.volume_centroid().ok_or(FrameRefusal::UnmeasuredFace)?;
    if !mass.is_finite() {
        return Err(FrameRefusal::NonfiniteGeometry);
    }
    if mass <= COMPONENT_EPS {
        return Err(FrameRefusal::NoMaterial);
    }
    if !centre.iter().all(|c| c.is_finite()) {
        return Err(FrameRefusal::NonfiniteGeometry);
    }
    Ok(centre)
}

/// `infer_part_frame`: a rigid-equivariant material origin and a local basis from the part's
/// plane normals and cylinder axes, ranked by area and by how the faces sit along them.
pub fn infer_part_frame(part: &Part) -> Result<PartFrame, FrameRefusal> {
    let origin = material_origin(part)?;
    let mut classes: Vec<DirectionClass> = Vec::new();
    for (index, face) in part.faces.iter().enumerate() {
        // Python reads `face.normal_at()` for a plane; its sign is dropped by the canonical sign.
        let raw = match &face.surface {
            Surface::Plane { frame } | Surface::Cylinder { frame, .. } => frame.z,
            _ => continue,
        };
        let direction = canonical_sign(unit(raw)?);
        let moments = part
            .face_moments(index)
            .ok_or(FrameRefusal::UnmeasuredFace)?;
        let (area, centre) = (moments.area, moments.centroid);
        if !area.is_finite() || !centre.iter().all(|c| c.is_finite()) {
            return Err(FrameRefusal::NonfiniteGeometry);
        }
        let relative = [0, 1, 2].map(|i| centre[i] - origin[i]);
        let mut offset = py::dot(&relative, &direction);
        match classes
            .iter_mut()
            .find(|c| py::dot(&direction, &c.direction).abs() >= PARALLEL_COS)
        {
            Some(class) => {
                if py::dot(&direction, &class.direction) < 0.0 {
                    offset = -offset;
                }
                class.area += area;
                class.face_areas.push(area);
                class.face_offsets.push((offset, area));
            }
            None => classes.push(DirectionClass {
                direction,
                area,
                face_areas: vec![area],
                face_offsets: vec![(offset, area)],
            }),
        }
    }

    // Python's `sorted(..., reverse=True)`: descending, equal signatures in face order.
    let signatures: Vec<Signature> = classes.iter().map(DirectionClass::signature).collect();
    let mut ranked: Vec<usize> = (0..classes.len()).collect();
    ranked.sort_by(|&a, &b| signature_order(&signatures[b], &signatures[a]));
    // Any equal-ranked class leaves a possible axis interchange, even when it is not chosen.
    let distinct = (1..ranked.len())
        .all(|i| signature_order(&signatures[ranked[i - 1]], &signatures[ranked[i]]).is_ne());
    for (k, &a) in ranked.iter().enumerate() {
        for &b in &ranked[k + 1..] {
            let (first, first_signed) = classes[a].oriented();
            let (second, second_signed) = classes[b].oriented();
            let along = py::dot(&first, &second);
            if along.abs() > ORTHOGONAL_COS {
                continue;
            }
            let y = unit([0, 1, 2].map(|i| second[i] - along * first[i]))?;
            let (x, y) = (clean(first), clean(y));
            let z = clean(unit(cross(x, y))?);
            let gauge = if first_signed && second_signed && distinct {
                FrameGauge::Full
            } else {
                FrameGauge::Orthogonal
            };
            return Ok(PartFrame {
                origin,
                x,
                y,
                z,
                gauge,
            });
        }
    }
    let Some(&first) = ranked.first() else {
        return Err(FrameRefusal::NoAnalyticDirection);
    };
    // One axis leaves roll unconstrained: the world axis least along it seeds a deterministic
    // representative of the axial gauge, not a material direction.
    let (x, _) = classes[first].oriented();
    let seeds = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let seed = seeds[py::first_min(&seeds, |s| py::dot(&x, s).abs())];
    let along = py::dot(&x, &seed);
    let y = unit([0, 1, 2].map(|i| seed[i] - along * x[i]))?;
    let (x, y) = (clean(x), clean(y));
    let z = clean(unit(cross(x, y))?);
    Ok(PartFrame {
        origin,
        x,
        y,
        z,
        gauge: FrameGauge::Axial,
    })
}

/// Why framed recognition gave no result: the part could not be read, or the frame was refused.
#[derive(Debug)]
pub enum FramedError {
    Read(Box<dyn std::error::Error>),
    Frame(FrameRefusal),
}

impl std::fmt::Display for FramedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FramedError::Read(e) => e.fmt(f),
            FramedError::Frame(r) => write!(f, "no part frame: {}", r.as_str()),
        }
    }
}

impl std::error::Error for FramedError {}

/// `PreparedFramedPart`: the frame inferred on the caller's part, and the working part (the
/// caller's part in the frame's local coordinates, its faces under the caller's indices).
pub struct FramedPart {
    pub frame: PartFrame,
    pub part: Part,
}

/// `outer` then `inner`, as one placement.
fn then(outer: &Placement, inner: &Placement) -> Placement {
    let mut out = [[0.0; 4]; 3];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, value) in row.iter_mut().enumerate() {
            *value = (0..3).map(|k| inner[i][k] * outer[k][j]).sum::<f64>();
        }
        row[3] += inner[i][3];
    }
    out
}

/// A part's source: its STEP file read under a placement ([`read_step_placed`] on the file's
/// bytes, or [`read_step_file_placed`]).
pub type Read<'a> = &'a dyn Fn(&Placement) -> Result<Part, Box<dyn std::error::Error>>;

/// `prepare_framed_part` for the part *read* gives under *placement* (the caller's space):
/// infers the frame on that part and reads it again with the frame's normalisation composed on.
/// The kernel's parts are not moved after reading, so the normalisation is applied by reading
/// again.
pub fn prepare_framed(read: Read<'_>, placement: &Placement) -> Result<FramedPart, FramedError> {
    let caller = read(placement).map_err(FramedError::Read)?;
    let frame = infer_part_frame(&caller).map_err(FramedError::Frame)?;
    let part = read(&then(placement, &frame.normalisation())).map_err(FramedError::Read)?;
    assert_eq!(
        part.faces.len(),
        caller.faces.len(),
        "a placement does not change what is read"
    );
    Ok(FramedPart { frame, part })
}

/// [`prepare_framed`] for a STEP file's bytes.
pub fn prepare_framed_step(bytes: &[u8], placement: &Placement) -> Result<FramedPart, FramedError> {
    prepare_framed(&|p| Ok(read_step_placed(bytes, p)?), placement)
}

/// [`prepare_framed`] for a STEP file on disk (gzipped when its name ends in `.gz`).
pub fn prepare_framed_file(
    path: &std::path::Path,
    placement: &Placement,
) -> Result<FramedPart, FramedError> {
    prepare_framed(&|p| read_step_file_placed(path, p), placement)
}

impl FramedPart {
    /// `PreparedFramedPart.recognise`: the working part recognised, with its frame.
    pub fn recognise(self) -> FramedRecognition {
        let features = features::recognise(&self.part);
        FramedRecognition {
            frame: self.frame,
            part: self.part,
            features,
        }
    }
}

/// `FramedRecognitionResult`: the records found in the part's own frame, the frame, and the
/// working part they were found on.
pub struct FramedRecognition {
    pub frame: PartFrame,
    pub part: Part,
    pub features: Features,
}
