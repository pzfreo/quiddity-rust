# AP242 and semantic PMI in haecceity

**Status:** design (2026-10-08, revised after review), not yet implemented. Branch `ap242`.

haecceity reads STEP geometry today (`crates/haecceity/src/step.rs`, through step-io's typed
model). This document adds what specify-core and draftwright still need OpenCascade for: reading
the semantic PMI a file carries, writing semantic PMI as AP242, and editing the PMI of a file
while leaving everything else in it byte for byte.

The authorities are the standards, not OpenCascade: ISO 10303-242 and its EXPRESS schema, the
CAx-IF *Recommended Practices for the Representation and Presentation of PMI (AP242)* v4.1
(2024-06-20, "the PMI practice") and *for User Defined Attributes* v1.8 ("the UDA practice"), and
the GPS/GD&T semantics they encode (ISO 1101, ISO 5459, ISO 286, ISO 2768, ASME Y14.5).
OpenCascade XCAF (`STEPCAFControl`) is a cross-check with a verdict for every difference.
specify-core's current PMI handling is shaped by OpenCascade's round-trip defects; none of that
shape is inherited (see [Anti-requirements](#anti-requirements)).

## Maintainer decisions (2026-10-08)

These override anything below and any stream brief that says otherwise.

1. **The schema follows the PMI** (revised 2026-10-08). A file that gains no PMI keeps its
   `FILE_SCHEMA` and its bytes: an AP214 or AP203 file is never rewritten as AP242 merely
   because haecceity read or touched it. A file to which PMI is added is written as AP242, since
   AP214 and AP203 cannot carry semantic PMI: its `FILE_SCHEMA` becomes the target AP242
   edition's identifier, and that claim must be true, so every instance of the edited file must
   pass `validate_document` against the target table; otherwise the edit is refused, naming the
   violating instances (never repaired silently). An AP242 file stays AP242. Which corpus and
   NIST AP214/AP203 files pass is pinned per file with the violations and reasons, and reported
   to the maintainer if many fail.
2. **Material as specify-core writes it.** The writer writes the material's name in the CAx-IF
   'material name' construct exactly as specify-core's files carry it (a `REPRESENTATION('material
   name', …)` whose `DESCRIPTIVE_REPRESENTATION_ITEM` holds the name, related to the part as
   OpenCascade's XCAF writes it; take the exact entity structure from a specify-core-written
   file, and validate it against the schema). No density is written (specify-core writes none:
   OpenCascade writes its unit wrongly). The reader reads the material name, and a density when
   a file states one, with its own unit. `Material` is therefore in scope for reading and
   writing; it is not "undetermined".
3. **General tolerance as a class.** The general tolerance is written as specify-core writes it:
   the CAx-IF PMI practice's `PROPERTY_DEFINITION('default tolerances', …)` on the part's
   `product_definition_shape`, with a `REPRESENTATION('default tolerances', …)` whose
   `DESCRIPTIVE_REPRESENTATION_ITEM('tolerance class', …)` holds the class as stated, e.g.
   `'ISO 2768-m'` or `'ISO 2768-mK'` (the ISO 2768-2 geometric class needs nothing more). The
   model's `GeneralTolerance` is `Class { text, standard: Option<Iso2768 { linear: f|m|c|v,
   geometric: Option<H|K|L> }> }` (the text kept as stated, the standard recognised from it), or
   `Table { … }` for a `default_tolerance_table` a file carries (read, not written).

Schema references below are to the AP242 MIM long form `242_mim_lf.exp` (WG12 N11521, from
stepcode); section numbers (§) are the PMI practice's unless stated.

## Requirements

What the two consumers do with STEP today, restated in standards terms. *specify-core* is
`/Users/paul/repos/specify-core/src/specify_core`; *draftwright* is `src/draftwright` on
draftwright's `origin/main`.

| # | Need (standards terms) | Where it comes from |
|---|---|---|
| R1 | Read a part's B-rep with a stable numbering of its faces, per distinct part of an assembly (a part placed twice is one part with two placements), and know which `advanced_face` instance each face is. | specify-core `load.py` (`load_all`, `parts`, `face_ranks`), `requirements.face_entities_of` |
| R2 | Read the product structure: product definitions, their names, the placements of their occurrences. | `load.py` (`_part_name`, `_instances`), `mates.py` |
| R3 | Read the semantic PMI already in a file and anchor every item to the part's faces: datums, datum features and datum targets, dimensions (size and location, nominal, ± deviations, limits, ISO 286 class), geometric tolerances (type, magnitude, zone, modifiers, datum system, what they apply to), tolerance relationships (composite). Every item on its own part. | `existing.py` (`read_existing`, `datum_faces`, `notes`, `part_settings`); `magnitudes.py`; draftwright `pmi.py`, `_pmi_part21.py` |
| R4 | Read part-level information: material (name, density), general tolerances, default decimal places, threads and knurls, notes and requirements, user defined attributes. | `existing.part_settings`, `existing.notes`; draftwright `read_material_properties`, `read_manufacturing_requirements`, `read_structured_manufacturing_requirements`, `read_surface_labels` |
| R5 | Read display facts that are semantic: decimal places of a value (`value_format_type_qualifier`, §5.4) and of the part (§4.1), basic dimensions, dimension modifiers. | draftwright `read_dimension_display_facts`, `dimension_basic_policy`; specify-core `locations.mark_basic` |
| R6 | Write, for each part of a file (all parts in one edit), the PMI of specify-core v1: datums A–H on one face, coplanar faces, or a hole or boss; size dimensions with ± deviations, limits or an ISO 286 fit; location dimensions (toleranced or basic); position (Ø, Ⓜ), flatness, perpendicularity, parallelism, profile of a surface, circular and total runout, each with its datum system; material; the general tolerance; internal and external threads; straight and diamond knurls; specify-core's own data (tapping drill, depths, answers). | `writer.py` (`_add`, `write_parts`), `requirements.py`, `rules.py` |
| R7 | Edit an existing file: add PMI to a part that has none or some, replace the PMI of a part, remove it; keep every other byte of the file. | `merge.py` (`transplant`), `writer._write_verified`, `resume.py` |
| R8 | Store application data on a part or feature in a standard construct. | `resume.embed` |
| R9 | Verify a write by reading it back with the same reader, semantically. | `writer.verify`, `writer._verify_appended` |
| R10 | Report what cannot be read rather than drop it: unknown or unsupported PMI entities, references that do not resolve, values without units. | draftwright `PmiExtractionReport`, `lint_pmi_extraction` |

Not needed by either consumer from the PMI layer: graphic presentation (specify-core writes
datum feature symbols only so that presentation-only viewers show something; draftwright draws
from semantics and reads no presentation except "common labels"), saved views, tessellated
presentation. specify-core's `mesh` command (a face-indexed tessellation for its picker) also
uses OpenCascade; it is not PMI and is listed under [Out of scope](#out-of-scope-for-now).

## Semantic model

A plain Rust data model in `crates/haecceity/src/pmi/model.rs`, independent of Part 21: no
type in it holds a file instance id. One reader maps AP242 to it, one writer maps it to AP242.

### Values and units

A file states each value as a decimal in a unit; converting it to millimetres in `f64` and back
does not reproduce it (of the three-decimal inch values 0.001–4.999, 670 change under
`x * 25.4 / 25.4`, e.g. 0.003 → 0.0030000000000000005). So the model keeps the value **as
stated**, and conversion is a view, never storage:

```rust
pub struct Decimal(/* the decimal text, validated: sign, digits, '.', optional exponent */);
pub enum LengthUnit { Millimetre, Micrometre, Centimetre, Metre, Inch, Foot, Other { name: String, metres: Decimal } }
pub enum AngleUnit  { Radian, Degree, Other { name: String, radians: Decimal } }
pub struct Length { pub value: Decimal, pub unit: LengthUnit }   // .mm() -> f64 for comparison
pub struct Angle  { pub value: Decimal, pub unit: AngleUnit }    // .rad() -> f64
pub enum Quantity { Length(Length), Angle(Angle) }
pub struct Value  { pub quantity: Quantity, pub decimal_places: Option<u8> } // §5.4 qualifier
pub struct Ratio(pub Decimal);  pub struct Count(pub u32);
```

The reader builds a `Decimal` from the Part 21 REAL token exactly as written (`0.0030` stays
`0.0030`) and the unit from the `measure_with_unit`'s own unit entity (`si_unit` with prefix,
`conversion_based_unit` such as INCH or DEGREE; `Other` for a conversion factor it does not know,
keeping the factor as stated). Values a consumer creates are decimals in the unit the consumer
states (specify-core: millimetres). The writer writes each value **in its own unit, with its own
digits**: it references a unit instance of the part's context equal to the value's unit, or adds
one (SI millimetre, `conversion_based_unit` inch, degree, …). Nothing is converted on write, so a
read-then-replace of an inch file writes every value's text unchanged, and no value can take its
unit from another. Semantic equality compares quantities (`mm()`, `rad()`) to 1e-12 relative;
text equality is checked separately where the tests require it.

### Part PMI

```rust
pub struct PartPmi {                            // the PMI of one product definition of the file
    pub part: PartId,                           // index into read_part_definitions' parts
    pub standard: Option<Standard>,             // §4: ISO or ASME document and its principle
    pub decimal_places: Option<u8>,             // §4.1 default tolerance decimal places
    pub features: Vec<Feature>,                 // shape aspects: what PMI applies to
    pub datum_targets: Vec<DatumTarget>,
    pub datums: Vec<Datum>,
    pub dimensions: Vec<Dimension>,
    pub tolerances: Vec<GeometricTolerance>,
    pub tolerance_relations: Vec<ToleranceRelation>,
    pub general: Vec<GeneralTolerance>,
    pub threads: Vec<Thread>,
    pub knurls: Vec<Knurl>,
    pub material: Option<Material>,
    pub notes: Vec<Note>,
    pub attributes: Vec<AttributeSet>,          // UDA practice, on the part or on a feature
}
```

**Anchoring.** `pub enum Anchor { Face(FaceIndex), Edge(EdgeIndex) }`. A `FaceIndex` is the
part's face number in `read_part_definitions`' numbering (OpenCascade's `TopExp::MapShapes`
order over the part's solid, which specify-core uses); an edge likewise. The binding to a
file instance (`#N` of the `advanced_face` / `edge_curve`) is not in the model: the reader
resolves `#N → index` and the writer `index → #N` through the face provenance of the same
document (Architecture item 3). An anchor the provenance cannot resolve is a finding on read and
a refusal on write. `PartId` likewise indexes the document's distinct parts; the model never
creates product definitions.

```rust
pub enum Feature {
    Items(Vec<Anchor>),                                  // one feature of one or more items (§6.5.1)
    Group { members: Vec<FeatureId>, kind: GroupKind },  // multiple elements, pattern of features,
                                                         // all around, between (§6.4)
    Derived { kind: DerivedKind, from: Vec<FeatureId> }, // explicit derived shape (§5.1.4, Table 3)
}
```

There is no "whole part" feature: `product_definition_shape` is not a `shape_aspect`; a tolerance
on the whole part is a `ToleranceTarget::WholePart` (§6.3).

**Datums, datum features and targets** (§6.5, §6.6; schema `datum` WR1–WR4, `datum_target`):

```rust
pub struct Datum { pub label: DatumLabel, pub feature: Option<FeatureId>, pub targets: Vec<DatumTargetId> }
pub struct DatumTarget {
    pub number: u32,                        // target_id > 0 (WR3); 'A1' is datum A, target 1
    pub shape: TargetShape,                 // Point | Line{length} | Rectangle{length, width}
                                            // | Circle{diameter} | CircularCurve{diameter} | Area(FeatureId)
    pub placement: Option<Placement>,       // placed targets: location and axes (§6.6.2)
    pub movable: Option<Direction>,         // 'movable direction' (§6.6.4)
    pub on: Option<FeatureId>,              // feature_for_datum_target_relationship, at most one (§6.6.3)
}
pub struct DatumSystem(Vec<Compartment>);   // primary, secondary, tertiary, in order (1..=3)
pub struct Compartment { pub references: Vec<DatumReference>, pub modifiers: Vec<DatumModifier> }
pub struct DatumReference { pub datum: DatumId, pub modifiers: Vec<DatumModifier> }
```

Invariant: a datum is established by its feature, its targets, or both, never by neither, and by
at most one feature (`datum` WR1–WR2). A `Feature` used by a datum *is* the datum feature: the
writer writes it as the `datum_feature` (not a `shape_aspect` beside it), and tolerances and
dimensions on that feature reference the `datum_feature` instance, so no duplicate shape aspect
exists. The reader maps a `datum_feature` and the tolerances that reference it back to one
`Feature`. Several datums in one compartment are a common datum (A–B, §6.9.8).

**Dimensions** (§5):

```rust
pub struct Dimension {
    pub kind: DimensionKind,       // Size{feature, SizeKind} | Location{from, to, LocationKind, path, directed}
    pub nominal: Value,            // Length or Angle by kind
    pub tolerance: DimTolerance,
    pub qualifier: Option<Qualifier>,           // maximum, minimum, average (§5.2.2)
    pub modifiers: Vec<DimensionModifier>,      // Tables 7–8
}
pub enum DimTolerance {
    None,
    Basic,                                          // theoretically exact (§5.3)
    Deviations { upper: Value, lower: Value },      // signed offsets from nominal (§5.2.3)
    Limits { upper: Value, lower: Value },          // value range (§5.2.4)
    Fit { class: Iso286Class, limits: Option<(Value, Value)> }, // §5.2.5, optionally with the
                                                    // stated limits beside it: Ø20 H7 (20.000/20.021)
}
pub struct Iso286Class { pub deviation: FundamentalDeviation, pub grade: ToleranceGrade }
```

`FundamentalDeviation` is one enum of A…ZC and a…zc (case gives hole or shaft); `ToleranceGrade`
is IT01…IT18. Deviations and limits hold `upper > lower` with no sign assumed (g6, f7 lie wholly
below nominal).

**Geometric tolerances** (§6.7–§6.9):

```rust
pub struct GeometricTolerance {
    pub kind: ToleranceKind,
    pub target: ToleranceTarget,
    pub magnitude: Option<Value>,       // schema: OPTIONAL length_measure_with_unit
    pub zone: Option<Zone>,             // form (Ø, sphere, …), projected, affected plane,
                                        // non-uniform, runout orientation (§6.9.2)
    pub modifiers: Vec<ToleranceModifier>,  // Ⓜ Ⓛ Ⓕ Ⓣ Ⓟ Ⓤ, statistical, … (§6.9.3)
    pub unit_basis: Option<UnitBasis>,  // §6.9.6
    pub maximum: Option<Value>,         // §6.9.5
    pub unequal: Option<Value>,         // §6.9.4
    pub datums: Option<DatumSystem>,
}
pub enum ToleranceTarget {              // schema geometric_tolerance_target (SELECT, line 4042)
    Feature(FeatureId),                 // shape_aspect
    Dimension(DimensionId),             // dimensional_size / dimensional_location: a feature of size
    Relation { relating: FeatureId, related: FeatureId, name: String }, // shape_aspect_relationship
    WholePart,                          // product_definition_shape (§6.3 "applies to all surfaces")
}
pub struct ToleranceRelation {          // schema geometric_tolerance_relationship
    pub kind: RelationKind,             // Composite | Precedence | Simultaneity
    pub relating: ToleranceId, pub related: ToleranceId,
}
```

Datum references by `ToleranceKind`, from the schema's subtype structure (a test checks this
table against the generated schema table):

| Kinds | Schema | Datums |
|---|---|---|
| angularity, perpendicularity, parallelism, circular runout, total runout, symmetry, concentricity, coaxiality | `SUBTYPE OF (geometric_tolerance_with_datum_reference)` | required |
| position, line profile, surface profile | `SUBTYPE OF (geometric_tolerance)`; datums by the complex form with `geometric_tolerance_with_datum_reference` | optional |
| straightness, flatness, roundness, cylindricity | WR1: `NOT (… GEOMETRIC_TOLERANCE_WITH_DATUM_REFERENCE IN TYPEOF(SELF))` | forbidden |

A `Composite` relation requires both tolerances to be position, both line profile or both
surface profile, with the same target (`geometric_tolerance_relationship` WR1–WR2), and each
tolerance has at most one composite relation (`geometric_tolerance` WR5).

**General tolerances.** A class, as stated, written in the PMI practice's 'default tolerances'
/ 'tolerance class' construct (decision 3): `GeneralTolerance::Class { text, standard }`, where
`standard` recognises ISO 2768 from the text (linear class f, m, c or v; geometric class H, K or
L when given, as in `ISO 2768-mK`). A `default_tolerance_table` a file carries is read as
`GeneralTolerance::Table { name, cells }` and reported; it is not written. `standards.rs` holds
ISO 2768-1's table so a consumer can evaluate a class.

**Threads and knurls** are exactly their schema parameters (`thread` WR1–WR16,
`turned_knurl` WR1–WR12), typed:

```rust
pub struct Thread {
    pub feature: FeatureId,               // 'applied shape' (WR13)
    pub partial_area: Option<FeatureId>,  // 'partial area occurrence' (WR12)
    pub side: ThreadSide,                 // 'thread side': internal | external (=1)
    pub major_diameter: Length,           // (=1)
    pub minor_diameter: Option<Length>,   // (<=1)
    pub pitch_diameter: Option<Length>,   // (<=1)
    pub number_of_threads: Ratio,         // ratio_measure_with_unit (=1)
    pub form: String,                     // descriptive (=1)
    pub fit_class: String,                // (=1)
    pub fit_class_2: Option<String>,      // (<=1)
    pub hand: Hand,                       // 'left' | 'right' (=1)
    pub crest: Option<Length>,            // (<=1)
    pub qualifier: Option<String>,        // (<=1)
    pub nominal_size: Option<Length>,     // (<=1)
    pub runout: Option<FeatureId>,        // 'thread runout' (WR16)
}
pub struct Knurl {
    pub feature: FeatureId,
    pub pattern: KnurlPattern,            // description: diamond | diagonal | straight (WR1)
    pub major_diameter: Length, pub nominal_diameter: Length, pub diametral_pitch: Length,
    pub number_of_teeth: Option<Count>, pub tooth_depth: Option<Length>, pub root_fillet: Option<Length>,
    pub helix_angle: Option<Angle>,       // required for diamond and diagonal (WR9)
    pub helix_hand: Option<Hand>,         // required for diagonal (WR10)
}
```

The schema defines no pitch item: what 'number of threads' (a ratio) means (threads per unit
length or starts) is settled by the reader stage from ISO 10303-242's definition text and the
files that carry threads, and recorded here; pitch, if derivable, is a function, not a field.
Tapping drill diameter and depth and full-thread depth are not thread semantics: they are an
`AttributeSet` on the thread's feature (UDA practice).

**Material** (`Material { name, density: Option<Density> }`): decision 2. The name is written in
the 'material name' construct as specify-core's files carry it; density is read when stated
(with its own unit) and not written.

**Notes and attributes.** `Note { text, on: Option<FeatureId> }` for descriptive requirements;
`AttributeSet { name, on: AttributeOwner (Part | Feature), items: Vec<(String, AttributeValue)> }`
per the UDA practice §5–7.

**Invariants**, checked by constructors and by the reader (a violation is a reported finding,
never a silent fix):

- Ids (`FeatureId`, `DatumId`, `DimensionId`, `ToleranceId`, …) index this `PartPmi`'s own
  vectors; every reference resolves.
- A datum has no position: precedence exists only in a `DatumSystem`. Labels are unique within a
  part; the same letter on two parts is two datums.
- A `DatumSystem` is a value: two equal systems are one system (§6.9.7).
- Datums are established as stated above; datum targets of one datum have distinct numbers.
- `Deviations`, `Limits` and a fit's limits have `upper > lower`; no sign is assumed.
- `Iso286Class` is a pair of enums: no other spelling exists.
- Datum references follow the table above; composite relations follow its rules.
- Every `Value`'s quantity is fixed by its role (a dimension's kind, a tolerance's magnitude is
  a length, …), and every value carries its own unit.

## Architecture

```
            bytes ──▶ p21::Document ─────────────────────────────────────▶ bytes
                       (every instance's byte range; edits)               (untouched bytes copied)
                          │                          ▲
     step.rs (B-rep) ◀────┤                          │ Edit (add / replace / remove)
     + face provenance    │                          │
                          ▼                          │
                     pmi::read ──▶ [PartPmi] ──▶ pmi::write ──▶ express::validate (backstop)
                     (+ findings,                (typed emission;     + express_rules
                      consumed ids)               removal plan)
```

1. **`p21`: a lossless Part 21 document.** Parses a file once into its instances, each with its
   id, its parsed record (step-io's `parser::RawEntity`) and the exact byte range of its text
   (from `#` through `;`), plus the header and section boundaries; checks the ids agree with
   step-io's graph. An `Edit` holds additions (provisional ids), replacements (same id, new
   record) and removals; applying one refuses a removal still referenced by a kept instance and
   a reference to nothing. Writing copies every untouched byte unchanged, writes a replaced
   instance in place, drops a removed one with its line, and appends additions before the
   `ENDSEC` of the last DATA section, numbered from the file's largest id upward in a
   deterministic order, in the file's own line ending; strings use Part 21 escapes
   (`\X2\…\X0\`), valid in every edition. A complex record is built only through a `Complex`
   type that holds its leaves sorted by name (the external mapping's order); `encode` refuses a
   raw complex record whose parts are not in that order. With no edit, output equals input.
2. **`express`: the AP242 schema as data.** A table generated from the EXPRESS long form by
   `tools/express_table.py` (source and sha256 recorded). `validate(record)` checks a simple or
   complex instance (names, attribute count and types, typed SELECT members, enumerations,
   reference targets, aggregate bounds, legal and minimal complex sets); `validate_document`
   runs it over every instance. The table also classifies every entity type into a *family*
   (semantic PMI, presentation, validation property, product/shape/geometry/other) from its
   supertypes, reviewed once and committed. `express_rules` checks by name the WHERE and UNIQUE
   rules the writer depends on (`thread` WR1–WR16, `turned_knurl` WR1–WR12,
   `default_tolerance_table` WR1–WR2 and cell WR1–WR5, `datum` WR1–WR4, `datum_target`
   WR1–WR5, `geometric_tolerance` WR1/WR5, `geometric_tolerance_relationship` WR1–WR2,
   `datum_system` UR1, `plus_minus_tolerance` UR1), each citing its label.
   **Editions.** The writer targets exactly the edition the express table represents (one table
   per edition if other long forms are found). The file's schema follows decision 1: unchanged
   unless PMI is added; an AP214/AP203 file gaining PMI becomes AP242 only if every instance
   validates against the target table, else the edit is refused naming the violations. For an
   AP242 file, the edit is refused if it introduces a violation the original did not have;
   violations the original already had are reported, not repaired.
3. **Face provenance** in `step.rs`: each `Part` face and edge records its `advanced_face` /
   `edge_curve` id and its placed instance. step-io fills each arena in ascending id order over
   the instances it keeps; the reader rebuilds the map from the raw graph and the report's
   dropped ids and checks every face and edge against its record: entity type, bound count,
   surface type and sense, **and one geometric value** (the surface's location point, the edge
   vertices' coordinates) against step-io's typed geometry, refusing on any disagreement. Per
   distinct part, `read_part_definitions` gives its `product_definition`, name, placements and
   faces (and edges) in specify-core's numbering; this is the binding the PMI reader and writer
   resolve anchors through.
4. **`pmi::read`** walks the `p21` records (step-io's typed model drops `thread`,
   `turned_knurl`, `runout_zone_definition`, `default_tolerance_table`) **inversely from each
   part**: the *roots* of a part's PMI are the semantic-PMI-family instances that reference its
   `product_definition_shape` (shape aspects by `of_shape`, tolerances on the whole part,
   property definitions) or its `product_definition` (UDA and property constructs); from them it
   follows references to anchors, values and units, and inverse references to the items built on
   them (datums on datum features, tolerances on shape aspects and dimensions, relationships). It
   returns `PmiRead { parts: Vec<PartPmi>, findings, provenance }`: findings are unknown or
   unsupported PMI entities (by id and type), nonconformances read through and how (NIST
   FTC-10's `LIMITS_AND_FITS('G6','hole','','')`), unresolved references, unitless measures,
   PMI on an assembly product definition or an occurrence path (out of scope, never
   misattributed), and constructs in a form the reader does not know. `provenance` lists,
   per part and per model item, the instance ids it consumed. Accounting: every
   semantic-PMI-family instance of the file is consumed or in a finding. Presentation is counted
   as presentation, not interpreted. A part placed twice is one part, read once.
5. **`pmi::write`** takes all parts at once: `write(doc, &[(PartId, PartPmi)], mode) -> Edit`
   with `mode` add, replace or remove (and a presentation policy for replace and remove, item 6).
   Datums resolve per part (two parts may both have A); anchors are checked to be faces or edges
   of the named part. A **typed emission layer** in `write.rs` is the only way the writer makes
   records: measures only through `length_measure(&Length, UnitRef)` and
   `angle_measure(&Angle, UnitRef)`, which emit the typed form
   (`LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(v),#u)`) with the value's own digits; complex
   instances only through `p21::Complex`; `tolerance_value` only from `Deviations { upper, lower }`;
   `limits_and_fits` only from `Iso286Class`. Each concept's form comes from one mapping table
   (below). Before anything is returned, every added or replaced instance passes
   `express::validate` and the edited document passes `express_rules` (the backstop).
6. **Editing and removal.** `add` writes new items beside the part's existing ones; a datum
   label the part already has resolves to its existing `datum` (with different faces: refused).
   `replace(part, new)` means: afterwards `read(part) == new`. It removes the instances the
   reader consumed for the part (its `provenance`), plus their forward dependencies that nothing
   outside the removed set references (measures, representations, items; never units, contexts
   or geometry, which are shared). Every kept instance that still references a removed one is
   then classified by family:
   - *presentation and validation properties* (`draughting_model_item_association`,
     `draughting_callout` and its annotation occurrences and their exclusively owned geometry
     and styles, CAx-IF PMI validation properties): with policy `Refuse` (the default) the edit
     is refused naming them by id and type; with policy `RemovePresentation` they are removed
     too, a `draughting_model` or view whose item list names them is rewritten without them, and
     the report lists every removed and rewritten instance by id;
   - *anything else* (an unconsumed PMI instance found by the reader, an application's own
     data): refused, naming it.
   The removal plan is a document-level operation (`removal.rs`: seeds, policy → removed and
   rewritten instances, or a refusal naming the blockers), independent of the PMI model.
   UDAs and material on the part are in scope of replace exactly when the reader consumed them
   (UDAs and material: yes).
   Unconsumed PMI of the part is kept byte for byte and reported again. `remove(part)` is
   `replace(part, empty)`. Verification is `pmi::read` of the output compared semantically
   with what was written, plus `express` over every instance the edit touched.

7. **Process boundary.** specify-core and draftwright (Python) use the `quiddity` CLI:
   `quiddity parts`, `quiddity pmi read`, `quiddity pmi write --mode add|replace|remove
   [--presentation refuse|remove]`. The versioned JSON form of the model lives in the `quiddity`
   crate (`src/pmi_json.rs`), so haecceity keeps no serde dependency. In JSON, anchors are face
   numbers with the file's sha256 and the reader version as binding (a mismatch is refused), and
   every value is its stated decimal text with its unit.

**Mapping** (writer form; the reader also accepts the older forms the practice says must still
be read, e.g. the pre-4.0.6 datum forms of §6.5.2):

| Concept | AP242 form |
|---|---|
| Feature on items | `shape_aspect` + `geometric_item_specific_usage` per item, or `item_identified_representation_usage` with a `set_representation_item` (§5.1, §6.5.1) |
| Group | `composite_group_shape_aspect` named 'multiple elements' / 'pattern of features', `shape_aspect_relationship` per member (§6.4); all around / between: `all_around_shape_aspect` / `between_shape_aspect` (§6.4.2–3) |
| Derived feature | `derived_shape_aspect` (or the Table 3 subtype) with `shape_aspect_deriving_relationship`s (§5.1.4) — written only when the model states a derived feature |
| Datum | one `datum` per label per part, established by `shape_aspect_relationship` from its `datum_feature` (the feature itself) and/or its `datum_target`s (§6.5, §6.6). A hole or boss datum is a datum feature on the cylindrical faces (§6.5.1, Figure 35); the axis is implied, so specify-core's "hole or boss axis" is written in that form, not as a derived axis |
| Datum target | `placed_datum_target_feature` (point, line, rectangle, circle, circular curve with `shape_representation_with_parameters`) or `datum_target` on an area feature; `feature_for_datum_target_relationship`; 'movable direction' (§6.6) — read now, written later |
| Datum system | one `datum_system` per distinct system, `datum_reference_compartment` per position, never shared (§6.9.7); common datums as `datum_reference_element`s (§6.9.8) |
| Size / location | `dimensional_size` / `dimensional_location` (+ angular, with path, directed) + `dimensional_characteristic_representation` + `shape_dimension_representation` with 'nominal value' (§5.1–§5.2.1); a datum feature with a size is the complex `dimensional_size_with_datum_feature` (§6.5.3) |
| Decimal places | `value_format_type_qualifier` on the value (§5.4); the part default by the §4.1 construct |
| Deviations | `plus_minus_tolerance` → `tolerance_value(lower, upper)` with typed `length_measure_with_unit`s (§5.2.3) |
| Limits | 'upper limit' and 'lower limit' items beside 'nominal value' (§5.2.4) |
| Fit | `plus_minus_tolerance` → `limits_and_fits(form_variance='H', zone_variance='hole'/'shaft', grade='7', source='')` (§5.2.5); stated limits as 'upper limit'/'lower limit' items beside it |
| Basic, reference | `descriptive_representation_item('dimensional note','theoretical')` / `'auxiliary'` (§5.3, Table 7); other modifiers under 'modifiers' (Table 8) |
| Geometric tolerance | the leaf type (`flatness_tolerance`, …); a complex instance only when the table of datum references, modifiers, unit basis, maximum or unequal disposition requires it (§6.9) |
| Tolerance target | the `datum_feature` / `shape_aspect`; the `dimensional_size` or `dimensional_location` for a feature of size; the `product_definition_shape` for the whole part (§6.1–§6.3) |
| Tolerance relation | `geometric_tolerance_relationship` 'composite tolerance' / 'precedence' / 'simultaneity' — read now, written later |
| Zone | `tolerance_zone` + `tolerance_zone_form` (§6.9.2); `runout_zone_definition` (three attributes) only for a stated runout orientation |
| General tolerance | `property_definition('default tolerances')` → `representation('default tolerances')` with `descriptive_representation_item('tolerance class', <text>)` (decision 3); `default_tolerance_table` read only |
| Thread / knurl | `thread` / `turned_knurl` with one `shape_representation_with_parameters` whose items are exactly the model's fields, named as the WHERE rules name them; 'applied shape', 'partial area occurrence', 'thread runout' relationships |
| Material | 'material name' representation as specify-core's files carry it (decision 2); density read only |
| Attributes | UDA practice §5–7: `general_property` 'user defined attribute' |
| Standard | `applied_document_reference` to the dimensioning standard (§4) |

## Anti-requirements

Each OpenCascade defect specify-core works around today (`writer.py`'s docstring, `merge.py`,
`existing.py`), and how this design rules it out. *Types* means the model or the emission layer
has no way to express the defect; *validator* means `express`/`express_rules` would catch it
before anything is written (the backstop for everything).

| OpenCascade defect | Ruled out by | How | Checked by |
|---|---|---|---|
| Datum precedence on the datum (one label per letter per tolerance) | types | `Datum` has no position; precedence is the order of `DatumSystem`'s compartments; one `datum` per label per part | A\|B, B\|C\|A, D\|B\|C on one part read back exactly; one `DATUM` per letter in the text |
| Lower deviation negated | types | `Deviations` holds signed offsets; `tolerance_value` is built only from it; no API takes a magnitude | +0.1/−0.05 and g6 (−0.009/−0.025) round trip |
| Both deviations below nominal unreadable | types | the only invariant is `upper > lower` | g6 and f7 read; NIST STC-10 |
| ISO 286 grade written wrongly (fits as bare limits) | types | `Iso286Class` is two enums; `limits_and_fits` is built only from it | Ø20 H7, Ø70 g6, and Ø20 H7 (20.000/20.021) read back as fits |
| Tolerance unit taken from the last dimension written (1000×) | types | every value carries its own unit; the emission layer takes the unit per measure; the writer holds no unit state | tolerances on a part with no dimension; mm values into an inch-context part |
| Values changed by unit conversion | types | values are kept as stated decimals in their own unit; nothing is converted on write | every value's text kept by read → replace on every inch NIST file |
| PMI that has been read cannot be written back | design + test | read and write share one model; `replace(part, read(part))` is an operation like any other | read → replace → read over every NIST file and specify input, semantically equal; what the writer cannot write yet (datum targets, tolerance relations) refused by name and pinned |
| Malformed `RUNOUT_ZONE_DEFINITION` | validator | written only when the model states an orientation; attribute count checked | validator test with a two-attribute zone |
| Untyped `MEASURE_WITH_UNIT` | types + validator | the emission layer has only typed measure functions; `express` rejects untyped SELECT values | validator test; scan of every written instance |
| Complex entities where the simple form is expected | types + validator | `Complex` holds sorted leaves; the mapping table chooses complex only when required; `express` requires a minimal set | validator test with a complex perpendicularity |
| Datums imported only when referenced | reader | every `datum`, `datum_feature` and `datum_target` of the part is enumerated | an unreferenced datum read back; NIST files |
| One datum per name per document (assemblies) | types | datums belong to a `PartPmi`; labels resolve per part | two-part assembly with datum A on each part |
| Threads, knurls and general tolerance appended as raw text | types + validator | model types written as `thread`, `turned_knurl`, `default_tolerance_table` through the emission layer; `express_rules` checks their WHERE rules | round trip; rule tests |

Also gone: the text transplant (editing is an `Edit` on the document), the presentation
workarounds, the schema-setting workaround, and UTF-8 written where escapes are required.

## Oracles and tests

In order of authority:

1. **The AP242 schema and the CAx-IF practices.** Every instance the writer emits is validated;
   the validator is tested on hand-written valid and invalid instances and run over every NIST
   AP242 file, its findings pinned with reasons (NIST says the files have errors).
2. **NIST's MBE PMI test models** (CTC 01–05, FTC 06–11, STC 06–10, AP242 editions 1–4; ten of
   them use inch units). The STEP File Analyzer's expected-PMI data is used if it can be
   extracted from its release. Otherwise selected models' PMI is transcribed from the drawings
   (`PDF/`), **checked by a second, independent pass against the drawing**, and every reader
   difference is resolved against both the drawing and the file text before it is pinned;
   transcription errors found that way are recorded in `tests/fixtures/ap242/README.md`.
3. **The STEP File Analyzer and Viewer** needs Windows (IFCsvr, tcom, twapi) and cannot run here.
4. **OpenCascade XCAF** through specify-core's venv (`tools/capture_pmi_occt.py`): a
   cross-check with a verdict per difference (`rust-correct`, `rust-wrong`, `equivalent`,
   `undetermined`, `not-applicable`).
5. **draftwright's extraction** (`tools/capture_pmi_draftwright.py`): everything draftwright
   reads must be in the model; parity is a reader acceptance item, so gaps close in the model
   before the writer exists.

Tests: `p21` round trip byte for byte; edits confined to their spans; face provenance against
OpenCascade's; the reader on NIST and on specify-core-written files (inputs only, not a
reference); the removal plan with both presentation policies on NIST files; the writer by
validation, by read-back, by `read → replace → read` over every NIST and specify file, by
value text preservation on inch files, by the anti-requirement cases, and by OpenCascade's
reading of the result with verdicts; determinism (same input, same bytes, fresh hash seeds).

## Out of scope for now

- **Graphic presentation** (callouts, datum feature symbols, polylines, tessellated
  presentation, saved views) derived from the semantics. Existing presentation is kept byte for
  byte, or removed and reported under the `RemovePresentation` policy; it is never updated.
- **Writing datum targets and tolerance relations** (composite frames). Both are read and in the
  model; the writer refuses them by name.
- **PMI on assembly occurrences** (`assembly_component_usage` paths); reported as findings.
- **Writing material density** (read only).
- **A general EXPRESS rule engine**; named checks cover what the writer emits.
- **ISO 286 limit computation** from a class.
- **AP242 XML, external references, validation properties written.**
- **specify-core's `mesh` command** (face-indexed tessellation): an OpenCascade replacement item
  for specify-core, but not PMI; planned separately on top of the face provenance.
