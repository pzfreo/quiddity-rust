//! A part's PMI less some of its items: what the PMI writer is given again after it refuses
//! items (the refused items go, and with them every item that refers to one of them).

use super::model::*;
use super::write::ItemRef;

/// Which items of a part's PMI [`without_kept`] keeps, per kind, by index (`true` = kept).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Kept {
    pub standards: Vec<bool>,
    pub decimal_places: bool,
    pub features: Vec<bool>,
    pub datum_targets: Vec<bool>,
    pub datums: Vec<bool>,
    pub dimensions: Vec<bool>,
    pub tolerances: Vec<bool>,
    pub tolerance_relations: Vec<bool>,
    pub general: Vec<bool>,
    pub threads: Vec<bool>,
    pub knurls: Vec<bool>,
    pub holes: Vec<bool>,
    pub material: bool,
    pub notes: Vec<bool>,
    pub surface_textures: Vec<bool>,
    pub attributes: Vec<bool>,
}

/// *p* without the given items and every item that depends on them, indices renumbered.
#[must_use]
pub fn without(p: &PartPmi, items: &[ItemRef]) -> PartPmi {
    without_kept(p, items).0
}

/// *p* without the given items and every item that depends on them, indices renumbered, with
/// which of *p*'s items are kept.
#[must_use]
#[allow(
    clippy::unwrap_used,
    reason = "the cascade removes every item that names a removed one, so each id a kept item \
              names has an entry in its kind's renumbering, and the rebuilt datums, targets and \
              datum systems keep what established the originals"
)]
pub(crate) fn without_kept(p: &PartPmi, refused: &[ItemRef]) -> (PartPmi, Kept) {
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
            Feature::Derived {
                kind,
                from,
                geometry,
            } => Feature::Derived {
                geometry: *geometry,
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
    let out = PartPmi {
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
        holes: p
            .holes
            .iter()
            .enumerate()
            .filter(|(i, h)| !has(ItemRef::Hole(*i)) && f_ok(h.feature))
            .map(|(_, h)| {
                let mut h = h.clone();
                h.feature = f(h.feature);
                h
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
                // Read-only presentation, which the writer does not write.
                callouts: Vec::new(),
            })
            .collect(),
        surface_textures: p
            .surface_textures
            .iter()
            .enumerate()
            .filter(|(i, s)| !has(ItemRef::SurfaceTexture(*i)) && s.on.is_none_or(f_ok))
            .map(|(_, s)| SurfaceTexture {
                on: s.on.map(f),
                ..s.clone()
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
                description: a.description.clone(),
                general_property: a.general_property.clone(),
            })
            .collect(),
        geometry: p.geometry.clone(),
        derived_geometry: p.derived_geometry.clone(),
    };
    let kept = Kept {
        standards: (0..p.standards.len())
            .map(|i| !has(ItemRef::Standard(i)))
            .collect(),
        decimal_places: p.decimal_places.is_some() && !has(ItemRef::DecimalPlaces),
        features: gone_f.iter().map(|g| !g).collect(),
        datum_targets: gone_t.iter().map(|g| !g).collect(),
        datums: gone_d.iter().map(|g| !g).collect(),
        dimensions: gone_n.iter().map(|g| !g).collect(),
        tolerances: gone_g.iter().map(|g| !g).collect(),
        tolerance_relations: p
            .tolerance_relations
            .iter()
            .enumerate()
            .map(|(i, r)| {
                !has(ItemRef::ToleranceRelation(i)) && !gone_g[r.relating.0] && !gone_g[r.related.0]
            })
            .collect(),
        general: (0..p.general.len())
            .map(|i| !has(ItemRef::General(i)))
            .collect(),
        threads: p
            .threads
            .iter()
            .enumerate()
            .map(|(i, t)| {
                !has(ItemRef::Thread(i))
                    && f_ok(t.feature)
                    && t.partial_area.is_none_or(f_ok)
                    && t.runout.is_none_or(f_ok)
            })
            .collect(),
        knurls: p
            .knurls
            .iter()
            .enumerate()
            .map(|(i, k)| !has(ItemRef::Knurl(i)) && f_ok(k.feature))
            .collect(),
        holes: p
            .holes
            .iter()
            .enumerate()
            .map(|(i, h)| !has(ItemRef::Hole(i)) && f_ok(h.feature))
            .collect(),
        material: p.material.is_some() && !has(ItemRef::Material),
        notes: p
            .notes
            .iter()
            .enumerate()
            .map(|(i, n)| !has(ItemRef::Note(i)) && owner_ok(&n.on))
            .collect(),
        surface_textures: p
            .surface_textures
            .iter()
            .enumerate()
            .map(|(i, s)| !has(ItemRef::SurfaceTexture(i)) && s.on.is_none_or(f_ok))
            .collect(),
        attributes: p
            .attributes
            .iter()
            .enumerate()
            .map(|(i, a)| !has(ItemRef::Attribute(i)) && owner_ok(&a.on))
            .collect(),
    };
    (out, kept)
}
