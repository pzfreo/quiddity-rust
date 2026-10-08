# AP242 and semantic PMI in haecceity

**Status:** design (2026-10-08), not yet implemented. Branch `ap242`.

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

## Requirements

What the two consumers do with STEP today, restated in standards terms. *specify-core* is
`/Users/paul/repos/specify-core/src/specify_core`; *draftwright* is `src/draftwright` on
draftwright's `origin/main`.

| # | Need (standards terms) | Where it comes from |
|---|---|---|
| R1 | Read a part's B-rep with a stable numbering of its faces, per distinct part of an assembly (a part placed twice is one part with two placements), and know which `advanced_face` instance each face is. | specify-core `load.py` (`load_all`, `parts`, `face_ranks`), `requirements.face_entities_of` |
| R2 | Read the product structure: product definitions, their names, the placements of their occurrences. | `load.py` (`_part_name`, `_instances`), `mates.py` (cylinders and planes placed per instance) |
| R3 | Read the semantic PMI already in a file and anchor every item to the part's faces: datums and their features, dimensions (size and location, nominal, ± deviations, limits, ISO 286 class), geometric tolerances (type, magnitude, zone, modifiers, datum system), whatever a datum feature is made of (several faces, a set). Every item on its own part. | `existing.py` (`read_existing`, `datum_faces`, `notes`, `part_settings`); `magnitudes.py`; draftwright `pmi.py`, `_pmi_part21.py` (`read_geometric_tolerances`, `read_datum_occurrences`, `read_dimension_associations`, `read_dimension_display_facts`, `read_dimension_length_factor`) |
| R4 | Read part-level information: material (name, density), general tolerances (ISO 2768), threads and knurls, notes and requirements, user defined attributes. | `existing.part_settings`, `existing.notes`; draftwright `read_material_properties`, `read_manufacturing_requirements`, `read_structured_manufacturing_requirements`, `read_surface_labels` |
| R5 | Read display facts that are semantic: decimal places of a value (`value_format_type_qualifier`), basic (theoretically exact) dimensions, dimension modifiers. | draftwright `read_dimension_display_facts`, `dimension_basic_policy`; specify-core `locations.mark_basic` |
| R6 | Write, for each part, the PMI of specify-core v1: datums A–H on one face, coplanar faces, or a hole or boss; size dimensions with ± deviations, limits or an ISO 286 fit; location dimensions (toleranced or basic); position (Ø, Ⓜ), flatness, perpendicularity, parallelism, profile of a surface, circular and total runout, each with its datum system; material; the ISO 2768 general tolerance; internal and external threads (designation, pitch, class, hand, tapping drill, depths); straight and diamond knurls (pitch, diameter). | `writer.py` (`_add`), `requirements.py` (`append`, `attributes`), `rules.py` (requirement kinds) |
| R7 | Write into an existing file as an edit: add PMI to a part that has none, add to a part that has some, replace the PMI of a part; keep every other byte of the file. | `merge.py` (`transplant`), `writer._write_verified`, `resume.py` |
| R8 | Store application data on a part (specify-core's answers) in a standard construct. | `resume.embed` |
| R9 | Verify a write by reading it back with the same reader, semantically, not by searching the text. | `writer.verify`, `writer._verify_appended` |
| R10 | Report what cannot be read rather than drop it: unknown or unsupported PMI entities, references that do not resolve, values without units. | draftwright `PmiExtractionReport`, `lint_pmi_extraction` |

Not needed by either consumer from the PMI layer: graphic presentation (specify-core writes
datum feature symbols only so that presentation-only viewers show something; draftwright draws
from semantics and reads no presentation except "common labels"), saved views, tessellated
presentation.

## Semantic model

A plain Rust data model in `crates/haecceity/src/pmi/`, independent of Part 21. One reader maps
AP242 to it, one writer maps it to AP242. All values are in the model's units: lengths in
millimetres, angles in radians, as `Length(f64)` and `Angle(f64)` newtypes; a dimension's kind
fixes which one its values are.

```rust
pub struct PartPmi {                       // the PMI of one product definition
    pub part: PartId,                      // its product_definition (file entity, or new)
    pub standard: Option<Standard>,        // ISO or ASME (PMI practice §4), with its tolerance principle
    pub features: Vec<Feature>,            // shape aspects: what PMI applies to
    pub datums: Vec<Datum>,
    pub dimensions: Vec<Dimension>,
    pub tolerances: Vec<GeometricTolerance>,
    pub general: Option<GeneralTolerance>,
    pub threads: Vec<Thread>,
    pub knurls: Vec<Knurl>,
    pub material: Option<Material>,
    pub notes: Vec<Note>,
    pub attributes: Vec<AttributeSet>,     // UDA practice, on the part or on a feature
}

pub enum Anchor { Face(FaceId), Edge(EdgeId) }        // a topological item of the part's shape
pub enum Feature {
    Items(Vec<Anchor>),                                  // one feature of one or more items (§6.5.1)
    Group { members: Vec<FeatureId>, kind: GroupKind },  // multiple elements, pattern of features,
                                                         // all around, between (§6.4)
    Derived { kind: DerivedKind, from: Vec<FeatureId> }, // axis, centre plane, centre point
    WholePart,                                           // all over (§6.3)
}
pub struct Datum { pub label: DatumLabel, pub feature: FeatureId, pub targets: Vec<DatumTarget> }
pub struct DatumSystem(Vec<Compartment>);                // primary, secondary, tertiary, in order
pub struct Compartment { pub references: Vec<DatumReference>, pub modifiers: Vec<DatumModifier> }
pub struct DatumReference { pub datum: DatumId, pub modifiers: Vec<DatumModifier> }

pub struct Dimension {
    pub kind: DimensionKind,            // Size(SizeKind) on one feature | Location{from, to, LocationKind}
    pub nominal: Value,                 // Length or Angle by kind, with optional decimal places
    pub tolerance: DimTolerance,
    pub qualifier: Option<Qualifier>,   // maximum, minimum, average (§5.2.2)
    pub modifiers: Vec<DimensionModifier>,
}
pub enum DimTolerance {
    None,
    Basic,                                   // theoretically exact
    Deviations { upper: Value, lower: Value },  // signed offsets from nominal, upper > lower (§5.2.3)
    Limits { upper: Value, lower: Value },      // value range (§5.2.4)
    Fit(Iso286Class),                           // tolerance class (§5.2.5)
}
pub struct Iso286Class { pub deviation: FundamentalDeviation, pub grade: ToleranceGrade }

pub struct GeometricTolerance {
    pub kind: ToleranceKind,            // the 15 ISO 1101 / Y14.5 characteristics
    pub feature: FeatureId,
    pub magnitude: Value,
    pub zone: Option<Zone>,             // form (Ø, sphere, …), projected, affected plane, runout orientation
    pub modifiers: Vec<ToleranceModifier>,   // Ⓜ Ⓛ Ⓕ Ⓣ Ⓟ Ⓤ, statistical, per unit, max value …
    pub datums: Option<DatumSystem>,
    pub unequal: Option<Value>,         // ISO Ⓤ displacement (§6.9.4)
}
pub struct GeneralTolerance { pub linear: Option<Iso2768Linear>, pub geometric: Option<Iso2768Geometric> }
pub struct Thread { pub feature: FeatureId, pub side: Side, pub designation: String, pub pitch: Length,
                    pub major: Length, pub class: String, pub hand: Hand, /* tapping drill, depths */ }
pub struct Knurl { pub feature: FeatureId, pub pattern: KnurlPattern, pub pitch: Length, pub diameter: Length }
```

**Invariants**, checked by constructors and by the reader (a violation is a reported finding,
never a silent fix):

- Ids (`FeatureId`, `DatumId`, …) index this `PartPmi`'s own vectors; every reference resolves.
- A datum has no position: precedence exists only in a `DatumSystem`. Labels are unique within
  a part; the same letter on two parts is two datums.
- A `DatumSystem` is a value: two equal systems are one system (PMI practice §6.9.7).
- `Deviations` and `Limits` have `upper > lower`; both may be on one side of nominal (g6, f7).
- `Iso286Class` is a pair of enums (`A`…`ZC`, `a`…`zc`; IT01…IT18): no other spelling exists.
- A tolerance kind that requires datums (perpendicularity, parallelism, runout, …) has them; one
  that forbids them (flatness, straightness, roundness, cylindricity) has none.
- Every `Value` has its quantity fixed by its role; a unit never enters the model.

**Anchoring.** An `Anchor` is the face or edge as the file states it: the instance id of its
`advanced_face` (or `edge_curve`) in the part's shape representation. That is what an AP242
`geometric_item_specific_usage` references, and it survives a write because the lossless layer
never renumbers existing instances. A new file's anchors are the ids its geometry was written
with. The B-rep reader records each `Part` face's source instance and, per distinct part, the
face numbering consumers use (R1); the two convert in both directions, and a face number from a
different file or reader version is refused (as specify-core's `Binding` does today).

## Architecture

```
            bytes ──▶ p21::Document ──────────────────────────────▶ bytes
                       (every instance's byte range; edits)        (untouched bytes copied)
                          │                     ▲
     step.rs (B-rep) ◀────┤                     │ Edit (add / replace / remove)
     + face provenance    │                     │
                          ▼                     │
                     pmi::read ──▶ PartPmi ──▶ pmi::write ──▶ express::validate
                     (+ findings)                              (every instance written)
```

1. **`p21`: a lossless Part 21 document.** Parses a file once into its instances, each with its
   id, its parsed record (step-io's `parser::RawEntity`) and the exact byte range of its text
   (from `#` through `;`), plus the header and section boundaries. step-io's spans mark only the
   `#N` token, so `p21` finds each instance's end itself (strings, `''` escapes and comments
   respected) and checks the ids agree with step-io's graph. An `Edit` holds additions (with
   provisional ids), replacements (same id, new record) and removals; applying one refuses a
   removal that is still referenced and an addition that references nothing that exists.
   Writing copies every untouched byte unchanged, writes a replaced instance in place, drops
   a removed one with its line, and appends additions before the `ENDSEC` of the last DATA section,
   numbered from the file's largest id upward in a deterministic order. New text uses the file's
   own line ending, and strings are encoded with Part 21 escapes (`\X2\…\X0\`), valid in every
   edition, so nothing else in the file needs re-escaping. A file whose `FILE_SCHEMA` is not
   AP242 (90 of the 100 corpus files are AP214, 10 AP203) is changed to AP242 in that header
   line, only when every entity type in it is declared by the AP242 schema; otherwise the edit
   is refused, naming the types. With no edit, output equals input byte for byte.
2. **`express`: the AP242 schema as data.** A table generated from the ISO 10303-242 EXPRESS
   long form by `tools/express_table.py` (its source file and sha256 recorded): every entity's
   supertypes, explicit attributes in order (inherited first, redeclarations applied), each
   attribute's type (simple type, enumeration values, SELECT members, entity, aggregate bounds,
   OPTIONAL). `validate(record)` checks a simple or complex instance: entity names exist,
   attribute count and types (typed parameters in SELECTs, enumeration values, reference
   targets' types, aggregate bounds), and that a complex instance's leaves form a legal and
   minimal set. The WHERE rules the writer depends on (e.g. `thread.WR1`,
   `default_tolerance_table.WR2`, `plus_minus_tolerance`'s uniqueness) are checked by named
   functions, each citing its rule. A general EXPRESS rule engine is out of scope.
3. **Face provenance** in `step.rs`: each `Part` face records its `advanced_face` id and its
   instance. step-io does not expose its id map, but fills each arena in ascending id order over
   the instances it keeps; the reader rebuilds the map from `p21` and the report's dropped ids,
   and checks every face against its record (bound count, surface reference type, sense),
   refusing on any disagreement. Per distinct part: its `product_definition`, name, placements,
   and its faces in the order consumers number them (OpenCascade's `TopExp::MapShapes` over the
   part's solid, which `Part`'s traversal order already matches).
4. **`pmi::read`** walks the `p21` records (not step-io's typed model, which drops `thread`,
   `turned_knurl`, `runout_zone_definition` and `default_tolerance_table`) from each part's
   `product_definition_shape` through shape aspects, `geometric_item_specific_usage` and
   `item_identified_representation_usage` to the anchors, as the PMI practice lays out. Every
   measure is converted by its own unit (the `measure_with_unit`'s unit entity, resolved to SI;
   a measure with no resolvable unit is a finding). It returns `PartPmi` per part plus
   `Findings`: unknown or unsupported PMI entities (by id and type), nonconformances it read
   through and how (e.g. NIST FTC-10's `LIMITS_AND_FITS('G6','hole','','')`, the grade in the
   wrong attribute), and references that do not resolve. Presentation entities are counted and
   listed as presentation, not interpreted.
5. **`pmi::write`** turns a `PartPmi` into an `Edit`: it reuses the part's existing anchors,
   contexts and units (adding an SI millimetre and radian unit only if the context has none),
   emits one entity form per concept from a single mapping table (below), validates every new
   or replaced instance with `express` before anything is written, and returns the edit for
   `p21` to apply. **Editing** is first class: `add` (new items beside the existing ones,
   resolving existing datum letters to the part's existing `datum`), `replace` (remove the
   part's PMI subgraph, the instances reachable from its PMI roots and from nothing else, and
   write the new one), and `remove`. Verification is `pmi::read` of the output compared
   semantically with what was written, plus `express` over every instance the edit touched.

**Mapping** (writer form; the reader also accepts the older forms the PMI practice says must
still be read):

| Concept | AP242 form |
|---|---|
| Feature on items | `shape_aspect` + `geometric_item_specific_usage` per item (§5.1, §6.5.1) |
| Group | `composite_group_shape_aspect` named 'multiple elements' / 'pattern of features', `shape_aspect_relationship` per member (§6.4) |
| Datum | one `datum_feature` and one `datum` per label per part, `shape_aspect_relationship` (§6.5) |
| Datum system | one `datum_system` per distinct system, `datum_reference_compartment` per position, never shared (§6.9.7) |
| Size / location | `dimensional_size` / `dimensional_location` + `dimensional_characteristic_representation` + `shape_dimension_representation` with 'nominal value' (§5.2.1) |
| Deviations | `plus_minus_tolerance` → `tolerance_value(lower, upper)` with `length_measure_with_unit`s (§5.2.3) |
| Limits | 'upper limit' and 'lower limit' items beside 'nominal value' (§5.2.4) |
| Fit | `plus_minus_tolerance` → `limits_and_fits(form_variance='H', zone_variance='hole'/'shaft', grade='7', source='')` (§5.2.5) |
| Basic, reference | `descriptive_representation_item('dimensional note','theoretical')` / `'auxiliary'` in the `shape_dimension_representation` (§5.3, Table 7); other modifiers grouped under 'modifiers' (Table 8) |
| Geometric tolerance | the leaf type (`flatness_tolerance`, …), complex only when modifiers or a zone require `geometric_tolerance_with_modifiers` etc. (§6.9) |
| Zone | `tolerance_zone` + `tolerance_zone_form` (§6.9.2); `runout_zone_definition` with its orientation only for a stated runout direction |
| General tolerance | `default_tolerance_table` of ISO 2768-1 cells ('lower limit', 'upper limit', 'plus minus tolerance value'), related to the 'default tolerance' representation by 'general tolerance definition' (schema `default_tolerance_table` WR2, cell WR2–5) |
| Thread / knurl | `thread` / `turned_knurl` (subtypes of `feature_definition`) with their `shape_representation_with_parameters` items as the schema's WHERE rules name them ('major diameter', 'pitch diameter', 'thread side', 'hand', 'fit class', …), linked to the feature's faces |
| Material | the CAx-IF material practice's 'material name' and 'density' properties |
| Attributes | UDA practice §5–7: `general_property` 'user defined attribute' |

## Anti-requirements

Each OpenCascade defect specify-core works around today (`writer.py`'s docstring, `merge.py`,
`existing.py`), and how this design makes it impossible.

| OpenCascade defect | Made impossible by | Checked by |
|---|---|---|
| Datum precedence held on the datum, so one datum label per letter per tolerance | `Datum` has no position; precedence is the order of `DatumSystem`'s compartments; the writer emits one `datum` per label per part and one `datum_system` per distinct system | a part with A\|B, B\|C\|A and D\|B\|C read back exactly; one `DATUM` per letter in the text |
| Lower deviation negated | `Deviations` holds signed offsets as the standard defines them; the writer copies them into `tolerance_value` unchanged; no API takes a magnitude | round trip of +0.1/−0.05 and of g6 (−0.009/−0.025) |
| Both deviations below nominal unreadable | the reader assumes no sign; the invariant is only `upper > lower` | g6 and f7 fixtures read; NIST STC-10 |
| ISO 286 grade written wrongly, so fits written as bare limits | `Iso286Class` is two enums, written as `limits_and_fits` letter and grade digits | Ø20 H7 and Ø70 g6 written as fits and read back as the same class |
| Tolerance length unit taken from the last dimension written (1000× errors) | every `Value` is in model units; the writer converts each measure by the unit it references, chosen per measure from the part's context; the writer holds no unit state | a part with tolerances and no dimension; a part whose context is in inches |
| PMI that has been read cannot be written back (hence the text transplant) | the file is never regenerated: `p21` keeps every instance's bytes, and the writer emits only new or replaced instances | add PMI to every NIST AP242 file and check every original instance is byte-identical and its PMI reads the same |
| Malformed `RUNOUT_ZONE_DEFINITION` | every written instance passes `express::validate` before anything is written; a zone definition is written only when the model states one | validator test with a three-attribute zone; every writer test validates its output |
| Untyped `MEASURE_WITH_UNIT` limits | the quantity is fixed by the value's role, so the writer emits `length_measure_with_unit(length_measure(…))` or the angle form; `express` rejects an untyped measure in a typed position | validator test; writer output scan |
| Complex entities where the simple form is expected | one mapping table chooses each form; `express` requires a complex instance to be minimal | validator test with a complex perpendicularity that needs no complex form |
| Datums imported only when referenced | the reader enumerates every `datum` and `datum_feature` of the part, used or not | fixture with an unreferenced datum |
| One datum per name per document (assemblies) | datums belong to a `PartPmi`; the writer resolves a label within that part's `product_definition_shape` only | an assembly of two parts, each with datum A on its own faces |
| Threads, knurls and the general tolerance appended as raw text | they are model types written through the same validated writer as their AP242 entities (`thread`, `turned_knurl`, `default_tolerance_table`) | round trip; `express` WHERE-rule checks for `thread` and `default_tolerance_table` |

Also gone: the presentation workarounds (`CommonLabel` crashes, presentations with no edges),
the schema-setting workaround, and UTF-8 written where escapes are required.

## Oracles and tests

In order of authority:

1. **The AP242 schema and the CAx-IF practices.** Every instance the writer emits is
   validated; the validator itself is tested on hand-written valid and invalid instances, and run
   over the PMI of every NIST AP242 file, its findings pinned with reasons (NIST says the files
   have errors).
2. **NIST's MBE PMI test models** (CTC 01–05, FTC 06–11, STC 06–10, AP242 editions 1–4, from
   `usnistgov/SFA` `Release/NIST-PMI-STEP-Files.zip`, in the scratchpad's `ap242-nist`). Their
   published PMI is the drawings in `PDF/` and, in the STEP File Analyzer's data, the expected
   PMI per model. Small checked fixtures (`tests/fixtures/ap242/`) hold the expected semantic
   PMI of selected models, each item transcribed with its drawing reference.
3. **The STEP File Analyzer and Viewer** checks semantic PMI against the practices but needs
   Windows (IFCsvr, tcom, twapi); it cannot run here. Its expected-PMI data is used if it can be
   extracted from the release; otherwise item 2's fixtures stand in.
4. **OpenCascade XCAF** through specify-core's venv: `tools/capture_pmi_occt.py` records what
   `STEPCAFControl_Reader` reads (dimensions, tolerances, datums, faces as entity ids). Every
   difference from haecceity's reader is listed in `tests/fixtures/known_pmi.json` with a verdict
   (`rust-correct`, `rust-wrong`, `equivalent`, `undetermined`, `not-applicable`) and its
   evidence, in the format of the other verdict files.

Tests: `p21` round trip byte for byte over the corpus and the NIST files; edits confined to
their spans; face provenance against OpenCascade's (`LoadedPart.face_ranks` mapped to `#` ids);
the reader on NIST fixtures and on specify-core-written files from corpus parts (inputs only:
how specify-core writes PMI is not a reference); the writer by validation, by read-back, by the
anti-requirement cases above, and by OpenCascade's reading of the result with verdicts;
determinism (same input, same bytes, fresh hash seeds).

## Out of scope for now

- **Graphic presentation** (callouts, datum feature symbols, polylines, tessellated
  presentation, saved views). It is derived from semantics and kept separate; neither consumer
  draws from it. Existing presentation in a file is kept byte for byte, but if it presents PMI
  that `replace` removes, that is reported, not repaired.
- **A general EXPRESS rule engine.** Structural validation plus named checks of the rules the
  writer depends on covers the defects above. A full WHERE/RULE evaluator is a project of its own.
- **Writing datum targets and composite tolerances.** They are read. specify-core v1 writes
  neither.
- **PMI on assembly occurrences** (instance-specific PMI through `assembly_component_usage`).
  Both consumers specify parts.
- **ISO 286 limit computation** from a class. Consumers have their own tables and the file
  states the class.
- **AP242 XML, external references, validation properties.**
