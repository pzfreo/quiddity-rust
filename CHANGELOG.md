# Changelog

All notable changes to quiddity-rust and haecceity. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Alpha 1's gates and their evidence are
in [docs/alpha1.md](docs/alpha1.md).

## Unreleased

### Added

- Derive semantic PMI axes from coaxial revolution faces and median planes from two
  parallel planar faces. Cross-check explicit constructive geometry, retain deriving face
  indices and cylinder radius, and report unsupported or disagreeing geometry precisely.
  Dimension and tolerance targets can reference these records; qualifiers remain data.
- PMI document format 2 (`haecceity-pmi-read/7`): optional `derived.geometry` references
  the new `derived_geometry` list. See [the record format](docs/pmi-derived-geometry.md).
  Existing version 1 PMI documents must be regenerated from STEP.

## 0.1.0-alpha.6 (2026-10-18)

### Changed

- Prism emptiness probes (used by native slot and recess recognition) now ask
  haecceity's bounded `fills_at_most(.., 0.0)` whether the prism holds any material,
  stopping at the first positive running volume, instead of computing the complete
  common volume (#16). Results are unchanged: unresolved-line semantics are kept, and a
  box that rounding collapses at extreme coordinates keeps the previous fraction-valued
  answer. In draftwright-rust#90, which combined this change with draftwright-side
  optimisations, planning NIST FTC-10's AP242 file fell from 203 s to about 5 s (most of
  the 203 s was spent in this probe), and the 65-part plan-time survey used 29% fewer
  instructions, with every plan JSON and SVG byte-identical.

## 0.1.0-alpha.5 (2026-10-17)

### Added

- Recognition document `quiddity-rust/recognition/3` retains Python-shaped
  `section_recesses` occurrences and source refusals, and now publishes
  `section_recess_patterns` (`SectionRecessArray`, `SectionRecessGrid`). Members are
  occurrence indices into `section_recesses`, never source-pocket indices. Pattern
  faces are the union of members' defining faces; frame mapping carries directions
  and the grid centre. Step levels and risers were already published in version 2.
  Existing record payloads and the K3 local-degradation behavior are preserved.

## 0.1.0-alpha.4 (2026-10-16)

### Added

- `PartDefinition.product_id`: the part's `PRODUCT.id`, escapes decoded, reported beside its
  index and display name (assemblies design D1).
- Tests on the assembly fixture downstream uses: a part read in its own coordinates, one part's
  PMI written alone (the other part's instances unchanged) with OpenCascade's reading of the
  result, and a nested sub-assembly flattened to its leaf parts.
- Python's local-degradation retry (`_take_inventory`): where the strict hole evidence refuses
  for want of one valid solid on a part whose solids are not all valid, `features::recognise`,
  `features::inventory`, `section_recess_projection` and `build_recognition_evidence` run again
  with every validity query answering as Python's degraded graph does, and holes, pockets,
  fillets, plates, step levels, risers, rectangular pads, slots, channels, prismatic pockets and
  edge-open recesses skip what Python skips there (`plates::discover_locally_degraded` is new). Corpus parts 13975 and 14052 now publish
  Python's section recesses (none; 14052's triangular pocket) instead of carrying the
  projection's refusal, and their inventories and evidence views match Python's.
- `RayCaster::hits_on` and `RayCaster::other_hit`: the hits on some faces alone, and whether a
  face other than some meets a ray within a stretch, answered without intersecting the other
  faces wherever that gives the same answer as `hits` (`None` otherwise).

### Changed

- Hidden-line projection is faster on parts whose faces stack up along the view (a thread seen
  along its axis, a comb's teeth): threaded_connector_109 renders in 4.1 rather than 23.5
  trillion instructions, cgb243 in 2.7 (draftwright #43 item 3). Every drawing is unchanged:
  each saving gives the same answer by construction. A stretch's visibility asks about only the
  faces it needs (`hits_on`, `other_hit`), skips asking its edge's own faces again when the ray
  is stopped past them anyway, and reuses a B-spline face's hits along a ray it has already
  asked; rays meet nearer boxes first; a B-spline face's grid cell is tested against its padded
  box before its triangles; a basis and its derivative come from one pass; a face domain's
  segment bins leave out segments parallel to the parity ray and stop below the ray's start.

## 0.1.0-alpha.3 (2026-10-14)

### Added

- Upgrading an AP203 or AP214 file to AP242 to add PMI now repairs two closed classes of
  defect that are also invalid in the file's own AP203 edition 2 schema, so six NIST AP203 files
  (CTC-01, CTC-02, CTC-03, CTC-05, FTC-06, FTC-09) take PMI: a `CC_DESIGN_APPROVAL` of a product
  definition becomes `APPLIED_APPROVAL_ASSIGNMENT`, and an inch or degree `NAMED_UNIT(*)` gets
  explicit `DIMENSIONAL_EXPONENTS` from its conversion factor's unit. Any other invalid
  instance is still refused. `WriteReport.repairs` and the CLI JSON report each repaired id, its
  kind and its original and new record text; every other byte is unchanged. Removing the PMI
  later does not restore the repaired records' original bytes (`docs/step-ap242.md` decision 1).
- `QUIDDITY_CORPUS=off` skips the corpus tests for a quick local run.

### Fixed

- haecceity reads hyperbola edges exactly, as one rational quadratic arc between the edge's
  vertices, when both vertices lie on the curve. Such edges were previously unresolved and their
  solids invalid, which turned off recognition on them (155 MFInstSeg models contain one).

### Changed

- The correspondence-under-motion check runs inside `tests/invariance.rs` on the same
  recognitions as the invariance check; the duplicate `cycle_xyz` corpus motion is dropped.
- The section-recess publication byte baseline is referenced on Linux, with the macOS values
  as alternatives.
- CI runs on pushes to `main`, now the only branch. Contributions are covered by a contributor
  licence agreement (`CLA.md`).

## 0.1.0-alpha.2 (2026-10-11)

### Added

- Publish `section_recesses` in `Features` JSON and as `SectionRecess` records in the
  recognition document consumed by draftwright (maintainer decision, 2026-10-11). Each record
  keeps Python's `index`, `body`, `geometry`, `classification` and `evidence`, its projection
  order and defining faces, and the existing caller/file coordinate mapping and `local` labels.
  Native recesses, uncovered prismatic pockets, passages, edge-open recesses, blind slots,
  pockets and channels use the existing reconciled projection; overlapping regions are
  published once. Existing source families and records are retained byte-for-byte.
- Schema note: this is an additive family under `quiddity-rust/recognition/2`, as for step
  levels and risers; the schema version is unchanged. Readers that validate a family
  allowlist must add `section_recesses`, `SectionRecess` and `SectionRecessRefusal` before
  updating its tag pin (the G7b reader stream). Refusals follow occurrences under the same
  family, preserving Python's source evidence and order for downstream lint warnings. Derived
  patterns remain available through the dedicated section-recess API.
  Corpus document tests compare occurrences and refusals on all 30 draftwright quality parts
  with captured Python records
  and preserve the existing verdicts for 15 incorrectly open NIST channels.

### Fixed

- Face mass no longer hangs on boundaries where the kink detector finds a new kink after every
  split (MFInstSeg 20221123_142528_16681): the kink-split budget is shared by the interval, so
  such a face now refuses (no area) instead of never returning.

## 0.1.0-alpha.1 (2026-10-10)

The first alpha: a pure-Rust port of Python quiddity's feature recognition, on haecceity, a
B-rep kernel and STEP/AP242 library that replaces OpenCascade.

### Added

- **haecceity kernel** (`crates/haecceity`, re-exported as `quiddity::kernel`): B-rep from STEP
  (via `step-io`) with face and edge provenance by `#N`; exact surfaces and curves (analytic,
  NURBS), face moments, volumes, rays and point classification, per-face geometric validity,
  hidden-line and section views for drawings (`hlr`), face triangulation (`Part::mesh`, `Part::triangulate`), a point on each
  trimmed face (`Part::face_anchor`) and the common area of two coplanar faces
  (`overlap::common_area`).
- **STEP and AP242 reading**: assemblies and distinct parts (`StepFile::read`, `read_part`,
  display names as OpenCascade XCAF gives them); a lossless Part 21 document (`p21`, byte-exact
  round trip and confined edits); the AP242 EXPRESS schema as data with an instance validator
  (`express`); semantic PMI read into a model (`pmi::read`: datums, sizes, locations, geometric
  tolerances, threads, knurls, material, general tolerances, notes, surface texture), checked on
  NIST's CAx-IF models and cross-checked against OpenCascade XCAF.
- **PMI writer** (`pmi::write`, add, replace and remove), verified by reading back
  (`pmi::verify`) and by OpenCascade reading the written files, following the maintainer's
  decisions of 2026-10-08 and 2026-10-09: material and general tolerance as specify-core writes
  them; no `mechanical_design_and_draughting_relationship` for datum feature symbols; one
  `geometric_item_specific_usage` per face; part notes as 'semantic text' and surface texture in
  AP242's surface conditions form; replace and remove of a written thread.
- **`express_rules`**: evaluated WHERE rules of the entities the writer emits (among them
  surface_texture_representation, general_property_association,
  mechanical_design_and_draughting_relationship) and the global rule
  `restrict_representation_for_surface_condition`; every written document is checked.
- **Feature recognition**: all 46 of Python quiddity's `recognise_*` entry points, each checked
  call by call against captured Python calls and over a 100-part corpus; `features::recognise`
  runs 45 families, step levels, risers and section recesses among them, as one reconciled
  inventory (Python's aggregate reconciliation); the recognition evidence view
  (`evidence_view::build_recognition_evidence`: constituent and host faces, hole-pattern members
  by position, section recesses); the section-recess family (`recognise_section_recesses`,
  `build_section_recess_document`) as library API; planar outer-profile evidence.
- **Correspondence**: revision-stable fingerprints, `correspondence::recognise` /
  `recognise_placed` (recognition in the part's own frame, reported in the file's coordinates),
  `correspond` between two revisions, and a versioned recognition document
  (`quiddity-rust/recognition/2`) for draftwright.
- **Browser builds**: haecceity runs its hidden-line work on the calling thread when no other
  thread can be spawned (wasm32-unknown-unknown), with the same results; `HAECCEITY_THREADS`
  caps the threads (e.g. 1 to time native work as a browser runs it).
- **Command line** `quiddity`: `parts`, `pmi read`, `pmi check`, `pmi write` (verified by
  reading back), `correspond` and `serve`, with versioned JSON.

### Known limitations

- Section recesses are in `Features` and correspondence but not in the recognition document
  (`quiddity <file>` lists their fingerprints without records); on corpus parts 13975 and 14052
  the family refuses where Python recovers by local degradation, and the refusal is carried, not
  published (docs/alpha1.md (b), questions 5 and 6).
- `Features` gives defining faces only and its patterns hold member copies; constituent faces
  and pattern members are in the recognition evidence view.
- The writer refuses datum targets, tolerance relations, the 'manufacturing requirement' note
  route, general tolerance tables and material density; PMI on assembly occurrences is reported,
  not modelled (`docs/step-ap242.md`).
- A thread's pitch has no place in the model and 'number of threads' is written as stated,
  pending a maintainer decision (`docs/step-ap242.md` question 6; U8).
- OpenCascade XCAF shows only the last of several part notes and no surface texture (pinned in
  `known_pmi_write.json`).
- `express_rules` evaluates only the WHERE rules named above and one of AP242's global rules
  (`GLOBAL_RULES`); it is not a general EXPRESS rule engine.
- Face triangulation refuses 2 corpus faces and the face anchor 1, each pinned in
  `crates/haecceity/tests/mesh.rs` and `anchor.rs`.
- Face moments differ from OpenCascade's adaptive integral (`BRepGProp`) by more than 1e-6 on 1358 faces
  (1345 rust-correct, 13 undetermined; `known_face_moments.json`).
- 13975's and 14052's local-degradation retry is not ported: their section-recess documents
  and recognition evidence views are refused where Python publishes (rust-wrong,
  `section_recesses/known_differences.json`).
- Every other difference from Python is listed with a verdict in the known-differences files
  the README's table names.
- CI takes 23 to 29 minutes on macOS (GitHub's 5 concurrent macOS jobs) and 11 to 16 on Linux
  (runs 38017180699 to 38031639949), against alpha 1's target of about 10 each.
