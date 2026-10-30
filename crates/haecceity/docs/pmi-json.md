# PMI JSON reference

## Standards check — 2026-10-30 (issue #142, rule 6 / agreement g)

[ISO 10303-28:2007](https://www.iso.org/standard/40646.html) specifies XML
representations of EXPRESS schemas and data, including EXPRESS-to-XSD mapping;
it is not a JSON binding. Its XML exchange structure is an entity graph.

AP242 edition 4 includes the edition 5 domain model, ISO/TS 10303-4442:2025.
The [ISO domain-model XSD location](https://standards.iso.org/iso/ts/10303/-4442/ed-5/tech/xml-schema/domain_model/)
and [MBx-IF ed4 Domain Model XML practice](https://www.mbx-if.org/home/wp-content/uploads/2025/10/rec_prac_ap242xml_assy_struct_v4.0.pdf)
identify XML as the implemented domain-model binding. The
[AP242 ed4 project](https://atlas.afnet.fr/en/iso-step-ap242-edition-4/)
also lists web services using JSON. That is evidence of JSON work, not enough to
claim an available normative JSON encoding of all EXPRESS or semantic PMI.

A public [experimental EXPRESS JSON encoding, v1.0.0.0, 2026-03-10](https://spatialcompute3d.com/blogs/express_json/express_json.html)
exists. It maps Part 21 entity instances and SELECTs to JSON arrays. It calls
itself experimental and is not an ISO publication or an identified ISO draft.
Searches of ISO's public catalogue, the AP242 project and MBx-IF publications
found no published or publicly accessible draft ISO JSON binding with rules that
could govern this mapping. This is a bounded finding, not a claim that no
unpublished work exists. In particular, ed4 JSON web-service work must not be
confused with a published Part 28 JSON replacement.

We therefore follow the existing AP242 EXPRESS semantic model and
[CAx-IF PMI Representation and Presentation v4.1, 2024-06-20](https://www.mbx-if.org/home/wp-content/uploads/2024/06/rec_pracs_pmi_v41.pdf).
This is an application-level semantic JSON mapping, not ISO Part 28 conformance
or a Part 21 entity-graph serialization. It retains stated decimal tokens,
AP242 names and the practice's groupings. Supplemental geometry alone retains
its typed geometry tree because that is how `PartPmi` represents it.

## Shipping and wire conventions

Canonical files are `crates/haecceity/docs/pmi-json.md` and
`crates/haecceity/schema/haecceity.{pmi,presentation}-1.json`; root `docs/` and
`schema/` links expose them to repository consumers. Cargo packages include the
canonical files. Specify's Python package can copy the two schemas into
`specify_core/schema/` as package data and expose the reference in its documentation;
that separate repository is not modified here. Rust exposes the schema texts as
constants under the opt-in `json` feature, independent of `presentation`.

A document is `{ "schema": "haecceity.pmi/1", "units": { "length": [...],
"angle": [...], "density": [...], "other": [...] }, "pmi": { ... } }`. The machine-readable schemas use JSON
Schema 2020-12. Every model field is explicit, including null optionals and empty
arrays. Unknown properties are errors. Each member of the independently identified semantic
collections is `{ "id": "caller string", "value": <typed value> }`.
One dimension or geometric tolerance is one such item, with all its semantic
content together. References within `pmi` are zero-based indices into the named
collection, independent of stable external item ids. Order is significant.

Enums with no payload are strings; enums with payload are single-key objects.
Keys and enum strings use AP242 entity/enumeration or CAx-IF names wherever the
model supplies a name table. Other model alternatives use lower snake case.
For example a diameter size kind is `"diameter"`, a tolerance kind is
`"position_tolerance"`, and a dimension's kind is
`{"size":{"feature":0,"kind":"diameter","path":null,"angle":null}}`.
The `$defs` enumerate every alternative and field without relying on this prose.

Decimals are strings, including directions, geometry real values and ratios.
`Length.unit`, `Angle.unit` and `Geometry.length_unit` are indices into the
appropriate **document-level** unit table; mixed units retain their original
values without conversion. Unit definitions include named SI/conversion units
and `other` with the stated exact metres/radians factor. `Density.unit` indexes `units.density`, whose definitions retain the model's
derived factors (base-unit names and decimal exponents). Other measured attribute
units index `units.other`, preserving their source unit names. Every unit therefore
lives at document level, including units the model holds only by name.

`FaceIndex` accepts an integer, `{"name":"OCCT face name"}`, or
`{"fingerprint":"support key"}`. Indices use `read_part_definitions().faces`
order, the kernel/Specify snapshot mesh's numbering; they are not STEP entity ids.
Names come from the face's `representation_item.name`, including names written
by OCCT's `write.stepcaf.subshapes.name`. A resolver accepts fingerprint support
keys supplied by the consuming snapshot/correspondence layer; it must not invent
an incompatible fingerprint algorithm. Unknown and ambiguous anchors are refused.
Edges retain zero-based indices. Geometry anchors reference supplemental items.

On STEP export, ids use `PmiRead.identifiers` (`ID_ATTRIBUTE`) where available;
otherwise `collection/index` is deterministic. All ids in a document are unique,
nonempty strings. Input ids are preserved when writing back to STEP; re-reading
that output returns the same ids. Generated ids avoid collisions with file ids.
Validation first applies the shipped schema, then constructors and
`PartPmi::validate`, then resolves topology; STEP writing applies the existing
writer's feature/kind, EXPRESS and rule checks. Every refusal includes its reason.

## Type sources

The table covers every `$defs` semantic type. Names in the left column are the
Rust/model names used as schema definition names. ID wrappers are array indices,
not separate AP242 entities. “PMI” section numbers refer to CAx-IF PMI v4.1;
[Supplemental Geometry v1.3](https://www.mbx-if.org/home/cax/recpractices/)
[User Defined Attributes v1.8](https://www.mbx-if.org/home/wp-content/uploads/2024/05/rec_prac_user_def_attributes_v18.pdf) and
[Material Identification and Density R2.1](https://web.archive.org/web/20240419135511id_/https://www.mbx-if.org/documents/RecPrac_MaterialDensity_v21.pdf) supply the additional practices. Schema-level
wrappers, ids and unit table indices are JSON transport conventions, explicitly
not AP242 entities.

| JSON types | AP242 entities/types represented | Practice |
| --- | --- | --- |
| `Decimal`, `Length`, `Angle`, `Quantity`, `Value`, `Ratio`, `Count`, `Bounds`, `LengthUnit`, `AngleUnit` | length_measure_with_unit, plane_angle_measure_with_unit, measure_representation_item, value_format_type_qualifier, plus_minus_tolerance, tolerance_value, si_unit, conversion_based_unit | PMI §§5.2–5.4 |
| `Anchor`, `Feature`, `GroupKind`, `DerivedKind`, `DerivedGeometry`, `DerivedGeometryKind` | shape_aspect, composite_group_shape_aspect, derived_shape_aspect and subtypes, shape_aspect_deriving_relationship, geometric_item_specific_usage | PMI §§3.8–3.9, 5.1.4, 6.4 |
| `Geometry`, `GeometryItem`, `GeometryValue` | geometric_representation_item and its typed geometry tree, constructive_geometry_representation | PMI §§3.9.1, 9.4.2; Supplemental Geometry §§5.1–5.4 |
| `DatumLabel`, `Datum` | datum, datum_feature, datum_feature_relationship | PMI §6.5 |
| `Placement`, `Direction`, `TargetShape`, `DatumTarget` | axis2_placement_3d, direction, datum_target, placed_datum_target_feature, target parameters, feature_for_datum_target_relationship | PMI §6.6, Tables 10–11 |
| `DatumModifier`, `SimpleDatumModifier`, `DatumModifierType`, `DatumReference`, `Compartment`, `DatumSystem` | datum_reference_modifier, simple_datum_reference_modifier, datum_reference_modifier_with_value, datum_reference_element, datum_reference_compartment, datum_system | PMI §§6.9.7–6.9.8, Table 15 |
| `Dimension`, `DimensionKind`, `SizeKind`, `LocationKind`, `AngleSelection` | dimensional_size, angular_size, dimensional_size_with_path, dimensional_location, angular_location, dimensional_location_with_path, directed_dimensional_location, angle_relator | PMI §5.1, Tables 2–4 |
| `DimTolerance`, `Principle`, `Qualifier`, `DimensionModifier` | shape_dimension_representation, plus_minus_tolerance, limits_and_fits, type_qualifier, descriptive_representation_item | PMI §§5.2–5.3, Tables 5–8 |
| `Iso286Class`, `FundamentalDeviation`, `FitFeature`, `DeviationLetter`, `ToleranceGrade` | limits_and_fits | PMI §5.2.3; ISO 286 |
| `GeometricTolerance`, `ToleranceKind`, `DatumRequirement`, `ToleranceTarget` | geometric_tolerance and the 15 characteristic subtypes, geometric_tolerance_with_datum_reference, toleranced_shape_aspect | PMI §§6.7–6.9, Table 12 (DatumRequirement is the model validation classification) |
| `Zone`, `AffectedPlaneKind`, `ProjectedZone`, `ZoneForm`, `ToleranceModifier`, `UnitBasis`, `UnitArea`, `AreaShape`, `AuxiliaryClassification` | tolerance_zone, tolerance_zone_form, projected_zone_definition, runout_zone_definition, non_uniform_zone_definition, geometric_tolerance_with_modifiers, geometric_tolerance_with_defined_unit, geometric_tolerance_with_maximum_tolerance, unequally_disposed_geometric_tolerance, geometric_tolerance_auxiliary_classification | PMI §§6.9.1–6.9.6, Tables 13–14 |
| `ToleranceRelation`, `RelationKind` | geometric_tolerance_relationship | PMI §6.9.9 |
| `Standard`, `StandardBody`, `GeneralTolerance`, `ToleranceCell`, `CellValue` | applied_document_reference, document, general_property, property_definition, representation, compound_representation_item | PMI §§4–4.1, Figures 1–3 |
| `Thread`, `ThreadSide`, `Hand` | thread, product_definition_shape, shape_aspect, shape_representation_with_parameters, property_definition, measure_representation_item | No thread section in PMI v4.1; AP242 thread WR1–WR16 (existing model extension) |
| `Knurl`, `KnurlPattern` | turned_knurl, product_definition_shape, shape_representation_with_parameters, property_definition, measure_representation_item | No knurl section in PMI v4.1; AP242 turned_knurl WR1–WR12 (existing model extension) |
| `Hole`, `RoundHole` | basic_round_hole, round_hole, shape_aspect, axis2_placement_3d | No hole-definition section in PMI v4.1; AP242 ed4 basic_round_hole / basic_round_hole_occurrence (reader extension) |
| `Material`, `Density` | property_definition, representation, descriptive_representation_item, measure_representation_item, positive_ratio_measure, derived_unit | Material Identification and Density R2.1 §§4.1–4.2 |
| `Note`, `Callout`, `NoteOwner` | property_definition, descriptive_representation_item, draughting_callout, draughting_model_item_association | PMI §§7.3–7.4; UDA |
| `SurfaceTexture`, `MaterialRemoval`, `SurfaceTextureParameter` | property_definition, general_property, surface_texture_representation, measure_representation_item | No surface-texture section in PMI v4.1; ISO 10303-1110 Surface_texture / Standard_surface_texture_parameter, with existing model decisions in step-ap242.md |
| `AttributeSet`, `GeneralProperty`, `AttributeValue` | property_definition, general_property, general_property_association, representation, descriptive_representation_item, measure_representation_item, compound_representation_item | PMI §7.4; UDA §§5–7 |
| `PartPmi`, `PartId`, `FaceIndex`, `EdgeIndex`, `FeatureId`, `DatumId`, `DatumTargetId`, `DimensionId`, `ToleranceId`, `GeometryId`, `DerivedGeometryId` | product_definition, product_definition_shape (part aggregate); references to the types above | PMI §3.8; JSON indexing convention |

## Presentation companion

`haecceity.presentation/1` contains `items`, a map from semantic item id to an
array of graphics, and `unlinked`, an array for graphics with no semantic link.
Each `Graphic` has `source: generated | file`, `name`, model-space `polylines`,
`triangles`, editable `text`, diagnostic `step_ids` and `unsupported` constructs.
Coordinates are millimetres; these display coordinates are JSON numbers rather
than semantic decimal statements. Unsupported graphics are retained as records
with their source ids and a reason, never silently removed.

| JSON type | AP242 entities | Practice |
| --- | --- | --- |
| Presentation document / items / unlinked | draughting_model, draughting_model_item_association | PMI §§7.1–7.3 |
| Graphic with source=file (As Drawn) | draughting_callout, annotation_occurrence, annotation_curve_occurrence, tessellated_annotation_occurrence, annotation_text_occurrence, annotation_plane, geometric_set, tessellated_geometric_set | PMI §§7.1–7.3; Polyline Presentation |
| Graphic with source=generated | same presentation entities, rendered from the semantic model by haecceity::presentation | PMI §§7.1–7.3 |
| polylines / triangles / text | polyline, tessellated_curve_set, triangulated_face, text_literal and literal text | PMI §7.1; Tessellated 3D Geometry |
| source / step_ids / unsupported | transport provenance and explicit diagnostic retention; no additional semantic entity | JSON convention |

Reading includes the file's own graphics. Presentation JSON writing is optional
and is not implemented in version 1; semantic JSON writing uses haecceity's
existing generated presentation. A semantic association may point to several
items, and each associated item keeps the graphic. Unassociated graphics remain
in `unlinked`.

ISO table helper types `Iso2768`, `LinearClass` (`f`, `m`, `c`, `v`) and
`GeometricClass` (`H`, `K`, `L`) map the `limits_and_fits`/general-tolerance
classification (PMI §4.1, ISO 2768). Supplemental `geometry`, computed
`derived_geometry` and `holes` remain plain arrays: they have no independently
identified requirement entity in `PartIdentifiers`; their owning feature has
an external id. All other indexed semantic collections carry `{id,value}`.

`SupportKey` maps the verified geometric namespace used by Specify #65/#77:
`{"fingerprint":{"schema":2,"faces":"<verified face hash>","support":[0]}}`.
It is a JSON support convention over AP242 shape-aspect topology (PMI §3.8), not
an EXPRESS entity. A face anchor has one support face; a feature can contain
several such anchors. `Anchors::bind_support` binds a consumer-verified namespace.
Opaque string support keys are also accepted through `Anchors.fingerprints`.
No new fingerprint algorithm or hash dependency is introduced.

Tolerance relationships use `applied_identification_assignment` with an
`identification_role` named `haecceity.pmi/1 item id`: AP242's
`id_attribute_select` excludes `geometric_tolerance_relationship`, while ed4's
`identification_item` includes it. JSON writes upgrade an ed1 file to the existing
ed4 writer target when this carrier is needed. Other independent items use
`ID_ATTRIBUTE`. The JSON-enabled reader recognizes this assignment role; its
assignment is consumed with the relationship. Ordinary writes keep their existing
schema and bytes. This is a transport convention over the standard assignment
entities, not an extension to the EXPRESS schema.

`LengthUnitDefinition` / `AngleUnitDefinition` / `DensityUnitDefinition` /
`OtherUnitDefinition` name the document-level unit definitions; their corresponding
`*Unit` definitions are integer transport references. AP242 sources are `si_unit`,
`conversion_based_unit`, `derived_unit` / `derived_unit_element`, and the named
unit of a `measure_with_unit` (PMI §§5.2–5.4; Material practice §4.2; UDA §7).

## STEP writer boundary

The JSON mapping is lossless over `PartPmi`; it cannot recover facts the model
never held. STEP writing keeps the existing writer's refusals: for example a
missing nominal value (PMI §5.2.1), a projected zone without its end geometry,
an ambiguous named unit, an unstated group kind, and read-only hole definitions.
Source duplicate identifiers are also an explicit JSON export refusal rather
than silently renumbered. `json::write` verifies the emitted semantic model by
reading it back before returning bytes. It never removes refused requirements.

The strict corpus STEP closure test is separately runnable with `--ignored` so
these pre-existing refusals remain visible. The local SFA sample test, also
explicitly invoked, records any supported-subset exclusions in adjacent audit
files; such a sample does not establish a complete source round trip.
