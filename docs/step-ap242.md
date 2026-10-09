# AP242 and semantic PMI in haecceity

**Status:** implemented (2026-10-08): stages 1–4 (foundations; model, reader, removal plan,
rule checks; writer; the `quiddity` command line `parts`, `pmi read`, `pmi check` and, stage 4,
`pmi write` verified by reading back).
Still refused by the writer, by name: datum targets, tolerance relations, notes, general tolerance
tables and decimal places, and material density (the material name is written). See
[Status](#status). Since 2026-10-09 ([decisions](#maintainer-decisions-2026-10-09)) part notes
in words are written as 'semantic text' (§7.4) and surface texture in AP242's surface conditions
form; the refused notes are the non-standard 'manufacturing requirement' route.

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

4. **Interface: the JSON CLI.** specify-core calls haecceity through the `quiddity` CLI with
   versioned JSON (`quiddity parts`, `pmi read`, `pmi write`), as Architecture item 7 says; no
   Python extension module.
5. **Standard forms only.** Threads, knurls, finish and notes are written only in their standard
   AP242 / CAx-IF forms. Python draftwright's non-standard 'manufacturing requirement' text route
   is not written; draftwright (Python and Rust) will read the standard forms instead.
6. **Datum feature symbols now.** The writer writes, for each datum it adds, a minimal datum
   feature symbol (presentation) linked to the datum feature through
   `draughting_model_item_association`, following the PMI practice's presentation sections, so a
   presentation-only viewer shows the datum. This is the one piece of presentation written in
   this run; it is derived from the semantic model (never a separate source of truth) and is
   removed with its datum on replace. Other presentation stays out of scope.
7. **Replace removes the replaced PMI's presentation by default.** The default presentation
   policy for replace and remove is `RemovePresentation`: callouts, associations and their
   exclusively owned geometry and styles that reference removed PMI are removed and every one is
   listed in the report. `Refuse` remains available as an option.
8. **Main-session decisions:** only the generated schema table is committed, with the long
   form's source URL and sha256 (not the EXPRESS file itself); step-io's author is not contacted
   (haecceity rebuilds and checks the id map itself).

## Maintainer decisions (2026-10-09)

From specify-core-rust's questions Q16, Q20 (2) and Q3 (its upstream needs U15 and U3). As
implemented, with the evidence found doing it; the open points are under
[Questions](#questions-2026-10-09).

1. **No `mechanical_design_and_draughting_relationship` for the datum feature symbols**
   (Q16). The symbols' `draughting_model` ('datum feature symbols') is no longer related to the
   part's shape representation; each symbol stays linked to its datum feature by its
   `draughting_model_item_association` (§7.3), and OpenCascade still gives every datum its symbol
   as presentation, with its annotation plane (`check_pmi_occt.py`, required by
   `opencascade_reads_the_written_files`). A **deliberate departure** from the form the writer
   wrote (decision 6's model related to the shape it annotates), to revisit; the practice itself
   does not ask for that relationship (§9.2's global draughting model references the geometry it
   shows; §9.4.4 relates draughting models to each other by
   `mechanical_design_and_draughting_relationship`, as NIST's files do, never to a shape
   representation). The cause is not what U15 says: OpenCascade 7.9's reader reads the files
   with the relationship. The
   SIGSEGV is in specify-core's `load.py`, which looks a label's name up with
   `label.FindAttribute(TDataStd_Name.GetID_s(), TDataStd_Name())`, an in/out handle the OCP
   binding mishandles: that call alone, made on every shape label, crashes on NIST CTC-01 (no
   haecceity instance in it); reading the same labels' names through `TDF_AttributeIterator`
   reads every file, with or without the relationship (the stage 3 `every_kind` file and
   specify-core-rust's crashing `plate`); `load.load_all` still crashes on the file with the
   relationship and opens the file without it. So the relationship only moves the heap so that
   the bad call crashes. `check_pmi_occt.py` records both: `loads` (specify-core's
   `load.load_all` in a child process) and `reads` (the safe walk).
2. **One `geometric_item_specific_usage` per face** (Q20 (2)), never an
   `item_identified_representation_usage` of a `set_representation_item` (the §5.1 Figure 5
   form, which OpenCascade drops, and the datums only such an item cites with it). A shape
   aspect has one usage per representation (`item_identified_representation_usage` UR2: the
   schema's `UNIQUE UR2: used_representation, definition`), and OpenCascade, given a feature
   with two such usages, reads only the first face (tried on the `every_kind` profile tolerance:
   faces 9 and 10 read as face 9). So each face of a feature of several faces is a member
   `shape_aspect` with its one usage, shared by every feature on that face (§5.1: one shape
   aspect and usage per face), and the feature is composed of its members by
   `shape_aspect_relationship` (§5.1 Figure 5's composition), which the reader already read as
   the feature's faces. OpenCascade reads all of them: the five sizes and tolerances it dropped
   in the written files (`known_pmi_write.json` "occt") now read on their faces with their datums.
3. **Notes and surface finish in standard forms** (Q3, decision 5).
   - *Part notes in words* (coating, heat treatment, edges) are the practice's editable text,
     §7.4: a 'semantic text' user defined attribute (`AttributeSet { name: "semantic text", items:
     [(<variable name>, Text(<line>))] }`, one per note), which the writer already wrote and the
     reader already read; on the part it is now on the `product_definition_shape` (§7.4.3
     Table 17's 'on part'). The form is NIST's (`GENERAL_PROPERTY('','semantic text',$)`, FTC-07
     16 times). OpenCascade's metadata reader keeps one string per property name on the part's
     label, so of several notes it shows the last (pinned rust-correct). `Note` stays the
     'manufacturing requirement' route, read and refused.
   - *Surface texture* is new in the model (`SurfaceTexture { on: Option<FeatureId>,
     material_removal, parameters: [SurfaceTextureParameter { characteristic, value: Length }] }`)
     and written as ISO 10303-1110 (AP242's surface conditions module) maps `Surface_texture` and
     `Standard_surface_texture_parameter`, the PMI practice having no section on it and no NIST
     file carrying one: `property_definition('surface texture','',<the faces' shape aspect or the
     part's product_definition_shape>)` with `representation('surface texture')` holding
     `descriptive_representation_item('material removal condition', 'any process allowed' |
     'material removal required' | 'no material removal')`; per parameter
     `property_definition('surface_condition','',<same owner>)`, related to the texture by
     `property_definition_relationship('surface texture parameter')`, represented by a
     `surface_texture_representation('surface texture parameter', (descriptive_representation_item(
     'measuring method', 'Ra'), measure_representation_item('characteristic value',
     LENGTH_MEASURE(3.2), µm)))` and associated with `general_property('', 'surface_condition')`.
     The parameter is named 'surface_condition', not the mapping's 'surface texture parameter':
     `surface_texture_representation` WR5 requires the association with 'surface_condition', and
     `general_property_association` WR2 requires the derived definition's name to equal the
     general property's, so no other name is schema-valid (question 3). Checked against the
     schema (the writer's validation), `surface_texture_representation` WR1–WR5,
     `general_property_association` WR1–WR2 and the global rule
     `restrict_representation_for_surface_condition` (`notes_and_surface_textures_are_standard_forms`;
     `express_rules` now evaluates the WHERE rules, with
     `mechanical_design_and_draughting_relationship` WR1–WR3, on every document it checks, but
     not the global rule). The reader reads this form, and the parameter
     named as the mapping names it; a parameter without exactly one association with
     'surface_condition' (WR5) is reported and its texture not read, as is anything else of a surface condition (direction,
     manufacturing method, machining allowance, evaluation length, filters, value ranges) is
     reported and the texture not read. OpenCascade XCAF has no surface texture
     (not-applicable). The CLI's JSON carries it (`surface_textures`, terms `any_process_allowed`,
     `material_removal_required`, `no_material_removal`).

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

**Material** (`Material { id, name, density: Option<Density> }`): decision 2, on the CAx-IF
*Recommended Practices for Material Identification and Density* Release 2.1 (obtained; see
`tests/fixtures/ap242/README.md`). §4.1: `property_definition('material property', 'material
name', <product_definition>)` → `representation('material name', …)` holding
`descriptive_representation_item(<material id>, <material name>)`; the reader keeps both strings
as stated. §4.2: `property_definition('material property', 'density', …)` →
`representation('density', …)` holding `measure_representation_item('density measure',
POSITIVE_RATIO_MEASURE(v), <derived unit>)`; the reader keeps the value as stated with its
derived unit. The practice predates AP242 (it names AP214 and AP203). Material on a sub-shape
(§4.1's optional form) is reported, not read; §5's material-as-product is not read. The writer writes the
name as specify-core's files carry it; density is read only.

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
   distinct part, `read_part_definitions` gives its `product_definition`, display name, placements and
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
     and styles, CAx-IF PMI validation properties): with policy `Refuse` the edit
     is refused naming them by id and type; with policy `RemovePresentation` (the default, decision 7) they are removed
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
| Feature on items | `shape_aspect` + `geometric_item_specific_usage` per item, or `item_identified_representation_usage` with a `set_representation_item` (§5.1, §6.5.1); the writer writes one `geometric_item_specific_usage` per face, a feature of several faces composed of one member shape aspect per face (decisions of 2026-10-09, 2) |
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
| Material | Material practice R2.1 §4.1: `property_definition('material property','material name')` → `representation('material name')` → `descriptive_representation_item(id, name)`, as specify-core's files carry it (decision 2); §4.2 'density' → `measure_representation_item('density measure', POSITIVE_RATIO_MEASURE, derived unit)`, read only |
| Attributes | UDA practice §5–7: `general_property` 'user defined attribute' |
| Note in words | §7.4 'semantic text': an attribute set of that name, one per note, on the part's `product_definition_shape` or a feature (decisions of 2026-10-09, 3) |
| Surface texture | ISO 10303-1110 `Surface_texture` / `Standard_surface_texture_parameter`: `property_definition('surface texture')`, related (by 'surface texture parameter') 'surface_condition' property definitions with `surface_texture_representation`s and `general_property` 'surface_condition' (decisions of 2026-10-09, 3) |
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

## Status

**Stage 1 (2026-10-08): foundations.** Delivered and tested.

- **`p21`** (`crates/haecceity/src/p21.rs`, `tests/p21.rs`): the lossless Part 21 document of
  [Architecture](#architecture) step 1. Every instance's id, record and byte range, checked
  against step-io; edits (add with provisional ids, replace, remove) with removal and dangling
  references refused, literal ids of additions refused, anchored (edition 3 ANCHOR) instances
  protected; byte-exact write of untouched text, including CRLF and Latin-1 files and ANCHOR and
  REFERENCE sections; complex records sorted, unsorted ones refused; `file_schema()` refuses a
  malformed FILE_SCHEMA. Checked over the fixtures, the corpus and the NIST files. Still refused:
  several DATA sections (step-io reads one), SIGNATURE sections, and the `\PB\`..`\PI\` code page
  switches in strings.
- **`express`** (`crates/haecceity/src/express.rs`, generated `express_table.rs` from
  `tools/express_table.py`, `tests/express.rs`): the AP242 EXPRESS schema as data for editions 1
  and 4, and an instance and document validator (attribute count, types, typed SELECT values
  including narrowing redeclarations, entity references, aggregates and their bounds, complex
  instance combinations against SUPERTYPE expressions), plus entity families. Each NIST file's
  violations are pinned (`tests/fixtures/ap242/express/`). Not yet: WHERE and UNIQUE rules, string
  widths, bounds written as expressions, and tables for editions 2 and 3 (no long form found;
  their files are checked as an upgrade to edition 4).
- **Face provenance** (`step.rs`, `brep.rs`, `tests/face_sources.rs`): each part's faces and
  edges carry their `ADVANCED_FACE`/`EDGE_CURVE` `#N`, with per-part numbering, so PMI binds to
  faces by `#N`. Compared with OpenCascade by instance, with one verdict per cause in
  `tests/fixtures/known_face_sources.json` (93 rust-correct, 43 undetermined, 1 not-applicable);
  the undetermined ones are OpenCascade's edge order within loops, which follows its ShapeFix
  healing. Edge indices are haecceity's own: anchor edges by `#N`. Includes a two-part assembly
  fixture (`tests/fixtures/ap242/assembly/`). Reading now parses each file twice (+28 to 37%,
  measured); wireframe `TRIMMED_CURVE`s that PMI can reference are not part edges yet.
- **Oracles** (`tests/fixtures/ap242/`, README there): NIST's expected PMI extracted from the
  STEP File Analyzer's own spreadsheets (`tools/nist_expected.py`), 7 gzipped NIST models,
  OpenCascade XCAF captures of all 17 NIST AP242 files and the specify-core inputs
  (`tools/capture_pmi_occt.py`), draftwright captures (`tools/capture_pmi_draftwright.py`),
  seven specify-core-written reader inputs (`tools/make_specify_inputs.py`), and the CAx-IF
  material practice (Release 2.1). The expected PMI is SFA's display notation, which the reader
  stage must parse to compare. The OpenCascade captures do not yet record angular units, which
  part a material is set on, or OpenCascade's own reader errors.

**Stage 2 (2026-10-08): semantic model, reader, removal plan, rule checks.** Delivered and
tested; `pmi::write` (and replace built from the removal plan) is the next stage.

- **Semantic model and reader** (`crates/haecceity/src/pmi/{mod,model,read,standards}.rs`,
  `tests/pmi_read.rs`, `tests/pmi_draftwright.rs`): `pmi::read(doc, &parts)` returns
  `PmiRead { parts, findings, provenance, accounting }`. Values are kept as stated with their own
  units; features anchor to faces and edges by `#N` or hold supplemental geometry by value;
  invariants are enforced by constructors and `PartPmi::validate`. Every semantic-PMI instance
  is consumed by a part or named by a finding (kinds include not-modelled, nonconformance,
  unitless-measure, conflict, assembly-pmi, occurrence-pmi); measure and unit errors are typed.
  All 17 NIST AP242 files are read and fully accounted for, with findings pinned per file in
  `tests/fixtures/ap242/read/findings.json`. Verdicts (`tests/fixtures/known_pmi.json`,
  `known_pmi_draftwright.json`): NIST expected PMI 74 (65 rust-correct, 9 equivalent);
  OpenCascade 403 (402 rust-correct, 1 rust-wrong: ftc_08's tessellated-only part has no B-rep
  anchor); specify-core intent 38 (20 rust-correct, 18 not-applicable); draftwright 117 (111
  rust-correct, 6 not-applicable). PMI on an assembly's own shape or on an occurrence is a
  finding, never part PMI (tested).
  The model departs from [Semantic model](#semantic-model) above, on NIST and draftwright
  evidence, and those sections are not yet rewritten: `standards` is a `Vec`;
  `Dimension.nominal` is optional and there is `Dimension.principle`; `Anchor::Geometry` and
  `PartPmi.geometry`; `Note.kind` and `NoteOwner`; `Material` has `id` and `name`;
  `ProjectedZone.end` is optional; `TargetShape::Curve`; `GroupKind::Unstated`;
  `GeometricTolerance.auxiliary` and `.description`. Reported, not held: the affected-plane
  zone, a derived feature's explicit geometry, hole definition parameters, datum-system axis
  placement, material on a sub-shape, material as a product. Open: what a thread's 'number of
  threads' counts (kept as stated; no pitch derived).
- **Removal plan** (`crates/haecceity/src/removal.rs`, `tests/removal.rs`): `plan(doc, seeds,
  PresentationPolicy::{Refuse, RemovePresentation})` and `RemovalPlan::into_edit`. Removes the
  least fixpoint of the seeds plus forward dependencies no kept instance holds; never removes
  shared infrastructure (units, contexts, product structure, topology, part shape
  representations; since 2026-10-09 not a feature definition's own `product_definition_shape`:
  "For specify-core-rust (2026-10-09): U14 and U10" below) or presentation representations;
  `Refuse` names every blocker, `RemovePresentation` removes presentation and
  validation-property blockers and rewrites draughting models, views and groups, refusing an
  emptied `items` set. Checked on all 17 NIST files (the feature-definition-shape rule of
  2026-10-09 re-checked only on the 14 present locally; ctc_04 e2, ftc_08 e4-tg and ftc_11 e3
  still to re-run) with both policies (counts pinned; kept instances byte-identical; parts and faces
  unchanged) and a hand-made fixture. nist_ctc_01 under `RemovePresentation` is a pinned refusal
  (its PMI draughting model would be emptied); removing emptied models instead is undecided. The
  tests' seed stand-in includes id/uuid attributes, property definitions on PMI items and UDA
  associations; the writer's provenance must include these or replace will be refused.
- **Named EXPRESS rule checks** (`crates/haecceity/src/express_rules.rs`,
  `tests/express_rules.rs`): `check_all(doc)` over a public `RULES` table (thread, knurl,
  tolerance table, datum, datum target, datum system, tolerance, compartment single owner,
  plus_minus_tolerance UR1, IIRU UR1/UR2/WR1), three-valued (UNKNOWN is not a violation), each
  rule citing its schema label. `valid.stp` passes all; `invalid.stp` gives exactly the pinned
  violations; NIST violations are pinned in `known_nist_rule_violations.json`. Two are
  schema/practice conflicts rather than faulty files: datum_target WR5 against
  placed_datum_target_feature WR1, and IIRU UR1/UR2 against the n:m draughting associations of
  PMI practice §7.3 (the pins still say file-wrong). Because existing NIST files already violate
  rules, the writer needs a "no new violations among touched instances" form of the check. Not
  checked: general_datum_reference WR1–WR6, geometric_tolerance WR2/WR4,
  geometric_tolerance_relationship WR3.

**Stage 3 (2026-10-08): the writer and the command line's read side.** Delivered and tested;
integrated on `ap242` from streams `ap242-writer` (datum feature symbols, decision 6, included)
and `ap242-cli-read`.

- **`pmi::write`** (`crates/haecceity/src/pmi/write.rs`, `tests/pmi_write.rs`,
  `tests/pmi_roundtrip.rs`, `tests/fixtures/known_pmi_write.json`, `tests/fixtures/ap242/write/`):
  `write(doc, parts, &[(PartId, PartPmi)], Mode::{Add, Replace, Remove}, PresentationPolicy)
  -> (Edit, WriteReport)`, every part in one edit, deterministic. Every item the writer will not
  write is refused at once, by part and item (`Refusal`, `ItemRef`), before anything is made.
  Before returning, the edit is applied and every added or replaced instance validated against
  the file's edition table, and `express_rules::check_all` of the result compared with the
  original's by (instance, rule): a violation the edit introduces refuses it; the original's are
  reported. `pmi::write::differences` compares two `PartPmi` by meaning (indices resolved,
  collections as sets, values as quantities to 1e-12), `differences_as_stated` by value text and
  unit too.
- **Round trips** (read → replace every part with what was read → read, values as stated),
  pinned per file in `known_pmi_write.json` "roundtrip" with the file's sha256 (a pin is never
  matched against another file of the same name): equal for 18 of 21 files, the 7 specify-core
  inputs and the assembly plus 10 of NIST's (nist_ctc_01, ctc_02, ctc_03, ftc_07, ftc_08-e2,
  ftc_10, stc_06, stc_07, stc_09-e4, stc_10), with the refused items left out and pinned with a
  reason, and the number of items left out per kind (refused items and everything depending on
  them) pinned too; the policy `Refuse` result pinned for each. No instance the reader consumed
  survives a replace (supplemental geometry excepted, below); no finding names a written
  instance; every other finding is the original's. Refused by the removal plan, pinned:
  nist_ftc_06, nist_ftc_09, nist_stc_08 (PMI of the part the reader did not read references what
  the replace removes). Without `HAECCEITY_NIST_PMI` the 7 committed NIST models run; with it,
  every NIST pin must be of a file in that directory (stale pins fail) and every file must be
  pinned.
- **NIST's set.** The pins are of the set the reader's fixtures record (`NIST-PMI-STEP-Files.zip`
  sha256 `1fb91bb8…`, February 2026; per-file sha256 in `ap242/occt/*.json.gz`). nist.gov
  serves an older zip from this machine (sha256 `8fa78429…`, files of 2022–2024), of which 11
  files are byte-identical to that set; with the 7 committed models, 13 of its 17 files are
  pinned. Not obtainable here, so not pinned: nist_ctc_04_asme1_ap242-e2,
  nist_ctc_05_asme1_ap242-e1 (the served file of that name differs), nist_ftc_08_asme1_ap242-e4-tg
  and nist_ftc_11_asme1_ap242-e3. A run with `HAECCEITY_NIST_PMI` naming the full set reports
  these four as not pinned until they are run and pinned.
- **Datum feature symbols** (decision 6): for each datum feature of a datum the writer adds, a
  minimal symbol (the label boxed in Hershey single-stroke lettering, a stem to a triangle whose
  apex is on the feature), derived from the model at write time: one tessellated callout
  (`tessellated_curve_set` in a `tessellated_geometric_set` named 'datum', §8.2) in its own
  annotation plane (§9.1), the planes in one `draughting_model` of the part related to its shape
  representation by `mechanical_design_and_draughting_relationship`, each callout linked to its
  datum feature by `draughting_model_item_association('PMI representation to presentation
  link', …)` (§7.3). Laid out above the part in the plane of its two longest extents, each
  symbol in the plane through its feature's point, sized to the part (1/30 of its diagonal).
  Replace and remove take it with its datum (the removal plan's presentation removal under the
  default `RemovePresentation`; under `Refuse` it is presentation like any other, and a replace
  that would orphan it is refused, as the caller asked). A datum resolved to the file's own
  (add) gets none. OpenCascade links each symbol to its datum as the datum's presentation, with
  its annotation plane (`check_pmi_occt.py` records it; the test requires it for every datum).
- **Anti-requirements**: each row has its test in `pmi_write.rs`, named after it (precedence on
  the system; signed deviations; g6/f7; fits as `LIMITS_AND_FITS`; each value's own unit,
  including millimetres into NIST STC-06's inch part; stated text; read PMI written back;
  three-attribute runout zones; no untyped measure; simple form where it suffices; unreferenced
  datums; datum A on both parts of the assembly, the other part's bytes untouched; THREAD,
  TURNED_KNURL and the 'default tolerances' class read back, tables refused).
- **Add** onto every NIST file and specify-core input keeps every original instance byte for
  byte and `FILE_SCHEMA`, and the original PMI reads back unchanged beside the new.
  **specify-core's intents** written onto the original corpus files (AP214 → AP242, each file
  upgradable per `corpus_upgrade.json`) read back as written; against what specify-core wrote,
  every difference matches a pinned pattern with a verdict (fits as deviations, text notes,
  values in metres, threads as attribute sets: rust-correct; knurls: undetermined).
  **OpenCascade** reads the written files (`tools/check_pmi_occt.py`): 6 differences, all
  rust-correct (it does not read a feature identified by an `item_identified_representation_usage`
  of a `set_representation_item`, the practice's §6.5.1 form; it gives a fit with stated limits
  the limits' middle as its value).

- **Command line, read side** ([Architecture](#architecture) item 7; `src/pmi_json.rs`,
  `src/bin/quiddity.rs`, `tests/pmi_cli.rs`): `quiddity parts` (distinct parts and the binding),
  `quiddity pmi read [--part N]` (the model and findings as JSON) and `quiddity pmi check`
  (a JSON document decoded through the model's constructors against the file). The JSON form is
  versioned (`quiddity-pmi` `VERSION` 1) and bound to the STEP text's sha256 and the reader
  (`READER`), so anchors are never trusted against another file or reader. Values are stated
  decimal text with their own unit, never JSON numbers; fits are deviation plus grade; datum
  systems live on the tolerance. Decoding refuses, naming the JSON path: unknown fields,
  unknown or misspelt terms (a name outside the practice's table is written `{"other": …}`, and
  an `{"other": …}` naming a standard term is refused), duplicate keys, and violated model
  invariants. `READER` is pinned to the sha256 of `pmi read`'s output over the committed
  fixtures, so a reader change fails until the version is bumped.

Settled by the implementation (where the design was open or silent):

- *Editions.* An AP242 file keeps its `FILE_SCHEMA` whatever its edition (decision 1: an AP242
  file stays AP242); the instances the writer makes are validated against that edition's table
  where there is one (editions 1 and 4), else against the target's (editions 2 and 3), and the
  report names the table (`validated_against`). For an edition 2 or 3 file, whether the written
  instances are valid instances of the file's own edition is therefore undetermined: the report
  says so (`edition_undetermined`) and the round-trip pins record it per file (8 NIST files).
  An AP214/AP203 file that gains PMI becomes `TARGET_SCHEMA` only when every instance validates (refused otherwise, naming them). A replace or remove that adds
  nothing keeps the schema.
- *Supplemental geometry* is the part's geometry, not PMI: a replace keeps it (it is not a
  seed of the removal plan) and what is written uses the file's item of equal value, so the
  geometry is not duplicated; new geometry goes into one `constructive_geometry_representation`
  ('supplemental geometry') of the part, related to its shape representation.
- *Removal* (the caller's decisions `removal.rs` leaves open): under `RemovePresentation` a
  presentation model the removal would empty is removed with what it held, and the
  `id_attribute`s and edition 4 `uuid_attribute`s of removed items and their geometric validation
  properties (by their description, which some files give free names) go with them; anything
  else blocks the replace.
- *Usages.* A feature of one item is a `geometric_item_specific_usage`; of several items in one
  representation, an `item_identified_representation_usage` of a `set_representation_item`
  (§6.5.1 example; the GISU redeclares its item as one `geometric_model_item`, and UR2 allows one
  usage per aspect and representation). *Superseded 2026-10-09 (decision 2 of that day): one
  `geometric_item_specific_usage` per item, each item of a feature of several in one
  representation through a member shape aspect of its own.* Features of equal items are one instance. Items several
  features share (UR1: one usage per item and representation) are owned by one referenced
  feature and the others are composed of it by `shape_aspect_relationship` (§6.5.2, read as
  the same items); in add, items a kept plain shape aspect already identifies are composed of it
  (per face, or all of a feature's faces where one usage of the aspect identifies exactly
  them, as a `set_representation_item` does).
- *Datum features*: a datum feature on a pattern applying to each member is the group and the
  datum feature in one complex instance (§6.5.2); a datum feature with exactly one size
  dimension, not otherwise a tolerance's or attribute's feature, is the
  `dimensional_size_with_datum_feature` (§6.5.3). A one-datum compartment carries its reference's
  modifiers (the two are one thing; `common_datum_list` needs two elements). Datum systems are
  named by their labels ('A|B|C'), made unique per part (`datum_system` UR1).
- *Threads and knurls*: thread WR12 requires the 'partial area occurrence', so a thread without
  one is refused; WR16's 'thread runout' aspect is always written. Their area (and a runout) is
  an `applied_area` (`thread_runout`) instance; where those faces are a feature in their own
  right it is a second instance composed of it. A knurl's area is its own faces (the model holds
  no other). Ratios take a context-dependent unit 'ratio' with the ratio unit (the reader resolves
  a named unit by its leaves), counts one named 'count'.
- *Refused, by name* (beyond the design's datum targets and tolerance relations): notes
  (decision 5), tables and the part's decimal places (decision 3), material density
  (decision 2), groups of unstated kind, dimensions without a nominal value, tolerances on a
  `shape_aspect_relationship` (the reader reads one from a feature as its composition), projected
  zones without their end, the affected plane, thread/knurl parameter counts outside WR1/WR2,
  booleans in attribute sets, attribute units the file does not name uniquely, and values that
  are not Part 21 REALs (written verbatim, never rewritten).

Reader gaps the writer found (`pmi/read.rs`, not changed here): a `boolean_representation_item`'s
value is looked up as its own attribute (it is `boolean_literal.the_value`), so booleans never
read; a bare 'thread runout' aspect (WR16, no runout feature) is reported unconsumed (fixed
2026-10-09: read with its thread, "For specify-core-rust (2026-10-09): U14 and U10" below); a simple
`RATIO_UNIT` is not resolved as a unit; a tolerance on a `shape_aspect_relationship` from a
feature is read as that feature's composition.

Not done in this stage: writing datum targets and tolerance relations (design: Out of scope);
pinning the four NIST files of the reference set not obtainable here (above); knurls from
specify-core intents (the intent lacks parameters `turned_knurl` requires; undetermined). The
stage 1 and 2 suites `p21.rs`, `express_rules.rs` and `removal.rs` still fall back to a
hard-coded scratch directory for the NIST files when `HAECCEITY_NIST_PMI` is unset; the stage 3
suites use the variable only. 
**Stage 4 (2026-10-08): the command line's write side.** Delivered and tested; integrated on
`ap242` from stream `ap242-cli-write`.

- **Command line, write side** (`src/bin/quiddity.rs`, `tests/pmi_cli.rs`): `quiddity pmi write
  file.step pmi.json -o out.step [--mode add|replace|remove] [--presentation refuse|remove]`
  (add by default; presentation removed by default, decision 7) writes every part of the
  document in one `pmi::write`, refusing a document bound to another file or reader. The output
  goes to a temporary file beside the destination, is read back with `pmi::read` and compared by
  meaning, values as stated (add: the part's PMI before plus exactly the items written, a feature
  or datum equal to one the part has being that one, and a standard it already states not
  written again; replace and remove: exactly the items written, and no instance the reader
  consumed for the part survives, supplemental geometry excepted; other parts and every finding
  as before), and renamed only then; a refusal creates nothing. The report is JSON
  (`quiddity-pmi-write`). Through the CLI, read → replace → read
  over the fixtures and NIST's set matches the writer's round-trip pins: the files with refused
  items are refused naming exactly the pinned refusals; where those are leaves (notes, tolerance
  relations, attribute sets) the JSON without them is written (13 files with NIST's set), reads
  back equal, and a second round trip is a byte-identical fixed point (the first merges features
  of equal items); add then remove on a corpus part returns it to its PMI.
  Tested on FTC-07: adding the part's own stated standard writes nothing (0 instances added,
  output byte-identical to the input). An existing destination, including the input itself, is
  replaced only once the check passes; a symbolic link at `-o` stays a link, the file it names
  is replaced in place and keeps its permissions (unix test).

Not done in this stage (writer behaviour the command line's read-back catches, in
`pmi/write.rs`, not changed here): add of a material to a part that has one is written by
`pmi::write`, and the command refuses it only because the output reads back with two material
names (tested); add of a feature equal to one the part has writes a second shape aspect for it,
which reads back as the same feature (leaving it out means renumbering every item that refers to
features by index). The refuse-policy test counts presentation blockers by parsing the writer's
refusal message on stderr, the only place a refused write reports them.

**For specify-core-rust (2026-10-09): parts, names, read-back.** Its upstream needs U1, U7 and
U9, in the library, so its stand-ins can go:

- **U7** `pmi::verify(before: Snapshot, base: &Document, written: &[(PartId, PartPmi)], mode,
  after: Snapshot) -> Verification` (`pmi/verify.rs`) is `quiddity pmi write`'s read-back check,
  moved: `Verification.problems` lists each `Problem` (part count, a part's name or face or edge
  count changed, a part not written that changed, add's lost, unexpected and missing items,
  replace's differences and surviving consumed instances, new findings), whose `Display` is the
  command's message. `base` is the document the edit was applied to (`before.doc` for the
  command; a copy with some PMI already cleared for specify-core's U14 stand-in, whose removed ids
  then do not count as survivors). In add, standards the part already states are left out of
  `written` by the caller, as the command does. `tests/pmi_verify.rs`.
- **U1** `step::read_part(bytes, &PartDefinition) -> Result<Part, StepError>`: the part's shape
  where it is first placed, read with no placement applied (its own coordinates as its shape
  representation states them, so nothing is inverted), faces in `PartDefinition.faces` order,
  face and edge sources (`Source::instance` the whole file's instance number) and unresolved lists
  kept; refused, naming the part, when its definition is not placed in the file or a face of the
  definition is not read exactly once. `Part::with_sources` / `with_unresolved` stay `pub(super)`:
  with `read_part` no caller outside haecceity rebuilds a part. `tests/step_parts.rs` checks, on
  the assembly fixture (the pin placed twice, once turned) and the 7 committed NIST files, that
  each placement in the whole file's read is the part moved: face by face, area to 1e-9 and
  centroid to 1e-6 mm, sources and unresolved faces equal.
- **U9** `PartDefinition.name` is the display name OpenCascade XCAF gives and specify-core
  reports: escapes decoded (`p21::decode`), `PRODUCT.id` when `PRODUCT.name` is empty, and the
  name of an assembly that holds the part as its only component (first in visiting order with a
  name). `StepFile::read(bytes)` reads a file once: `parts` (as `read_part_definitions`, which
  now calls it), `assemblies` (each `Assembly { product_definition, name, components:
  [Component { occurrence, definition }] }`, components in file order, assemblies depth first
  from the roots) and `part(i)`, the `Part` of `read_part` without parsing again. Checked against
  XCAF (specify-core's `load.parts` and the labels' own names through OCP, OpenCascade 7.9) on
  the assembly fixture, `wrapped.step` (made for this, a plate in an assembly named with Part 21
  escapes: both 'Gehäuse Ø'), the corpus's 10 NIST files and NIST's AP242 set: every product
  name agrees with XCAF's label for the product (FTC-08 and CTC-04: the id; NIST CTC-02 AP242
  crashed OpenCascade in that check, so has the capture's result only). The one kind of
  difference left is pinned in `known_face_sources.json`, verdict rust-correct: where XCAF makes
  a NIST product an assembly of its representation items, specify-core names the part after the
  item's label ('SOLID'); haecceity names it by its product. Three earlier pins (an empty name
  reported as written) are gone. Since part names are in `pmi read`'s output and `pmi check`
  holds a document to them, `pmi_json::READER` is now `haecceity-pmi-read/2` (of the committed
  fixtures only NIST STC-06 and STC-09 read differently: their names, the id for an empty name).
  Stage 5 makes it `haecceity-pmi-read/3`: a file's surface textures are now read where they
  were reported (no committed fixture has one, so the pinned output is otherwise unchanged).

**Stage 5 (2026-10-09): the maintainer's writer decisions of 2026-10-09.** Delivered and tested
([decisions](#maintainer-decisions-2026-10-09)): no relationship for the symbols' draughting
model; one usage per face; 'semantic text' notes and `SurfaceTexture` written, read and carried
by the CLI's JSON (`PartProvenance.surface_textures`, `ItemRef::SurfaceTexture`, refused when a
value is not a Part 21 REAL).

- *Read-back*: `a_feature_of_several_faces_has_a_usage_per_face` and
  `notes_and_surface_textures_are_standard_forms` (`pmi_write.rs`) read each write back equal as
  stated and pass `pmi::verify`; no `set_representation_item` or `item_identified_representation_usage`
  is written, each face is identified once, and the added instances violate no rule.
  `add_composes_a_feature_of_an_aspect_of_its_faces`: an add onto a file whose plain shape
  aspect identifies the feature's two faces by one `set_representation_item` usage composes the
  feature of that aspect and adds no usage (per-face members would identify each face twice). A remove
  takes everything written except the micrometre unit the write added (the removal plan never
  removes units). The NIST and specify round trips (`pmi_roundtrip.rs`, 13 NIST files here) and
  the add tests are unchanged: their pins needed no update.
- *OpenCascade* (`check_pmi_occt.py`, specify-core's venv, OCP 7.9.3.1): all five written files
  load in specify-core's `load.load_all` and read in XCAF; the stage 3 `every_kind` (with the
  relationship) still crashes `load.load_all` (SIGSEGV) and reads in the safe walk. Differences,
  `known_pmi_write.json` "occt", 5 (was 6): the H7 fit read as its limits' middle (rust-correct,
  as before, its instance renumbered), two of `every_kind`'s three part notes (XCAF keeps the
  last 'semantic text' only; rust-correct) and its two surface textures (XCAF has none;
  not-applicable). The five "feature identified by a `set_representation_item`" differences are
  gone: OpenCascade reads those sizes and tolerances on all their faces, with their datums.
- *Written files* (`tests/fixtures/ap242/write/`): re-exported (no relationship; per-face
  usages; `every_kind` gains three part notes and two surface textures, one on the two faces of
  its profile tolerance) and re-captured.

**For specify-core-rust (2026-10-09): U14 and U10.** Its upstream needs U14 (a part with a
thread haecceity wrote written again) and U10 (its Python notes on faces), so its stand-ins
(`writer::clear`'s restated shapes, the runout exception in `writer::verify`, the
`GEOMETRIC_ITEM_SPECIFIC_USAGE` scan in `existing.rs`) can go.

- **U14, reader**: thread WR16 requires one 'thread runout' aspect of the thread's shape and
  allows it no 'thread runout usage' (no runout stated); the reader now consumes such a bare
  aspect with its thread (a 'partial area occurrence' without its usage, which WR12 forbids,
  stays unconsumed and reported). `assert_reads_back` (`pmi_write.rs`) no longer excuses it: no
  write leaves a finding on what it wrote.
- **U14, removal plan**: a `product_definition_shape` whose definition is a
  `characterized_object` (a `thread`'s or `turned_knurl`'s own shape, the 'applied shape'
  construct of thread WR13) is no longer shared infrastructure. `product_definition_shape` WR1
  allows only a `characterized_product_definition` (product definition, occurrence or
  relationship: product structure, still infrastructure) or a `characterized_object` as its
  definition, and UR1 gives each definition one shape, so that shape is its object's alone; the
  reader consumes it with the thread, and the plan removes it with it. Anything kept that
  references it (the thread's own aspects, when the shape alone is a seed) is a blocker as
  before, and a part's own shape is still refused as a seed
  (`removal_takes_a_feature_definitions_shape_only_with_its_feature`, both policies). The NIST
  removal pins are unchanged (checked on the 14 of the 17 pinned files available here; none
  has such a shape).
  `a_written_thread_is_replaced_and_removed` writes an internal thread (no runout) and,
  separately, an external thread with a runout and a straight knurl onto the spool; each reads
  back with no unconsumed instance, is then replaced by a flatness and removed; every output
  passes `express::validate_document` and `express_rules::check_all` with no new violation and
  `pmi::verify` (exactly the items written, no consumed instance surviving), and no feature
  definition's shape is left. Before the change the replace failed with U14's message ("seed
  #6514 product_definition_shape is shared infrastructure").
- **U10**: specify-core's Python writer (`requirements.append`) writes a thread, knurl or face
  finish as a part-level 'manufacturing requirement' note of that kind, immediately followed by
  a plain `shape_aspect` named as the kind whose `geometric_item_specific_usage`s (named alike)
  identify the faces, and nothing semantic refers to it (its callout's
  `draughting_model_item_association` does). The reader already read that aspect as a feature in
  its own right; it now puts the note of its kind written last before it on that feature
  (`Note.on`), for the kinds specify-core's `existing.notes` reads ('internal thread', 'external
  thread', 'knurl', 'surface finish', 'surface texture'). A note no aspect follows (a part note,
  as 'surface texture' unless otherwise specified) stays on the part; an aspect of that form that
  no unanchored note of its kind precedes is reported (`unresolved`: which note it is for is not
  stated) and its faces stay a feature. The pairing rests on the writer's order, the only link
  the file states. Python's `existing.notes` gives one entry per kind with the union of every
  such usage's faces and no text; haecceity gives each note its own faces (equal to Python's
  wherever a part has one note of a kind, as in every specify-core file here).
  `specify_core_notes_on_faces_are_anchored` (`pmi_read.rs`) checks the committed specify-core
  files against `existing.notes` run on them with specify-core's venv (the assembly's two parts,
  the bolt, string post and thumbwheel: 7 notes, faces equal), and two finishes and a stray
  aspect added to the bolt; oracle 5 (`specify_core_outputs`) now also requires each thread and
  knurl of the intent to have its note on its faces (no new pin). A live write of
  specify-core-rust's plate with a face finish (its `tests/existing.rs` script) reads the
  'surface finish' note on face 2 and the 'internal thread' note on face 12, as `existing.notes`
  does. The notes' owners are in `pmi read`'s output, so `pmi_json::READER` is now
  `haecceity-pmi-read/4` (the committed fixtures' output changed only in those owners).

**specify-core needs** (specify-core-rust `docs/upstream-needs.md`, checked at this head):

- **U3** notes and surface finish in `pmi::write`: *met* in the form decided 2026-10-09
  (decision 3): part notes in words as 'semantic text' attribute sets, surface texture on the
  part or on faces as `SurfaceTexture`; `PartPmi.notes` (the 'manufacturing requirement' route)
  stays refused by decision 5. `notes_and_surface_textures_are_standard_forms` (`pmi_write.rs`).
- **U7** read-back verification as a library function: *met*, `pmi::verify`
  (`pmi/verify.rs`); `tests/pmi_verify.rs` (`add_reads_back_as_before_plus_exactly_the_items_written`,
  `replace_reads_back_as_exactly_the_items_written`, `other_parts_and_findings_are_as_before`).
- **U8** thread pitch and 'number of threads': *open*, a maintainer decision
  ([question 6](#questions-2026-10-09)).
- **U9** part display names and one read: *met*, `PartDefinition.name` and `StepFile::read`;
  `tests/step_parts.rs` (`a_wrapped_part_takes_its_assemblys_name`,
  `an_empty_product_name_is_its_id`).
- **U10** notes on faces: *met* (above), `specify_core_notes_on_faces_are_anchored`.
- **U14** replace and remove of a written thread: *met* (above),
  `a_written_thread_is_replaced_and_removed`,
  `removal_takes_a_feature_definitions_shape_only_with_its_feature`.
- **U15** OpenCascade crash on datum symbols: *met* for specify-core's loader by decision 1
  (no relationship written; the crash itself is specify-core's `load.py`, question 1):
  `opencascade_reads_the_written_files` (`pmi_write.rs`) requires every written file, datum
  symbols included, to load in specify-core's `load.load_all` and read in XCAF.

### Questions (2026-10-09)

What was done meanwhile is in each.

1. **The crash is specify-core's, not OpenCascade's** (decision 1). With OpenCascade reading the
   relationship and specify-core's `load.py` crashing on a binding misuse, should the
   relationship come back (decision 6's full form) once specify-core's `_name` reads names
   through `TDF_AttributeIterator`? *Meanwhile:* left out, as decided; files open in
   specify-core as it is.
2. **Per-face usages through member shape aspects** (decision 2). The literal form, several
   usages on the feature's own shape aspect, violates UR2 and OpenCascade reads one face of it;
   the members are valid and OpenCascade reads every face. Is that the intended form? *Meanwhile:*
   members.
3. **Surface texture's mapping has defects** (decision 3). ISO 10303-1110 maps `Surface_texture`
   to a `surface_texture_representation`, whose WR2 and WR3 (one descriptive item, the
   'measuring method', and a measure) its mandatory 'material removal condition' cannot meet; it
   names the item of `characteristic_value` 'evaluation length' (the path of
   `evaluation_length`, copied); it gives a parameter no owner of its own. No practice and no
   NIST file settles them. *Meanwhile:* a plain `representation('surface texture')`, the value
   named 'characteristic value', the parameter on its texture's owner; the reader reads only
   that. And the mapping names the parameter's `property_definition` 'surface texture
   parameter', while `surface_texture_representation` WR5 (one `general_property_association`
   with the general property 'surface_condition') and `general_property_association` WR2 (the
   derived definition's name equals the general property's) require 'surface_condition': no
   name meets both. *Meanwhile:* 'surface_condition', so every written instance is
   schema-valid; the reader accepts either name. A reference file (CAx-IF or a CAD system's)
   would settle it.
4. **The material removal condition is mandatory** (ISO 1302, ISO 10303-1110) and specify-core's
   finish ('Ra 3.2') does not state one. *Meanwhile:* the model requires it; specify-core must
   state one ('any process allowed' is ISO 1302's basic symbol, which an unqualified 'Ra 3.2'
   on a drawing means).
5. **Several part notes, one shown by OpenCascade.** One 'semantic text' property per note is
   §7.4's form; one property with a line per note would make them one note (and whether
   OpenCascade would show more of it is untried). *Meanwhile:* one property per note.
6. **A thread's pitch and 'number of threads'** (specify-core-rust U8). The schema has no pitch
   item, and thread WR5's 'number of threads' is a ratio whose meaning (starts, or threads per
   unit length) neither the schema text read so far nor any file here settles. *Meanwhile:*
   the model keeps 'number of threads' as stated and holds no pitch; the writer writes what the
   model states (specify-core-rust writes 1, form 'M', the designation as `qualifier`, and the
   pitch, tapping drill and depths in an attribute set on the thread's feature, as haecceity's
   own test of specify-core intents does). Where should the pitch go, and what does 'number of
   threads' count?

## Out of scope for now

- **Graphic presentation** other than datum feature symbols (callouts, polylines, tessellated
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
