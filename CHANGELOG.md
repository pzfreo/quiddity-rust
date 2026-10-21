# Changelog

All notable changes to quiddity-rust and haecceity. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Alpha 1's gates and their evidence are
in [docs/alpha1.md](docs/alpha1.md).

## Unreleased

### Added

- A B-rep STEP writer, `haecceity::brep_write::write_step`: a loaded `Part` (every solid,
  voids included, and faces in no solid; every surface and curve kind the reader holds,
  NURBS, parabolas, hyperbolas and swept surfaces included; pcurves; orientation; placed
  instances in components placed as the file placed them) written as a new AP242 file through
  step-io 0.3's `StepBuilder`, deterministic, with its semantic PMI carried through
  `pmi::write` and every item not carried named in the report. Reading the written file gives
  the same part bit for bit and the same recognition; geometry that did not resolve is refused
  by name. `quiddity brep <file.step> -o <out.step>` writes a file's part;
  `tools/check_brep_write_occt.py` checks written files against OpenCascade. Design, guarantee
  and refusals: [docs/brep-writer.md](docs/brep-writer.md). For files we were given, PMI edits
  stay byte-preserving (`pmi write`).
- `pmi::without`: a part's PMI less some items and everything that refers to them (moved from
  the PMI round-trip test). `Part::file_uncertainty`: the source file's length uncertainty.

- Read parabola edges and faces bounded by them. Like hyperbolas, finite parabola spans
  use exact quadratic B-splines for evaluation, sampling, bounds and mass/area integration;
  rigid transforms retain their native conic geometry. OCCT trim points take precedence
  over conflicting parabola parameters; parameter-only trims use ISO 10303-42's
  dimensionless parameter `(F*u*u, 2*F*u)` because no point establishes a writer's convention.

- AP242 PMI writing of composite, precedence and simultaneity tolerance relations; stated
  default-tolerance table cells and decimal places within them; material density in its own
  dimensionally correct derived unit; edition 4 relationship targets and affected-plane links.
- Legacy manufacturing-requirement notes migrate to semantic text or unambiguous surface
  texture. `standard_form` exposes the expected requirements and the CLI write report adds
  `note_migrations`. Replace removes legacy properties and their owned presentation. Reader
  binding `/9` holds affected planes and table qualifiers and separates relation targets from
  feature composition; JSON document version 2 stays compatible.
- Regression fixtures for the new forms, five ordered numbered part notes across replace and
  resumed add, and Python specify-note migration. Literal target and standalone-decimal
  schema/practice conflicts use the reported exemptions authorized on 2026-10-20 (evidence
  in `step-ap242.md`).

- Write every model datum-target shape and the datums it establishes (PMI practice §6.6),
  and decimal-only Figure 3 tolerance cells/defaults. `practice_schema_conflicts` reports the
  exact added instance/rule exemptions authorized by the maintainer (target WR5 only, decimal
  cell WR3/WR4). Structural validation and every other rule remain mandatory; raw rules and
  NIST file-wrong pins are unchanged. Reader binding `/11` projects anonymous decimal-only
  carriers to part defaults and keeps semantic text on targets separate from parameters.
- Synthetic target/precision fixtures, the 13 CTC-02/FTC-10 target clean-copy round trips,
  and OCCT cross-checks including shared target supports; no target-refusal stripping.

### Changed

- Haecceity now uses step-io 0.3, including its read-only model API, streaming readers,
  typed conics and shell/face/bound/edge-use `Orientation`. Haecceity's B-rep orientation
  flags and public geometry API remain unchanged. Hyperbola spans retain the existing
  arithmetic, using step-io's typed geometry as their source.
  **Public Part 21 API:** `p21::Attribute::String` and `Document::graph()` now contain
  decoded Unicode text. Pass plain text to `Attribute::String`; `p21::encode` escapes
  it on output. `p21::escape`/`decode` still convert encoded string bodies, and must
  not be applied to already decoded attributes. Re-exported parser spans use `u64`
  byte offsets, as in step-io 0.3. The raw graph's schema names are now the
  `schema.raw: Vec<String>` field (formerly `schema.raw()`), and its upstream
  `Graph` type is non-exhaustive, so callers cannot construct it with a struct literal.

- One supported recognition entry point: `correspondence::recognise(&Part)` recognises in the
  inferred part frame using an in-memory rigid transform, without a reader closure or second
  STEP read. Haecceity's `Part::transformed` preserves topology and source indices and rebuilds
  derived caches lazily; non-rigid placements and reflections are refused.
  **Upgrading:** `recognise` now returns framed records. Some fields are `local` on parts not
  axis-aligned in their file; consumers must read them through `frame`. `recognise_placed` is
  gone. Caller-space `features::recognise` and `recognise_<family>` are no longer public in
  default builds; the hidden, non-default `python-parity` feature is test-only and unsupported.
  Discovery and inventory functions, `Context`, evidence builders and lower-level feature
  algorithms are likewise available only with `python-parity`. Feature record types such as
  `HolePattern` remain public at their existing paths. Parity integration test targets require
  `--features python-parity`; `frames::prepare_framed*`, `working_part`, `FramedPart`,
  `FramedError` and `FramedPart::recognise` are feature-only, as is `RecordFrame::CallerSpace`.
  `Part::transformed` is new public haecceity API. Both `read_step_placed` and
  `read_step_file_placed` now also reject non-orthonormal and non-finite placements, in
  addition to reflections.

- Chamfers, fillets and circular blind steps recognise perpendicular supporting planes rolled
  about a principal run axis, using the planes’ geometry. Motion tests and tighter breakdown
  limits cover the recovered features; the default recognition route is unchanged.
- Recognition no longer depends on a part's placement or on input order where ADR 0011 says it
  must not: every first-wins, sign or ordering selector was audited and corrected (holes and
  counterbore stacks, bosses, recesses and channels, plane corners, patterns, turned axes,
  circular patterns, thin walls, voids, coordinate clusters, frames, oriented chamfers). A
  remaining arbitrary choice is a presentation gauge, never a semantic one; only two readings
  that would publish different features are refused. Identical counterbores at both ends of a
  hole keep their dimensions and claim both mouths. A per-file, per-family inventory floor over
  1,453 parts guards against losing features; the few changed counts each carry OpenCascade or
  analytic evidence.
- **Public `recess_reduce::merge_proposals` now refuses equal-width role ties with
  different run axes when no body extents are available.** Previously input order selected
  an interpretation. Part-aware recognition uses intrinsic body extents to accept a unique
  reading; symmetric competing claims remain refused (ADR 0011). This changes the public
  reducer's behaviour, without changing the document schema.
- quiddity-rust and haecceity are relicensed from AGPL-3.0-only to Apache-2.0 by the copyright
  owner, matching Python quiddity. `LICENSE`, `NOTICE`, the CLA and CONTRIBUTING say so.
- Both crates are ready for crates.io: descriptions, repository, keywords and categories, an
  `include` list (sources, README, CHANGELOG, LICENSE, NOTICE; no test fixtures), and an exact
  version on quiddity's `haecceity` dependency. `cargo package --workspace` packages and
  verifies both (haecceity 520 KiB, quiddity 505 KiB compressed).

### Fixed

- Repair consistently inward closed solid boundaries on STEP import, reporting each
  affected solid in input repairs and recognition warnings. Mixed or unproved
  orientations remain unchanged (#53).
- Restore regression coverage for original-input note indices in refusals, reporting
  semantic-text and surface-texture requirements on unwritable features by their own items.
  Narrow datum-target exemptions to WR5; WR4 remains mandatory because every emitted
  target name is empty.

- Isolate relationship tolerance endpoints so OCCT composition traversal leaves other
  tolerances on their original faces. Correct the earlier projection-origin diagnosis.
- Distinguish affected intersection line/plane and orientation plane descriptions (§6.9.2.1)
  with optional `Zone.affected_plane_kind`; ambiguous variants are refused. Reader binding
  becomes `/10`; the JSON addition is optional. Rust literals of `Zone` require the new field.
- Keep unitless legacy roughness as semantic text rather than invent micrometres. Report
  every supplied note callout and link removed callout ids/types to their original note in
  `note_presentation_removed`. `WriteReport` is now `#[non_exhaustive]`.
- Check and name tolerance relationship cycles as `geometric_tolerance_relationship.WR3`.
  Clarify that omitting a target description passes rules but loses its shape. OCCT
  affected-plane verdicts are not-applicable.

- Preserve a single sheet when converting radial `SURFACE_OF_REVOLUTION` line
  profiles, and recover boundary inversion stalled at a collapsed surface side.
  This corrects revolution face fluxes and mass properties without discarding
  their prescribed orientation (#53).

## 0.1.0-alpha.9 (2026-10-20)

Upgrading: fingerprints are now `quiddity-rust/fingerprint/3` and `correspond` refuses
version 2, so stored alpha.8 fingerprints must be regenerated from the STEP file (#51).
Feature IDs keep their `family/n` form and most are unchanged, but IDs are positional and
some records are added, removed, reordered or claim different faces (see Changed), and many
published sizes move. Re-recognise rather than reuse stored alpha.8 IDs, faces or dimensions.

### Added

- Add recognition document trust metadata: exact input-file SHA256, completed inventory
  mode, and strict per-record evidence status. Patterns inherit member proof; source
  refusals remain unproved. The fields are additive: documents keep their 3/4 schema tags and
  existing records, but a `/3` document is no longer byte-identical to alpha.8's; readers must
  ignore unknown fields. See [the trust contract](docs/recognition-trust.md) (#28–#30).
- `InteriorVoidRefusal` records: a bounded-air candidate whose sampled widening behind a throat
  is not established, previously dropped silently, is kept in the document's `interior_voids`
  family after the recognised voids, with `reason: "widening_behind_throat_unproved"` and its
  complete measurements. Its `faces` are the candidate's skin; it is `unproved`, claims no
  recognised feature, has no fingerprint and does not appear in the top-level
  `interior_voids` array. A document containing one is tagged `quiddity-rust/recognition/4`.
  `recognition::RECORD_TYPES` lists it under `interior_voids`. `Record::is_refusal()` covers
  it, so CLI and `serve` warnings do not list it as an unproved record
  ([docs/recognition-4.md](docs/recognition-4.md),
  [docs/recognition-trust.md](docs/recognition-trust.md), #49).
- New API: `recognition::{EvidenceStatus, RunMode}`, `Record::is_refusal`,
  `RecognitionDocument::{source_sha256, run_mode}`, `Record::evidence_status`,
  `Recognition::source_sha256`, `Features::{run_mode, evidence_status,
  interior_void_refusals}`, `features::interior_voids::InteriorVoidRefusal`,
  `features::evidence::strict_status`, `features::policy::round_measurement`; in haecceity, `step::decode_step_file_bytes` and
  `FaceMoments::second_moments` (central second moments). Code constructing these structs
  literally must supply the new fields (#28, #29, #33, #46, #49).
- Rust-first independent-reference policy (README, CONTRIBUTING): the Python captures are a
  frozen regression record; new behaviour is judged against OpenCascade, analytic fixtures,
  MFInstSeg/MFCAD++ labels and motion invariance (#22).
- Labelled-dataset accuracy: `examples/mfinstseg_score.rs` scores face and instance
  precision/recall against MFInstSeg (or MFCAD++ with `--mfcadpp`) through the source-cited
  [class mapping](tests/fixtures/mfinstseg_mapping.json); `examples/face_order.rs` and
  `tools/check_dataset_face_order.py` verify face order against OpenCascade; `tests/mfinstseg.rs`
  ratchets a hash-pinned 200-model sample (runs only with `QUIDDITY_MFINSTSEG` set; dataset not
  committed), updated with `tools/update_mfinstseg_baseline.py` (#23–#25).
- Evidence tools: `tools/check_face_integrals.py` with `examples/face_integrals.rs` (live
  OpenCascade face integrals and an independent 3D-boundary integration), and
  `tools/centroid_evidence.py`, `tools/perimeter_evidence.py`, `tools/drawing_evidence.py` with
  `examples/centroid.rs` and `examples/trim_lengths.rs` (boundary-flux centroids, raw STEP trim
  lengths, 3D silhouettes) (#35, #41, #43).

### Changed

- Fingerprint version 3 (#51): sizes are measured from raw geometry before positions are
  rounded, with decimal half-way points within the coordinate floor rounded ties-to-even
  (e.g. 92.075 → 92.08; #46); equal pocket wall separations keep all walls and choose by face
  identity, so square-footprint pockets now claim all four walls (#45); a hole stack with equally
  wide distinct ends opens at the end its cylinder's own axis points to, not the end high in
  world space (#47); groove supports are found symmetrically at both floor ends, and a proved helical thread root (cgb243 face 224) is no
  longer also published as a groove (#48); void sampling uses a box-centred lattice, which
  changes sample counts and volume estimates (#49); plate face bands use the midpoint of their
  extreme planes at full precision, which changes `lo`, `hi` and thickness (#50). Coordinate
  clustering and level-riser tests now include the tolerance boundary (#45). See
  [docs/correspondence.md](docs/correspondence.md).
- Records that change on existing parts include: mfcadpp_regressions/11512's chamber on faces
  33–34 becomes an `InteriorVoidRefusal`; a GRM-03 riser on the level-tolerance boundary is
  no longer published; nist_ctc_03's two slots change order (#45, #46, #49).
- Under motion, the invariance ledger's `rust-wrong` entries fall from 22 to 0 and the
  correspondence ledger's from 53 to 2; both remaining are mfcadpp/10170's section-recess
  `run_interval` endpoints on the three-decimal grid (#45–#50).
- Face integration controls error on every component (area, first and second moments) in
  dimensionless coordinates. Stored areas, centroids and framed coordinates move by at most
  3e-8 relative on the 30 quality parts; nist_ctc_05's framed pocket length on the 60.325 tie
  moves between 60.32 and 60.33 (#33).
- haecceity `face_mass` and `face_moments` return `None` for a non-finite or non-positive
  integral instead of a zero or `abs()` value (#33).
- Parity ledgers: undetermined verdicts fall from 113 to 2 (#36, #42, #44), settled with
  OpenCascade, independent boundary integration, MFCAD++ labels and motion evidence; the two
  left are cgb242 face 483's area and moments. Settling them changes only verdicts and reasons.
- CLI and `serve` warnings now read `<path>: warning: <reasons>; retained N records; unproved
  records [ids]`. The reasons are unresolved geometry, no solid (shell geometry only), solids
  failing strict validity, a local-degradation retry, or unproved record evidence. Refusal
  records are excluded from the unproved list. A refused section-recess projection keeps its
  own `section-recess projection refused: <reason>` warning (#29).
- The frozen per-platform section-recess publication hashes are replaced by an in-run check
  that publication is additive to the same recognition without it (#33).

### Fixed

- cgb242's solid centroid is no longer refused after translation (#33).
- Correspondence anchors skip a face without an area instead of weighting it as zero (#34).

## 0.1.0-alpha.8 (2026-10-20)

### Added

- Recognise modelled (helical) screw threads (#20). A `modelled_threads` record carries its axis, side (internal or external), major and minor diameter, pitch, hand,
  number of starts and extent along the axis, and claims its flank, crest, root and runout faces
  with stated tolerances. Inconsistent geometry (e.g. varying pitch, unequal starts, missing
  opposed flanks) becomes a `modelled_thread_refusals` record with a machine-readable `reason`,
  claiming no faces; no ISO designation is inferred. Across the 100-part corpus five threads are found, on cgb243 (four)
  and threaded_connector_109 (one); unrecognised freeform faces fall from 74 to 66 and from 140
  to 4 on those parts, and the other 98 outputs are byte-identical. A diamond knurl and a
  round-wire spring are not recognised as threads; a shallow helical groove and a varying-pitch
  sweep are explicitly refused. See [docs/modelled-threads.md](docs/modelled-threads.md).
- New API: `analyse_modelled_threads`, `recognise_modelled_threads`, `ModelledThread`,
  `ModelledThreadRefusal`, `ThreadAnalysis`, `ThreadAxis`, `ThreadTolerances`; `Features` gains
  `modelled_threads` and `modelled_thread_refusals`; `evidence_view::PHYSICAL_FAMILIES` grows
  to 41 entries (#20).
- Recognition documents are `quiddity-rust/recognition/4` only when they contain thread records
  or refusals; all others stay `/3` byte for byte. `recognition::SCHEMA_VERSION` names `/4`;
  compare with `LEGACY_SCHEMA_VERSION` or `schema_version(&features)` (#20).

### Changed

- `haecceity::pmi::Accounting::presentation` counts graphic PMI only: annotation occurrences,
  draughting callouts, and the draughting models and presentation views that carry them.
  Geometry styling, colours, curve fonts, layers and shared representation contexts no longer
  count, so a geometry-only file reports no PMI presentation (e.g. NIST CTC-01 392 → 0; the
  CTC-01 AP242 file 843 → 71). The reader binding moves to `haecceity-pmi-read/8`; the JSON form
  stays version 2, and PMI JSON bound to `haecceity-pmi-read/7` is refused by `pmi check` and
  `pmi write`; re-run `quiddity pmi read` (#21).

## 0.1.0-alpha.7 (2026-10-18)

### Added

- Derive semantic PMI axes from coaxial revolution faces, median planes from two parallel
  planar faces, and a plane from a single planar face. Cross-check explicit constructive
  geometry: a point agrees with an axis when it lies on the axis or on a deriving surface.
  Retain deriving face indices and cylinder radius, and report unsupported or disagreeing
  geometry precisely. Dimension and tolerance targets can reference these records;
  qualifiers remain data.
- PMI document format 2 (`haecceity-pmi-read/7`): optional `derived.geometry` references
  the new `derived_geometry` list. See [the record format](docs/pmi-derived-geometry.md).
  The shared format version also moves the `quiddity parts` listing and write reports to 2.
  Existing version 1 documents must be regenerated from STEP.
- GRM-03 (a Gramel thumbwheel drive screw): all ten semantic dimensions now resolve to
  geometry (previously all refused), with nominals and qualifiers unchanged. NIST AP242
  output is unchanged; see [the integration evidence](docs/pmi-derived-evidence.md).

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
