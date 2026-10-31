use super::*;
use std::collections::{BTreeMap, BTreeSet};

const NEAR: f64 = 1e-4;
fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < NEAR
}
pub(super) fn principal(d: Point) -> Option<usize> {
    let n = d.iter().map(|x| x * x).sum::<f64>().sqrt();
    if !n.is_finite() || n <= 0. {
        return None;
    }
    let axes: Vec<_> = (0..3).filter(|&k| (d[k] / n).abs() >= 1e-6).collect();
    (axes.len() == 1).then(|| axes[0])
}
fn across(k: usize) -> [usize; 2] {
    match k {
        0 => [1, 2],
        1 => [0, 2],
        _ => [0, 1],
    }
}
pub(super) fn group_id(members: &[String]) -> String {
    let mut members = members.to_vec();
    members.sort();
    members.dedup();
    format!("pattern[{}]", members.join(","))
}
fn tolerance(input: &Input) -> Tolerance {
    if input.policy.general_tolerance {
        Tolerance::General
    } else {
        Tolerance::Untoleranced
    }
}
fn size_id(feature: &str, role: &str) -> String {
    format!("dim:{feature}/{role}")
}
fn location(
    from: &Support,
    to: &Support,
    axis: usize,
    pattern: Option<PatternRole>,
    input: &Input,
) -> Required {
    Required {
        id: format!("dim:{}/to/{}/{}", from.feature, to.feature, axis),
        kind: Kind::Location {
            from: from.clone(),
            to: to.clone(),
            axis,
        },
        nominal: (to.at[axis] - from.at[axis]).abs(),
        count: 1,
        pattern,
        tolerance: tolerance(input),
        reference: None,
        covers: vec![to.feature.clone()],
    }
}
fn sizes(owner: &str, members: &[&Hole], input: &Input) -> Vec<Required> {
    let first = members[0];
    let mut quantities = vec![Size {
        role: "bore.diameter".into(),
        nominal: first.diameter,
        kind: SizeKind::Diameter,
    }];
    if let Some(depth) = first.depth {
        quantities.push(Size {
            role: "bore.depth".into(),
            nominal: depth,
            kind: SizeKind::Length,
        });
    }
    quantities.extend(first.details.clone());
    quantities
        .into_iter()
        .map(|s| Required {
            id: size_id(owner, &s.role),
            kind: Kind::Size {
                feature: owner.into(),
                role: s.role,
                kind: s.kind,
            },
            nominal: s.nominal,
            count: drillings(members),
            pattern: None,
            tolerance: tolerance(input),
            reference: None,
            covers: members.iter().map(|m| m.support.feature.clone()).collect(),
        })
        .collect()
}
fn drillings(members: &[&Hole]) -> usize {
    type Drilling = (String, [i64; 3], Option<usize>);
    type Segment = (f64, f64, f64);
    let mut lines: BTreeMap<Drilling, Vec<Segment>> = BTreeMap::new();
    let mut count = 0;
    for h in members {
        if h.depth.is_some() || !h.details.is_empty() {
            count += 1;
            continue;
        }
        let Some(depth) = h.traversal.filter(|d| d.is_finite() && *d > 0.) else {
            count += 1;
            continue;
        };
        let norm = h.direction.iter().map(|x| x * x).sum::<f64>().sqrt();
        if norm <= 0. {
            count += 1;
            continue;
        }
        let d = h.direction.map(|x| x / norm);
        let at = h.support.at;
        let along = (0..3).map(|k| at[k] * d[k]).sum::<f64>();
        let line = [0, 1, 2].map(|k| ((at[k] - along * d[k]) * 1000.).round() as i64);
        lines
            .entry((spec(h), line, h.support.body))
            .or_default()
            .push((along, along + depth, h.diameter));
    }
    for mut segments in lines.into_values() {
        segments.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut reach = f64::NEG_INFINITY;
        for (lo, hi, diameter) in segments {
            if lo - reach >= diameter {
                count += 1;
            }
            reach = reach.max(hi);
        }
    }
    count
}
fn spec(h: &Hole) -> String {
    // Same machining requirement; through traversal length is deliberately absent.
    let details: Vec<_> = h
        .details
        .iter()
        .map(|s| (&s.role, size_key(s.nominal), s.kind))
        .collect();
    let mut details = details;
    details.sort_by(|a, b| a.0.cmp(b.0));
    serde_json::to_string(&(
        size_key(h.diameter),
        h.depth.map(size_key),
        &h.bottom,
        h.direction.map(size_key),
        details,
    ))
    .unwrap()
}
fn size_key(x: f64) -> i64 {
    (x * 1e6).round() as i64
}
fn key(x: f64) -> i64 {
    (x * 10000.).round() as i64
}
fn distinct(mut xs: Vec<f64>) -> Vec<f64> {
    xs.sort_by(f64::total_cmp);
    xs.dedup_by(|a, b| near(*a, *b));
    xs
}
fn even(xs: &[f64]) -> bool {
    xs.len() < 3 || xs.windows(2).all(|w| near(w[1] - w[0], xs[1] - xs[0]))
}
fn lattice(centres: &[Point], k: usize) -> Option<(Vec<f64>, Vec<f64>, Point)> {
    let [u, v] = across(k);
    let cols = distinct(centres.iter().map(|c| c[u]).collect());
    let rows = distinct(centres.iter().map(|c| c[v]).collect());
    if centres.is_empty()
        || cols.len() * rows.len() != centres.len()
        || !even(&cols)
        || !even(&rows)
    {
        return None;
    }
    let sites: BTreeSet<_> = centres.iter().map(|c| (key(c[u]), key(c[v]))).collect();
    if sites.len() != centres.len() {
        return None;
    }
    let mut c = centres[0];
    c[u] = (cols[0] + cols[cols.len() - 1]) / 2.;
    c[v] = (rows[0] + rows[rows.len() - 1]) / 2.;
    Some((cols, rows, c))
}
fn line(centres: &[Point], k: usize) -> Option<(usize, Vec<f64>)> {
    let [u, v] = across(k);
    for (along, across) in [(u, v), (v, u)] {
        let xs = distinct(centres.iter().map(|c| c[along]).collect());
        if xs.len() == centres.len()
            && xs.len() >= 2
            && even(&xs)
            && distinct(centres.iter().map(|c| c[across]).collect()).len() == 1
        {
            return Some((along, xs));
        }
    }
    None
}
fn established<'a>(
    p: &Pattern,
    holes: &BTreeMap<&str, &'a Hole>,
    input: &Input,
) -> Option<Vec<&'a Hole>> {
    let members: Vec<_> = p
        .members
        .iter()
        .map(|m| holes.get(m.as_str()).copied())
        .collect::<Option<_>>()?;
    if members.len() < 2 || members.iter().any(|h| spec(h) != spec(members[0])) {
        return None;
    }
    let k = principal(members[0].direction)?;
    let [u, v] = across(k);
    let sites: BTreeSet<_> = members
        .iter()
        .map(|h| (key(h.support.at[u]), key(h.support.at[v])))
        .collect();
    if sites.len() != members.len() {
        return None;
    }
    if let Arrangement::BoltCircle { centre, diameter } = p.arrangement {
        if !diameter.is_finite() || diameter <= 0. {
            return None;
        }
        if members.len() < 5 {
            let corroborated = holes
                .values()
                .filter(|h| !p.members.contains(&h.support.feature))
                .map(|h| (h.support.at, h.direction))
                .chain(input.round_axes.iter().copied())
                .any(|(at, d)| {
                    principal(d) == Some(k)
                        && ((at[u] - centre[u]).powi(2) + (at[v] - centre[v]).powi(2)).sqrt() <= 0.5
                });
            if !corroborated {
                return None;
            }
        }
    }
    Some(members)
}
fn origin(input: &Input, axis: usize) -> Support {
    let mut datums = input.datums.clone();
    datums.sort_by(|a, b| a.label.cmp(&b.label).then(a.faces.cmp(&b.faces)));
    let datum = |label: Option<&str>| {
        datums
            .iter()
            .filter(|d| label.is_none_or(|s| s == d.label))
            .find_map(|d| {
                let mut matches = input.planes.iter().filter(|p| {
                    p.axis == axis
                        && d.faces.iter().all(|f| p.support.faces.contains(f))
                        && !d.faces.is_empty()
                });
                let p = matches.next()?;
                Some(p.support.clone())
            })
    };
    match &input.policy.origins[axis] {
        Origin::Automatic => {
            if let Some(s) = datum(None) {
                return s;
            }
        }
        Origin::Datum(label) => {
            if let Some(s) = datum(Some(label)) {
                return s;
            }
        }
        _ => {}
    }
    let at = input.bounds[usize::from(input.policy.origins[axis] == Origin::Maximum)][axis];
    let mut supports: Vec<_> = input
        .planes
        .iter()
        .filter(|p| p.axis == axis && near(p.support.at[axis], at))
        .collect();
    supports.sort_by(|a, b| a.support.feature.cmp(&b.support.feature));
    if let Some(p) = supports.first() {
        return p.support.clone();
    }
    let mut point = [0.; 3];
    point[axis] = at;
    Support {
        feature: format!(
            "envelope:{axis}:{}",
            if input.policy.origins[axis] == Origin::Maximum {
                "max"
            } else {
                "min"
            }
        ),
        faces: vec![],
        body: None,
        at: point,
        plane: true,
    }
}
fn locate(
    owner: &Support,
    k: usize,
    role: Option<PatternRole>,
    covers: &[String],
    input: &Input,
) -> Vec<Required> {
    across(k)
        .into_iter()
        .filter_map(|axis| {
            let mut r = location(&origin(input, axis), owner, axis, role.clone(), input);
            r.covers = covers.to_vec();
            (r.nominal > NEAR).then_some(r)
        })
        .collect()
}
fn pitch(
    owner: &Support,
    axis: usize,
    values: &[f64],
    covers: &[String],
    input: &Input,
) -> Required {
    let mut a = owner.clone();
    let mut b = owner.clone();
    a.feature = format!("{}:station:{axis}:{}", owner.feature, key(values[0]));
    b.feature = format!("{}:station:{axis}:{}", owner.feature, key(values[1]));
    a.at[axis] = values[0];
    b.at[axis] = values[1];
    let mut r = location(
        &a,
        &b,
        axis,
        Some(PatternRole::Pitch {
            intervals: values.len() - 1,
        }),
        input,
    );
    r.id = format!("dim:{}/pitch/{axis}", owner.feature);
    r.covers = covers.to_vec();
    r
}

pub(super) fn plan(input: &Input) -> (Vec<Required>, Vec<Conveyed>) {
    let mut out = Vec::new();
    let mut conveyed = Vec::new();
    let holes: BTreeMap<_, _> = input
        .holes
        .iter()
        .map(|h| (h.support.feature.as_str(), h))
        .collect();
    let mut patterns = input.patterns.clone();
    for p in &mut patterns {
        p.members.sort();
    }
    patterns.sort_by(|a, b| {
        b.members
            .len()
            .cmp(&a.members.len())
            .then(a.id().cmp(&b.id()))
    });
    let mut taken = BTreeSet::new();
    for p in &patterns {
        let Some(members) = established(p, &holes, input) else {
            continue;
        };
        if p.members.iter().any(|m| taken.contains(m)) {
            continue;
        }
        let k = principal(members[0].direction).unwrap();
        let centres: Vec<_> = members.iter().map(|m| m.support.at).collect();
        let mut supports = members
            .iter()
            .flat_map(|m| m.support.faces.iter().copied())
            .collect::<Vec<_>>();
        supports.sort();
        supports.dedup();
        let mut owner = Support {
            feature: p.id(),
            faces: supports,
            body: members[0].support.body,
            at: centres[0],
            plane: false,
        };
        let mut spacing = Vec::new();
        let role = match p.arrangement {
            Arrangement::Grid => {
                let Some((cols, rows, c)) = lattice(&centres, k) else {
                    continue;
                };
                owner.at = c;
                for (axis, values) in across(k).into_iter().zip([cols, rows]) {
                    if values.len() > 1 {
                        spacing.push(pitch(&owner, axis, &values, &p.members, input));
                    }
                }
                PatternRole::Centre
            }
            Arrangement::Line => {
                let Some((axis, values)) = line(&centres, k) else {
                    continue;
                };
                owner.at = *centres.iter().find(|c| near(c[axis], values[0])).unwrap();
                spacing.push(pitch(&owner, axis, &values, &p.members, input));
                PatternRole::FirstMember
            }
            Arrangement::BoltCircle { centre, diameter } => {
                owner.at = centre;
                spacing.push(Required {
                    id: size_id(&owner.feature, "bolt_circle.diameter"),
                    kind: Kind::Size {
                        feature: owner.feature.clone(),
                        role: "bolt_circle.diameter".into(),
                        kind: SizeKind::Diameter,
                    },
                    nominal: diameter,
                    count: 1,
                    pattern: Some(PatternRole::Centre),
                    tolerance: tolerance(input),
                    reference: None,
                    covers: p.members.clone(),
                });
                PatternRole::Centre
            }
        };
        taken.extend(p.members.iter().cloned());
        let shared_sizes = sizes(&owner.feature, &members, input);
        let shared_locations = locate(&owner, k, Some(role), &p.members, input);
        for m in &members {
            for r in sizes(&m.support.feature, &[m], input) {
                let Kind::Size { role, .. } = &r.kind else {
                    unreachable!()
                };
                conveyed.push(Conveyed {
                    id: r.id,
                    by: vec![size_id(&owner.feature, role)],
                    why: "pattern size states each identical member".into(),
                });
            }
            for r in locate(
                &m.support,
                k,
                None,
                std::slice::from_ref(&m.support.feature),
                input,
            ) {
                let Kind::Location { axis, .. } = r.kind else {
                    unreachable!()
                };
                let by = shared_locations
                    .iter()
                    .chain(&spacing)
                    .filter(|s| {
                        matches!(&s.kind,Kind::Location { axis:a, .. } if *a==axis)
                            || matches!(p.arrangement, Arrangement::BoltCircle { .. })
                    })
                    .map(|r| r.id.clone())
                    .collect();
                conveyed.push(Conveyed {
                    id: r.id,
                    by,
                    why: "pattern origin and spacing locate the member".into(),
                });
            }
        }
        out.extend(shared_sizes);
        out.extend(shared_locations);
        out.extend(spacing);
    }
    // Unpatterned equal holes share their complete machining spec, not just one diameter.
    let mut groups: BTreeMap<String, Vec<&Hole>> = BTreeMap::new();
    for h in holes
        .values()
        .filter(|h| !taken.contains(&h.support.feature))
    {
        groups.entry(spec(h)).or_default().push(h);
    }
    for members in groups.values() {
        let owner = if members.len() == 1 {
            members[0].support.feature.clone()
        } else {
            group_id(
                &members
                    .iter()
                    .map(|m| m.support.feature.clone())
                    .collect::<Vec<_>>(),
            )
        };
        let shared = sizes(&owner, members, input);
        for m in members {
            if members.len() > 1 {
                for r in sizes(&m.support.feature, &[m], input) {
                    let Kind::Size { role, .. } = &r.kind else {
                        unreachable!()
                    };
                    conveyed.push(Conveyed {
                        id: r.id,
                        by: vec![size_id(&owner, role)],
                        why: "identical machining specifications share a counted size".into(),
                    });
                }
            }
            if let Some(k) = principal(m.direction) {
                out.extend(locate(
                    &m.support,
                    k,
                    None,
                    std::slice::from_ref(&m.support.feature),
                    input,
                ));
            }
        }
        out.extend(shared);
    }
    // One baseline owner per physical support plane. Extents remain overall measurements.
    let mut planes = input.planes.clone();
    planes.sort_by(|a, b| {
        a.body
            .cmp(&b.body)
            .then(a.axis.cmp(&b.axis))
            .then(a.support.at[a.axis].total_cmp(&b.support.at[b.axis]))
            .then(a.support.feature.cmp(&b.support.feature))
    });
    let mut bodies: BTreeMap<(usize, usize), Vec<&Plane>> = BTreeMap::new();
    for p in &planes {
        if p.axis < 3 {
            bodies.entry((p.body, p.axis)).or_default().push(p);
        }
    }
    for ((_, axis), ps) in bodies {
        if ps.len() < 2 {
            continue;
        }
        let (lo, hi) = (&ps[0].support, &ps[ps.len() - 1].support);
        let mut overall = location(lo, hi, axis, None, input);
        overall.covers = ps.iter().map(|p| p.support.feature.clone()).collect();
        if overall.nominal > NEAR {
            out.push(overall);
        }
        for p in &ps[1..ps.len() - 1] {
            let r = location(&origin(input, axis), &p.support, axis, None, input);
            if r.nominal > NEAR {
                out.push(r);
            }
        }
    }
    // A curved outer side may have no plane; still state the envelope extent once.
    for axis in 0..3 {
        let lo = input.bounds[0][axis];
        let hi = input.bounds[1][axis];
        if hi - lo <= NEAR
            || out
                .iter()
                .any(|r| span(r).is_some_and(|(k, a, b)| k == axis && near(a, lo) && near(b, hi)))
        {
            continue;
        }
        let support = |end: usize| Support {
            feature: format!("envelope:{axis}:{end}"),
            faces: vec![],
            body: None,
            at: input.bounds[end],
            plane: true,
        };
        out.push(location(&support(0), &support(1), axis, None, input));
    }
    let specified: BTreeSet<_> = input.specified.iter().map(|r| r.id.clone()).collect();
    out.retain(|r| !specified.contains(&r.id));
    out.extend(input.specified.clone());
    consolidate(out, conveyed, &specified, input.policy.references)
}

fn span(r: &Required) -> Option<(usize, f64, f64)> {
    if let Kind::Location { from, to, axis } = &r.kind {
        Some((
            *axis,
            from.at[*axis].min(to.at[*axis]),
            from.at[*axis].max(to.at[*axis]),
        ))
    } else {
        None
    }
}
fn same(a: &Required, b: &Required) -> bool {
    if a.tolerance != b.tolerance
        || matches!(a.pattern, Some(PatternRole::Pitch { .. }))
            != matches!(b.pattern, Some(PatternRole::Pitch { .. }))
        || (matches!(a.pattern, Some(PatternRole::Pitch { .. })) && a.pattern != b.pattern)
        || a.count != b.count
        || a.reference != b.reference
    {
        return false;
    }
    match (span(a), span(b)) {
        (Some((ka, la, ha)), Some((kb, lb, hb))) => {
            ka == kb
                && near(la, lb)
                && near(ha, hb)
                && match (&a.kind, &b.kind) {
                    (Kind::Location { to: a, .. }, Kind::Location { to: b, .. }) => {
                        a.body == b.body
                    }
                    _ => false,
                }
        }
        _ => a.kind == b.kind && near(a.nominal, b.nominal),
    }
}
fn plane_span(r: &Required) -> Option<(usize, f64, f64)> {
    match &r.kind {
        Kind::Location { from, to, .. }
            if from.plane && to.plane && r.pattern.is_none() && r.reference.is_none() =>
        {
            span(r)
        }
        _ => None,
    }
}
/// Draftwright lint's bounded breadth-first station walk. Point locations and pitch are
/// never edges in this graph; equality of scalar values is not physical determination.
fn path(edges: &[(usize, f64, f64, Id)], target: (usize, f64, f64)) -> Option<Vec<Id>> {
    let (axis, lo, hi) = target;
    let mut queue = vec![(lo, Vec::new())];
    let mut seen = vec![lo];
    let mut cursor = 0;
    while cursor < queue.len() {
        let (at, route) = queue[cursor].clone();
        cursor += 1;
        if near(at, hi) {
            return Some(route);
        }
        for (k, a, b, id) in edges {
            if *k != axis {
                continue;
            }
            let next = if near(*a, at) {
                *b
            } else if near(*b, at) {
                *a
            } else {
                continue;
            };
            if seen.iter().any(|x| near(*x, next)) {
                continue;
            }
            seen.push(next);
            let mut route = route.clone();
            route.push(id.clone());
            queue.push((next, route));
        }
    }
    None
}
fn consolidate(
    mut out: Vec<Required>,
    mut conveyed: Vec<Conveyed>,
    specified: &BTreeSet<Id>,
    references: bool,
) -> (Vec<Required>, Vec<Conveyed>) {
    out.sort_by(|a, b| {
        specified
            .contains(&b.id)
            .cmp(&specified.contains(&a.id))
            .then(a.id.cmp(&b.id))
    });
    let mut kept: Vec<Required> = Vec::new();
    for mut r in out {
        r.covers.sort();
        r.covers.dedup();
        if let Some(owner) = kept.iter_mut().find(|o| same(o, &r)) {
            if owner.id != r.id {
                conveyed.push(Conveyed {
                    id: r.id,
                    by: vec![owner.id.clone()],
                    why: "same physical span and tolerance class; stated once".into(),
                });
            }
            owner.covers.extend(r.covers);
            owner.covers.sort();
            owner.covers.dedup();
        } else {
            kept.push(r);
        }
    }
    // Stated requirements are authorities; generated plane intervals close no new chains.
    let mut edges = Vec::new();
    let mut result = Vec::new();
    for mut r in kept {
        if let Some(target) = plane_span(&r) {
            let eligible = edges
                .iter()
                .filter(|(_, _, _, id)| {
                    result.iter().any(|o: &Required| {
                        &o.id == id
                            && o.tolerance == r.tolerance
                            && match (&o.kind, &r.kind) {
                                (Kind::Location { to: a, .. }, Kind::Location { to: b, .. }) => {
                                    a.body == b.body
                                }
                                _ => false,
                            }
                    })
                })
                .cloned()
                .collect::<Vec<_>>();
            if let Some(by) = path(&eligible, target)
                && !specified.contains(&r.id)
            {
                if references && r.tolerance == Tolerance::Untoleranced {
                    r.reference = Some(by);
                } else {
                    conveyed.push(Conveyed {
                        id: r.id,
                        by,
                        why: "support-plane interval determined by an existing station path".into(),
                    });
                    continue;
                }
            } else {
                edges.push((target.0, target.1, target.2, r.id.clone()));
            }
        }
        result.push(r);
    }
    // Flatten ownership so consumers never need to chase omitted ids.
    conveyed.retain(|c| !result.iter().any(|r| r.id == c.id));
    let map: BTreeMap<_, _> = conveyed
        .iter()
        .map(|c| (c.id.clone(), c.by.clone()))
        .collect();
    fn expand(id: &str, map: &BTreeMap<Id, Vec<Id>>, seen: &mut BTreeSet<Id>, out: &mut Vec<Id>) {
        if !seen.insert(id.into()) {
            return;
        }
        if let Some(ids) = map.get(id) {
            for next in ids {
                expand(next, map, seen, out);
            }
        } else {
            out.push(id.into());
        }
    }
    for c in &mut conveyed {
        let mut by = Vec::new();
        for id in &c.by {
            expand(id, &map, &mut BTreeSet::new(), &mut by);
        }
        by.sort();
        by.dedup();
        c.by = by;
    }
    result.sort_by(|a, b| a.id.cmp(&b.id));
    conveyed.sort_by(|a, b| a.id.cmp(&b.id));
    conveyed.dedup_by(|a, b| a.id == b.id);
    (result, conveyed)
}

#[cfg(test)]
pub(super) fn test_consolidate(
    rs: Vec<Required>,
    ids: &[&str],
    references: bool,
) -> (Vec<Required>, Vec<Conveyed>) {
    consolidate(
        rs,
        vec![],
        &ids.iter().map(|s| s.to_string()).collect(),
        references,
    )
}
#[cfg(test)]
pub(super) fn test_member_location(i: &Input) -> Required {
    locate(
        &i.holes[0].support,
        2,
        None,
        std::slice::from_ref(&i.holes[0].support.feature),
        i,
    )
    .remove(0)
}
