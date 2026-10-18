//! Bind analytic derivations to STEP faces and cross-check supplemental geometry.
use super::*;
use crate::geom::V3;
use crate::pmi::derived::{self, Explicit, Surface, SurfaceKind};

impl Reader<'_> {
    pub(super) fn derived_geometry(
        &mut self,
        st: &mut PartState,
        aspect: u64,
        from: &[FeatureId],
    ) -> Option<DerivedGeometryId> {
        let mut faces = BTreeMap::new();
        let mut seen = BTreeSet::new();
        let result = (|| {
            for &f in from {
                self.deriving_faces(st, f, &mut seen, &mut faces)?;
            }
            let mut tolerance = derived::DISTANCE_TOLERANCE;
            let mut surfaces = Vec::new();
            for (&face, &rep) in &faces {
                tolerance = tolerance.max(self.geometry_tolerance(rep));
                surfaces.push(self.deriving_surface(st.part.faces[face.0], rep)?);
            }
            let geometry = derived::derive(&surfaces, tolerance)?;
            // Intermediate derived aspects have already passed their own cross-checks.
            let mut explicit = Vec::new();
            for (_, rep, items) in self.usages(aspect) {
                let rep = rep.ok_or("explicit geometry has no representation")?;
                tolerance = tolerance.max(self.geometry_tolerance(rep));
                for item in items {
                    if !self.is(rep, "constructive_geometry_representation") {
                        return Err(format!(
                            "explicit item #{item} is not in a constructive_geometry_representation"
                        ));
                    }
                    if !self
                        .get_refs(rep, "representation", "items")
                        .contains(&item)
                    {
                        return Err(format!(
                            "explicit item #{item} is not a member of its constructive geometry representation #{rep}"
                        ));
                    }
                    let shape = self.explicit_derived_item(item, rep)?;
                    geometry
                        .cross_check(shape, tolerance)
                        .map_err(|why| format!("#{item}: {why}"))?;
                    explicit.push((item, rep));
                }
            }
            Ok((geometry, explicit))
        })();
        let (g, explicit) = match result {
            Ok(g) => g,
            Err(why) => {
                let mut named: Vec<u64> = faces.keys().map(|f| st.part.faces[f.0]).collect();
                for (usage, _, items) in self.usages(aspect) {
                    named.push(usage);
                    named.extend(items);
                }
                self.find(
                    if why.contains("disagrees") { FindingKind::Conflict } else { FindingKind::NotModelled },
                    Some(st.id), aspect, &named,
                    format!("derived geometry refused: {why}; derivation relationships and semantic values are retained"),
                );
                return None;
            }
        };
        let mut supplemental = Vec::new();
        for (item, rep) in explicit {
            match self.geometry(st, item, Some(rep)) {
                Ok(id) => supplemental.push(id),
                Err(why) => {
                    self.find(
                        FindingKind::NotModelled,
                        Some(st.id),
                        aspect,
                        &[item],
                        format!("derived geometry refused: {why}"),
                    );
                    return None;
                }
            }
        }
        let decimal = |x: f64| Decimal::parse(&x.to_string()).expect("finite computed coordinate");
        let length = |x| Length {
            value: decimal(x),
            unit: LengthUnit::Millimetre,
        };
        let id = DerivedGeometryId(st.pmi.derived_geometry.len());
        st.pmi.derived_geometry.push(DerivedGeometry {
            kind: g.kind,
            deriving_faces: faces.into_keys().collect(),
            point: g.point.map(length),
            direction: Direction(g.direction.map(decimal)),
            radius: g.radius.map(length),
            supplemental,
        });
        Some(id)
    }

    /// Follow the feature graph through groups and derivation chains, keeping each face
    /// once. The defining usage supplies the face's own representation context and units.
    fn deriving_faces(
        &mut self,
        st: &PartState,
        feature: FeatureId,
        seen: &mut BTreeSet<FeatureId>,
        faces: &mut BTreeMap<FaceIndex, u64>,
    ) -> Result<(), String> {
        if !seen.insert(feature) {
            return Ok(());
        }
        match &st.pmi.features[feature.0] {
            Feature::Items(items) => {
                for item in items {
                    let Anchor::Face(face) = item else {
                        return Err("deriving aspects must resolve exclusively to part faces (found an edge or supplemental item)".into());
                    };
                    let face_id = st.part.faces[face.0];
                    let mut representations = BTreeSet::new();
                    for &u in &st.prov.features[feature.0] {
                        if self.is(u, "item_identified_representation_usage")
                            && self
                                .get_refs(
                                    u,
                                    "item_identified_representation_usage",
                                    "identified_item",
                                )
                                .contains(&face_id)
                            && let Some(rep) = self.get_ref(
                                u,
                                "item_identified_representation_usage",
                                "used_representation",
                            )
                        {
                            representations.insert(rep);
                        }
                    }
                    let rep = *representations.first().ok_or_else(|| {
                        format!("deriving face #{face_id} has no representation context")
                    })?;
                    let unit = self
                        .context_length_unit(rep)
                        .ok_or_else(|| format!("deriving face #{face_id} has no length unit"))?;
                    for other in representations.into_iter().chain(faces.get(face).copied()) {
                        if self.context_length_unit(other) != Some(unit.clone()) {
                            return Err(format!(
                                "deriving face #{face_id} has conflicting representation units"
                            ));
                        }
                    }
                    faces.insert(*face, rep);
                }
            }
            Feature::Derived { geometry: None, .. } => {
                return Err(format!(
                    "deriving feature {} has refused or unsupported derived geometry",
                    feature.0
                ));
            }
            Feature::Group { members, .. } | Feature::Derived { from: members, .. } => {
                for &m in members {
                    self.deriving_faces(st, m, seen, faces)?;
                }
            }
        }
        Ok(())
    }

    fn deriving_surface(&mut self, face: u64, rep: u64) -> Result<Surface, String> {
        let mut surface = self
            .get_ref(face, "face_surface", "face_geometry")
            .ok_or_else(|| format!("deriving face #{face} has no surface"))?;
        for _ in 0..16 {
            if !self.is(surface, "rectangular_trimmed_surface") {
                break;
            }
            surface = self
                .get_ref(surface, "rectangular_trimmed_surface", "basis_surface")
                .ok_or("trimmed surface without a basis surface")?;
        }
        let unit = self
            .context_length_unit(rep)
            .ok_or("deriving face context has no length unit")?;
        if !unit.millimetres().is_finite() || unit.millimetres() <= 0.0 {
            return Err(format!("surface #{surface} has an invalid length unit"));
        }
        let kind = if self.is(surface, "cylindrical_surface") {
            let a = self
                .get(surface, "cylindrical_surface", "radius")
                .ok_or("cylinder without a radius")?;
            let r = self
                .real_text(surface, a)
                .ok_or("cylinder radius is not numeric")?;
            SurfaceKind::Cylinder(
                Decimal::parse(&r).map_err(|e| e.0)?.to_f64() * unit.millimetres(),
            )
        } else if self.is(surface, "conical_surface")
            || self.is(surface, "toroidal_surface")
            || self.is(surface, "surface_of_revolution")
        {
            SurfaceKind::Revolution
        } else if self.is(surface, "plane") {
            SurfaceKind::Plane
        } else {
            return Err(format!(
                "deriving face #{face} has unsupported surface #{surface} ({})",
                self.entity_label(surface)
            ));
        };
        let placement = if self.is(surface, "surface_of_revolution") {
            self.get_ref(surface, "surface_of_revolution", "axis_position")
        } else {
            self.get_ref(surface, "elementary_surface", "position")
        }
        .ok_or_else(|| format!("surface #{surface} has no axis placement"))?;
        let (point, direction) = self.derived_placement(placement, unit)?;
        Ok(Surface {
            kind,
            point,
            direction,
        })
    }

    fn derived_point(&mut self, point: u64, unit: &LengthUnit) -> Result<V3, String> {
        let Some(Attribute::List(coords)) = self.get(point, "cartesian_point", "coordinates")
        else {
            return Err(format!("#{point} is not a Cartesian point"));
        };
        if coords.len() != 3 {
            return Err(format!("#{point} is not a 3D Cartesian point"));
        }
        let mut out = [0.; 3];
        for (i, a) in coords.iter().enumerate() {
            let t = self
                .real_text(point, a)
                .ok_or("point coordinate is not numeric")?;
            out[i] = Decimal::parse(&t).map_err(|e| e.0)?.to_f64() * unit.millimetres();
        }
        if out.iter().any(|v| !v.is_finite()) {
            return Err(format!("#{point} has non-finite coordinates"));
        }
        Ok(out)
    }

    fn derived_placement(&mut self, placement: u64, unit: LengthUnit) -> Result<(V3, V3), String> {
        let location = self
            .get_ref(placement, "placement", "location")
            .ok_or("placement without a location")?;
        let point = self.derived_point(location, &unit)?;
        let entity = if self.is(placement, "axis1_placement") {
            "axis1_placement"
        } else if self.is(placement, "axis2_placement_3d") {
            "axis2_placement_3d"
        } else {
            return Err(format!("#{placement} is not a 3D axis placement"));
        };
        let direction = match self.get(placement, entity, "axis") {
            Some(Attribute::EntityRef(d)) => self
                .direction(*d)
                .ok_or("placement axis is not a 3D direction")?
                .0
                .each_ref()
                .map(Decimal::to_f64),
            Some(Attribute::Unset) => [0., 0., 1.],
            _ => return Err("placement axis is missing or not a direction reference".into()),
        };
        let direction =
            crate::geom::unit(direction).ok_or("placement axis is zero or non-finite")?;
        Ok((point, direction))
    }

    fn explicit_derived_item(&mut self, item: u64, rep: u64) -> Result<Explicit, String> {
        let unit = self
            .context_length_unit(rep)
            .ok_or_else(|| format!("explicit geometry #{item} has no length unit"))?;
        if self.is(item, "cartesian_point") {
            return self.derived_point(item, &unit).map(Explicit::Point);
        }
        if self.is(item, "axis1_placement")
            || self.is(item, "axis2_placement_3d")
            || self.is(item, "plane")
        {
            let plane = self.is(item, "plane");
            let placement = if plane {
                self.get_ref(item, "elementary_surface", "position")
                    .ok_or("explicit plane without a placement")?
            } else {
                item
            };
            let (point, direction) = self.derived_placement(placement, unit)?;
            return Ok(if plane {
                Explicit::Plane {
                    point,
                    normal: direction,
                }
            } else {
                Explicit::Placement { point, direction }
            });
        }
        if self.is(item, "line") {
            let p = self
                .get_ref(item, "line", "pnt")
                .ok_or("explicit line without a point")?;
            let v = self
                .get_ref(item, "line", "dir")
                .ok_or("explicit line without a vector")?;
            let d = self
                .get_ref(v, "vector", "orientation")
                .ok_or("explicit vector without a direction")?;
            let direction = self
                .direction(d)
                .ok_or("explicit line direction is not 3D")?
                .0
                .each_ref()
                .map(Decimal::to_f64);
            let magnitude = self
                .get(v, "vector", "magnitude")
                .and_then(|a| self.real_text(v, a))
                .and_then(|t| Decimal::parse(&t).ok())
                .map(|d| d.to_f64());
            if !magnitude.is_some_and(|m| m.is_finite() && m > 0.) {
                return Err("explicit line vector has invalid magnitude".into());
            }
            return Ok(Explicit::Line {
                point: self.derived_point(p, &unit)?,
                direction,
            });
        }
        Err(format!(
            "explicit geometry #{item} ({}) is not a supported point, axis placement, line or plane cross-check",
            self.entity_label(item)
        ))
    }

    fn geometry_tolerance(&mut self, rep: u64) -> f64 {
        let mut tolerance = derived::DISTANCE_TOLERANCE;
        if let Some(ctx) = self.get_ref(rep, "representation", "context_of_items") {
            for u in self.get_refs(ctx, "global_uncertainty_assigned_context", "uncertainty") {
                if let Ok(Measured::Length(l)) = self.measure(u)
                    && l.mm().is_finite()
                    && l.mm() > 0.
                {
                    tolerance = tolerance.max(l.mm());
                }
            }
        }
        tolerance
    }
}
