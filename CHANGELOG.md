# Changelog

All notable changes to quiddity-rust and haecceity. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Alpha 1's gates and their evidence are
in [docs/alpha1.md](docs/alpha1.md).

## Unreleased

- **Conservative stock-wall support (for #100).** Plate edges also check parallel
  walls outside their slab over the same run, so a step shoulder beside a thin
  plate strip remains unexplained. Interior stock walls must be perpendicular to
  the plate and have complete, perpendicular original-stock boundary edges along
  their run and height; parallel cut ends and walls borrowing two stock end caps
  abstain. Walls entirely on an outward envelope plane retain their independent
  supporting-plane evidence and convex stock contacts.

- **Face accounting and record coverage (for #100).** Recognised convex corner
  treatments can supply replaced box corners; certified enclosed spline trims on
  native surfaces no longer veto stock evidence. New `plate_edge` and `stock_wall`
  reasons require plate perimeter/support and direct convex stock anchors, reject
  concave cut boundaries, and keep plate-only notches unexplained. Drill-point
  holes claim their terminal cones; rounded rectangular slots claim both proved
  end walls after reconciliation, preserving feature selection and the inventory
  floor. Schema remains recognition `/6`, fingerprints `/3`; affected feature
  fingerprints change faces, centroid and adjacency, with correspondence retained.
  CTC-01 has 90 feature, 20 body and 7 unexplained faces (formerly 86/0/31).
  The issue omitted four V-notch flanks and incorrectly assumed cap 104 was already
  claimed by `polygonal_bosses`; those stay unexplained. Four rectangular-notch
  side walls retain their existing step-level claims. No notch or boss cap is body.

## 0.2.0-alpha.2 (2026-10-24)

Upgrading from 0.2.0-alpha.1:

- **`oriented_slots`, `oriented_chamfers` and `oriented_slot_patterns` are gone** from the
  published document: their records are in `slots`, `chamfers` and `slot_patterns`, whatever
  the recognition frame. Feature IDs of formerly oriented records change; the schema stays
  `quiddity-rust/recognition/6` and fingerprints `/3`. See [docs/recognition-6.md](docs/recognition-6.md).
- **API.** The typed `Deref` view of `PublishedFeatures` is removed: use `families()` or
  serialisation.

- **Slot precision in turned frames (for #92).** Converted slots retain measured
  extents instead of rounding distances to 0.01 mm. Slot patterns use raw member
  centres for pitches and directions, with coordinate-floor axis snapping and
  face-stable array sense. File-axis slot extents keep their existing values.

- **One published family per feature kind (for #92).** Recognition schema 6 merges
  oriented slots, chamfers and slot patterns into their plain families, preserving
  proofs and counts. Plain records precede oriented records; IDs, pattern members,
  evidence and accounting follow the merge. Fingerprints stay version 3, but merged
  records' fingerprint values change within v3: family/ID, oriented chamfer `turned`,
  and oriented slot source-end proof traits. As with plates in #71, the version is
  unchanged. Correspondence accepts 0.1.1's oriented records and retains their old IDs
  in the pairs. See [recognition-6.md](docs/recognition-6.md) for field mapping and
  moving saved references by defining faces or correspondence. File-axis letters
  and the crate version remain unchanged.

## 0.2.0-alpha.1 (2026-10-24)

Upgrading from 0.1.1:

- **Recognition schema 6 (#92).** Published records have no `local` fields. An axis field
  (`axis`, `long_axis`, `width_axis`, `other_axis`, ...) is a file-axis letter as before, or a
  unit vector in file coordinates; a converted record carries an `origin` and its measuring
  axes. Only records that had `local` fields change; the top-level `frame` is provenance only.
  Readers must accept `quiddity-rust/recognition/6`: see [docs/recognition-6.md](docs/recognition-6.md).
  Fingerprints stay `quiddity-rust/fingerprint/3`.
- **API.** `Recognition.features` and `Mapped` use `PublishedFeatures`; `Mapped::local`,
  `fingerprinted_local` and `LocalFields` are removed. Plate `lo`/`hi` are `Placed<f64>` (#84).
- **Changed results.** Frames change on parts whose file axes win the new vote (#81): CTC-02
  and FTC-07 AP242 become axis-aligned, cgb217 and cgb241 change frame. Concave rounds are no
  longer convex fillets (#89). CTC-02's 49.5 mm rows are no longer a pattern (#82).
  Re-recognise rather than reuse 0.1.1 output.

- **Publish nonprincipal measurements in file coordinates (for #92).** Recognition schema 6
  removes `local`. Formerly frame-relative directions use unit vectors; relative coordinates
  have explicit origins and measuring axes. Ordinary file-axis records and intrinsic
  fingerprints retain their values. Recognition frame voting and oriented families are unchanged.

- **Decide fillet convexity from the face (for #89).** Both prismatic paths now require
  an outward-facing cylinder. cgb241 face 27 returns to its concave R20 blend: fillets
  fall from 2 to 1, with convex face 47 retained. cgb202 loses 17 inward-facing false
  fillets (296 to 279) and recovers 3 concave blends (59 to 62); CTC-03 loses 6
  inward-facing false fillets (17 to 11) and recovers 5 blends (10 to 15).
  CTC-02 rc/AP242 keep 21/20 fillets; FTC-07 rd/AP242 keep 16/16. The raw inventory
  also drops 13 concave candidates on 12 MFCAD++ parts, 21 on 13 holdout parts,
  21 on 18 captured fixtures, and 1 on golden_circular_blind_step. Every removal
  has per-face material-side evidence; no outward-facing fillet is removed.

- **Preserve tiny material rotations during recognition (for #81).** File-axis support
  preference keeps the original near-axis material representative; publication alone snaps
  within the coordinate floor. sm-hanger again retains its 2.42e-9-radian inferred tilt while
  publishing in file axes, and a 1e-6-radian copy retains all 6 blind pockets and its pattern.

- **Frame inference votes over complete direction sets (for #81).** Planar normals and
  cylinder axes each vote once per distinct coplanar plane or coaxial equal-radius cylinder; file axes win a tie, while genuinely rotated parts retain
  their intrinsic frame. Real 1° features no longer merge into the first direction read.
  CTC-02 AP242 fillets rise from 3 to 20 (rc has 21: its extra R1.093 feature is absent
  from AP242); FTC-07 AP242 fillets rise from 13 to 16 and polygonal bosses from 0 to 4,
  with all 5 plates retained. cgb202 fillets rise from 272 to 296 and cgb217 from 6 to 14;
  cgb207 recovers its missing file-axis fillet at a total of 20. All 43 Specify fillet
  regressions are recovered. Parallel chamfer support edge directions are compared without endpoint-order dependence
  after frame normalisation, recovering mfcadpp/10252's missing 18.694-mm, 0.54×45°
  chamfer (0 to 1), the 44th Specify regression. CTC-02 AP242 also gains 4 pockets (0 to 4)
  and step levels (2 to 13); its 4 former risers were outer envelope walls seen in the wrong
  frame, and are gone. FTC-07 AP242 step levels rise from 6 to 8. Mixed-direction parts can
  change frame and so directional families: cgb217 moves from 13.1° to 45° (98 against 96
  supports) and cgb241 from the file axes to 27.4° (20 against 18). On cgb241 that relabels
  two blends (faces 27 and 47) as fillets; face 27, a concave R20, is a blend again after
  #89, leaving only face 47 relabelled. Counting distinct surfaces rather than faces keeps
  mfcadpp_regressions/11512 aligned and unchanged despite split channel faces (15
  file-axis surfaces against 10 rotated). Measured analytic evidence accompanies every bounded framed-floor allowance; broader
  shallow-support fillet coverage remains a limitation.
- Plate endpoints are published once on the file's 0.001 mm grid, with exact face
  separation retained for fingerprints (for #84). Their `lo`/`hi` fields now use
  `Placed<f64>`, as other rounded positions do, retaining the original planes through
  subsequent motions. Nearby projecting details no longer
  extend a predominant plate skin within the opposing skin's transverse envelope:
  whistle_frame_reference is 7.0 mm thick again (for #85). The six supporting-plane
  corrections from #71 remain in place; only whistle's plate fingerprint thickness changes.
- **CTC-02's 49.5 mm rows are not published as a pattern (for #82).** Its six ø8.38
  holes at x = ±25, ±74, ±122 are spaced 48, 49, 50, 49, 48. Alpha.8 chose one of two
  mirror-image three-hole rows (spacings 49 and 50, mean 49.5) by input order; both share
  the two central holes and have equal support, so under ADR 0011 neither is published.
  No refusal record is emitted; every hole is still recognised, and the 13 other patterns
  remain. A regression test pins the part and its hole stations.
- **Identical countersinks at opposite bore mouths stay attached (for #83).** The ø6 hole
  in draftwright-rust's two-face fixture again links to its ø14, 90°, 4 mm deep seat;
  both countersink records remain (2). A through hole retains its symmetric seats and
  both sets of supporting faces. Its `csink` refers to the seat at the presentation end;
  competing seats at one mouth still refuse.

## 0.1.1 (2026-10-22)

Upgrading from 0.1.0:

- **Record position fields are `features::placed::Placed<T>` (#69).** A field that holds a
  rounded position (a hole's `location`, a fillet's `at`, a blend's `center`, and so on) is now
  `Placed<T>`. It dereferences, compares and serialises as the published value, so reading it
  and recognition documents are unchanged; code that builds records or destructures such a
  field takes the value with `*` and builds one with `Placed::exact` or `.into()`.
- **Changed results.** Six plates' `lo`/`hi` and fingerprint `thickness` change (#71; the
  fingerprint version stays `quiddity-rust/fingerprint/3`); CTC-04 loses its 12 false
  countersinks (#76); framed positions move by less than one grid step (#69); sm-hanger's
  records are published in file axes (#70). Re-recognise rather than reuse 0.1.0 output for
  these parts.

- **Chamfer corners are no longer countersinks (for #76).** A countersink's cone must now run
  at least 270° round its axis (all faces on that cone together). On NIST CTC-04 the 12 reported
  countersinks were the quarter-cone corner faces of the 5×45° pocket-rim chamfer, seated only on
  the pockets' R5 corner-radius walls; they are gone, and the corner faces are now `unexplained`
  in the face accounting (no record spans the rim round its corners). No other corpus part
  changes; whole cones over a bore are kept.
- **A join within one surface on the outline is judged as the silhouette it is (for #75).**
  Hidden-line projection judged a seam or same-surface join lying on the view's outline from
  the edge's curve, which a file may leave a hair inside the surface: NIST CTC-05's shaft
  outline (edge 375, 1.5e-8 inside its cylinder) sent the ray along the surface into the
  cylinder's other half, and the front view drew it hidden. Such an outline edge is now judged
  from 1e-5 of the part's size outside its first drawn face, as traced silhouettes are. Across
  the corpus only outline stretches change, in 11 parts: of 594 mm now drawn visible, 392 mm
  already lay under a visible line, 190.5 mm is CTC-05's shaft, 8.9 mm is in cgb203's side view
  (closer to OpenCascade) and under 1 mm is in each of six other parts.
- **CTC-05 is in the OpenCascade hidden-line comparison (for #75).** `tools/capture_hlr.py`
  keeps CTC-05 although its record exceeds the size limit; its four views agree with
  OpenCascade. Its edge 226 ends on a vertex 0.018 mm off its circle's plane, so the edge's
  exact circle strays up to that far from the stretch's points; the comparison allows that gap
  for CTC-05 only (`references/drawings.json`), and the circle is still drawn exactly.
- **A compound's child order does not change recognition; a test now pins it (for #73).** The
  1 channel against 2 in #73 is the file: draftwright-rust's forward-order `two-channels.step`
  holds one occurrence of the block, the reversed file both.
- **Faster STEP reading (for #72).** The inverted-shell repair (#59) no longer integrates
  every face of every solid on each read. It first looks for one face with an outward witness,
  by ray parity against boxes that certainly hold each face (no face interior sampled), which
  proves the shell is not inverted. The full test (sampled boxes, signed volume, every face
  inward, wire senses) runs only when that proof is not found. The same shells are repaired,
  with the same reports, part definitions and masses. Instructions retired, v0.1.0 → now
  (alpha.8 for reference), for `read_step` and `read_part_definitions`: nist_ctc_02 10.3G →
  0.9G (0.88G); nist_ctc_01 0.55G → 0.21G (0.20G); gramel string_post 0.90G → 0.27G (0.13G).
  string_post is still about twice alpha.8: the remaining cost is in the proof's rays, which
  build the B-spline surfaces' grids and edge deviations that recognition needs anyway, so a
  read that never recognises (such as a PMI-only read) still pays it.
  `read_part_definitions` still builds the B-rep and runs the repair: the PMI edge numbering
  depends on the repaired face senses, so skipping it would renumber edges on inverted inputs.
- **A plate's `lo` and `hi` are again faces of the part (for #71).** Since #50 a side whose faces
  lie on several nearby planes published the midpoint of their face band, a plane no face lies
  on (GRM-03's thumbwheel screw: thickness 2.25 between faces 2.5 apart). The midpoint still
  chooses the band; `lo` is now the lowest of the −axis side's planes and `hi` the highest of the
  +axis side's, unrounded, which exchange correctly when the part is reversed. Six of the
  corpus's 48 plates change, with their fingerprint `thickness`; the fingerprint version stays
  `quiddity-rust/fingerprint/3`, whose meaning (thickness is `hi − lo`) is unchanged.
- **Framed records are on their published grids (for #69).** A position a recogniser rounds
  (a fillet's `at` to 0.001 mm, a hole's location to 11 significant figures, a recess's
  `w_center` to 0.01 mm, a body key's box to 1e-6 mm) now keeps the value it was rounded
  from. Framed recognition carries that unrounded value through the frame's motion and rounds
  it once, in the file's coordinates, on the same grid; a decimal half-way point within the
  coordinate floor takes the even neighbour. Positions derived from published ones (pattern
  centres, a gusset mirror plane, an oriented slot's centre) are derived again from the
  published values. Values move by less than one grid step: NIST CTC 01's hole at
  `-124.99999999647548` is `-125.0`. Section recesses publish their frame origin on the
  plane through the file's origin across the run, with run coordinates measured from it.
  Sizes, counts, orders and fingerprints are unchanged (fingerprints read the values as found
  in the frame). Record fields that hold positions are `features::placed::Placed<T>`, which
  dereferences to the published value; build one with `Placed::exact` or `.into()`.
  Positions a recogniser publishes unrounded (a plate's faces, a boss's location, step and
  riser levels, sheet-metal and thin-wall bodies, freeform poles, a hole pattern's centre as
  the mean of its members) are carried unrounded, as before. Each end of an interval is
  rounded in the file on its own, so `hi - lo` can differ from the published size by one grid
  step where it did not before (and agree where it did not): read sizes from the size
  fields, not from interval ends.
- **No noise near zero in significant-figure positions (for #69).** A hole's `location` (11
  significant figures) and a circular blind step's points (6) kept round-off near zero, such
  as NIST CTC 01's holes at z = 4.3e-11. A coordinate within the kernel's coordinate floor
  (`COORD_FLOOR`, 1e-6 mm) of zero now publishes as 0. Other values keep their significant
  figures: hole patterns are found from the published locations, and rounding those to
  0.001 mm loses patterns.
- **Near-aligned frames are published in the file's axes (for #70).** Recognition still runs
  in the frame as inferred. Its records are published through the signed permutation of the
  file's axes nearest the frame when that moves no point of the part by more than the
  kernel's coordinate floor (`COORD_FLOOR`, 1e-6 mm; the rotation's distance from the
  permutation times the part's reach from the frame origin), so they map to the file's axes
  exactly instead of staying frame-local, and differ from the exact motion's by less than
  the floor. On `sm-hanger.step` (2.4e-9 rad off) the six fillets, the plate and the body
  keys are now in the file's axes; only step levels, read along the frame's z by
  specification, stay local. Recognition and fingerprints are unchanged: fingerprints read
  the records carried through the exact motion.

## 0.1.0 (2026-10-22)

Upgrading from alpha.9:

- **Recognition API (#61).** `correspondence::recognise(&Part)` is the one entry point and
  returns framed records: on a part that is not axis-aligned in its file, read `local`
  fields through `frame`. `recognise_placed` is gone, and caller-space
  algorithms are private: `features::recognise`, `recognise_<family>`, discovery and
  helper functions, `features::context::Context`, `recess_reduce::merge_proposals`, the
  reader-based `frames` helpers (`prepare_framed*`, `working_part`, `FramedPart`,
  `FramedRecognition`, `FramedError`) and `evidence_view::build_recognition_evidence`.
  Feature record types stay public. `RecordFrame::CallerSpace` exists only in crate tests,
  so drop it from exhaustive matches. The `python-parity` feature is removed (#67). `read_step_placed` and `read_step_file_placed` refuse
  non-orthonormal and non-finite placements.
- **Recognition documents are `quiddity-rust/recognition/5` (#66)** with a `face_accounting`
  array: every face is feature, refused, body or unexplained. Versions 3 and 4 still read,
  with accounting `None`. `Recognition` gains `input_repairs` and a private field, so it
  can no longer be built as a literal. `RecognitionDocument` gains `face_accounting` and
  `input_repairs` (#59, #66).
- **Records may differ from alpha.9's (#57, #58).** Placement- and order-dependent choices
  were corrected, and a few readings are now refused (ADR 0011).
  Fingerprints stay `quiddity-rust/fingerprint/3`, but re-recognise rather than reuse
  stored IDs, faces or dimensions.
- **haecceity uses step-io 0.3 (#62).** Part 21 strings are decoded text, spans are `u64`,
  and schema names are `schema.raw`.
- **PMI (#60).** Reader binding `/11`. `Zone` has a new `affected_plane_kind` field, and
  `WriteReport` is `#[non_exhaustive]`, and `PartProvenance` gains `note_callouts`. JSON
  documents stay version 2.
- **License.** Both crates are Apache-2.0 (#55).

New in 0.1.0: the B-rep STEP writer and `quiddity brep [--pmi]` (#64, #65), PARABOLA geometry
(#62), the inverted-shell repair (#59), revolution volumes (#56), and a regression record that
Rust owns, with Python no longer consulted (#67).

### Added

- Complete recognition face accounting: feature record IDs, named refusals and reasons,
  positively established body faces (`stock_box_side`, `stock_box_notched`,
  `stock_box_split`), or unexplained. The conservative
  geometry rules and independent MFInstSeg ratchet are in [docs/face-accounting.md](docs/face-accounting.md).
  No feature record or fingerprint changes.
  **Upgrading:** all newly emitted recognition documents, including `quiddity serve`, use
  `quiddity-rust/recognition/5` and include a `face_accounting` array indexed by file face.
  Read the group on each face; do not interpret missing accounting on older documents as
  an empty unexplained list. Versions 3/4 still deserialize with accounting `None`.
- A B-rep STEP writer, `haecceity::brep_write::write_step`: a loaded `Part` (every solid,
  voids included, and faces in no solid; every surface and curve kind the reader holds,
  NURBS, parabolas, hyperbolas and swept surfaces included; pcurves; orientation; placed
  instances in components placed as the file placed them) written as a new AP242 file through
  step-io 0.3's `StepBuilder`, deterministic, with its semantic PMI carried through
  `pmi::write` and every item not carried named in the report. Reading the written file gives
  the same part bit for bit and the same recognition; geometry that did not resolve is refused
  by name; source product names are kept. `quiddity brep <file.step> -o <out.step>` writes a
  file's part with its PMI, or with `--pmi <pmi.json>` with the PMI of a `quiddity-pmi`
  document bound to that file (edit PMI as JSON, write it with the B-rep to a new file);
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
  `note_migrations`. Replace removes legacy properties and their owned presentation. The reader
  binding holds affected planes and table qualifiers and separates relation targets from
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

- Rust owns its regression baseline, with source-commit provenance and deterministic gzip
  expectations. Regenerate with `python3 tools/rust_snapshot.py`, review decompressed changes
  with `--diff BASE_REF`, and verify current HEAD with `--check`. Python is no longer consulted.
- Removed Python recognition/PMI oracle outputs, capture-only tools, parity harnesses and the
  `python-parity` Cargo feature. Caller-space algorithms are private; their internal tests
  retain independent reference checks. The MFInstSeg scorer compiles the shared private engine
  without a feature flag, preserving its scoring semantics and label floors. Python-written inputs, live OCCT checks, NIST/design-intent,
  analytic, label, motion and inventory-floor tests stay. Independent ledgers live under
  `tests/fixtures/references/`; retired verdicts are archived with evidence and commits in
  `docs/history/python-parity.md`.

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
  builds. Discovery, inventories, context construction and caller-space helpers are private;
  feature record types remain public. Internal unit tests exercise these algorithms without
  a feature flag. The old reader-based frame helpers and `RecordFrame::CallerSpace` exist
  only in crate tests.
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
- **Recess proposal merging (`recess_reduce::merge_proposals`, now private) refuses
  equal-width role ties with different run axes when no body extents are available.**
  Previously input order selected an interpretation. Part-aware recognition uses intrinsic
  body extents to accept a unique reading; symmetric competing claims remain refused
  (ADR 0011). The document schema is unchanged.
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
  with optional `Zone.affected_plane_kind`; ambiguous variants are refused. The JSON
  addition is optional. Rust literals of `Zone` require the new field.
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
