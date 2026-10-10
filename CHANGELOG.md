# Changelog

All notable changes to quiddity-rust and haecceity. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Alpha 1's gates and their evidence are
in [docs/alpha1.md](docs/alpha1.md).

## [0.1.0-alpha.1] (unreleased)

The first alpha: a pure-Rust port of Python quiddity's feature recognition, on haecceity, a
B-rep kernel and STEP/AP242 library that replaces OpenCascade.

### Added

- **haecceity kernel** (`crates/haecceity`, re-exported as `quiddity::kernel`): B-rep from STEP
  (via `step-io`) with face and edge provenance by `#N`; exact surfaces and curves (analytic,
  NURBS), face moments, volumes, rays and point classification, per-face geometric validity,
  hidden-line evidence, face triangulation (`Part::mesh`, `Part::triangulate`), a point on each
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
  runs 45 families, step levels, risers and section recesses among them, as one reconciled inventory (Python's
  aggregate reconciliation); the recognition evidence view
  (`evidence_view::build_recognition_evidence`: constituent and host faces, hole-pattern members
  by position, section recesses); the section-recess family (`recognise_section_recesses`,
  `build_section_recess_document`) as library API; planar outer-profile evidence.
- **Correspondence**: revision-stable fingerprints, `correspondence::recognise` /
  `recognise_placed` (recognition in the part's own frame, reported in the file's coordinates),
  `correspond` between two revisions, and a versioned recognition document
  (`quiddity-rust/recognition/2`) for draftwright.
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
  and recognition evidence views are refused where Python publishes (rust-wrong, `section_recesses/known_differences.json`).
- Every other difference from Python is listed with a verdict in the known-differences files
  the README's table names.
- CI on macOS takes about 24 minutes (GitHub's 5 concurrent macOS jobs); Linux about 11.
