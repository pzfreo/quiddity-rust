# Alpha 1 readiness: quiddity-rust and haecceity

The maintainer's alpha 1 gates for this repository are section 2 of draftwright-rust's
`docs/alpha1.md` (2026-10-09): (a) haecceity's PMI writer per the 2026-10-09 decisions, each
verified by read-back and an OpenCascade cross-check; (b) all 46 recognisers ported, section
recesses wired into recognition; (c) specify-core-rust's upstream needs
(`specify-core-rust/docs/upstream-needs.md`, U1–U15) done or explicitly deferred with a reason;
(d) CI green on Linux and macOS, each around 10 minutes.

**Checked at** `c2ce06a` (origin/prototype/rust-port, 2026-10-10), and brought up to `a2e9721`
when it was merged (status round 34): that head adds step levels and risers to `Features`, the
recognition evidence view and 19 of 21 refused faces meshed (`36483c1`), and section-recess
fixes (`0324cc4`). Every *met* line names tests run green:
`QUIDDITY_CORPUS=… QUIDDITY_CORPUS_REQUIRED=1 cargo test --workspace --release` on arm64 macOS
at the merge (status round 34 gives the counts; at `a2e9721`, 466 passed, 0 failed, 3 ignored:
the development aids `export_written_files`, `dump`, `face_moment_evidence`). *In progress*
names the stream whose branch moves the item; *deferred* names the concrete blocker. Questions for the maintainer are under
[Questions](#questions).

| Gate | Status at `a2e9721` |
|---|---|
| (a) PMI writer decisions | **met** |
| (b) 46 recognisers, section recesses wired | **not met**: all 46 ported as functions; 2 not in `Features` (section recesses: in the recognition evidence view, wiring into `Features` is the backlog item; passages: Python's result has no such field) |
| (c) U1–U15 | **not met**: 11 met (U1, U2, U3, U4, U5, U6, U7, U9, U10, U14, U15), U11 (a) met; 1 partly met (U12: section recesses not in `correspondence::recognise`); 3 deferred on a maintainer decision (U8, U11 (b), U13) |
| (d) CI green, about 10 minutes each | **Linux met** (10.8 min), **macOS not met** (24.2 min; blocked on a runner decision); q-fills-at-most-callers in progress |

## (a) PMI writer, decisions of 2026-10-09

Landed by `c228f95` (stream q-pmi-writer-occt), with `express_rules` checks from `9dc29b4` and
`60645bf`, and the cross-check of the U14/U10 written files from `d4e9a45` (stream
q-pmi-occt-thread-notes, `b604d72`, `6e7f9d3`). The written files OpenCascade reads are
`tests/fixtures/ap242/write/*.step.gz`, each with its capture `*.occt.json.gz` (`tools/check_pmi_occt.py`, specify-core's venv, OCP 7.9.3.1):
`every_kind`, four specify-core intents written onto their corpus files
(`assembly_plate_pin`, `spool_fits`, `string_post_tapped`, `thumbwheel_thread_knurl`), a written
thread replaced and removed (`thread_replaced`, `thread_removed`; U14) and an add beside
specify-core's notes on faces (`thumbwheel_notes_add`; U10). All eight captures record `loads: true` (specify-core's
`load.load_all`) and `reads: true` (XCAF).
`written_files_are_the_writers_current_output` holds the committed files to the writer's
current bytes, so the captures are of today's output; `opencascade_reads_the_written_files`
requires each capture to load and read, every datum to have its symbol as presentation, and
every difference to be pinned (`known_pmi_write.json` "occt": 6, 4 rust-correct and 2
not-applicable).

- **No `mechanical_design_and_draughting_relationship` for datum feature symbols**: met.
  Read-back: `datum_feature_symbols_go_with_their_datums` (its `datum_symbols` asserts the
  symbols' draughting model is related by no such relationship). OpenCascade: every datum of
  `every_kind` and the intents read with its symbol. None of the eight written files contains
  the entity (counted in the decompressed files).
- **One `geometric_item_specific_usage` per face**: met. Read-back:
  `a_feature_of_several_faces_has_a_usage_per_face`, `add_composes_a_feature_of_an_aspect_of_its_faces`.
  OpenCascade: `every_kind`'s two-face profile tolerance and the intents' multi-face sizes and
  positions read on all their faces. No written file contains a `set_representation_item` or a
  plain `item_identified_representation_usage`.
- **Notes and surface texture in standard forms**: met. Read-back and rules:
  `notes_and_surface_textures_are_standard_forms` (through `express_rules::check_all`), and
  `express_rules`' `surface_texture_rules_on_a_written_document`,
  `surface_condition_rule_on_a_written_document`,
  `draughting_relationship_rules_on_a_written_document`. OpenCascade: `every_kind`'s three part
  notes ('semantic text') load; XCAF shows the last only (pinned rust-correct) and has no surface
  texture (pinned not-applicable), so surface texture's cross-check is that the file loads and
  reads, not that OpenCascade reports the texture.
- **Thread replace/remove (U14) and face notes (U10) written files**: met. `thread_replaced`,
  `thread_removed` and `thumbwheel_notes_add` load and read; XCAF has no thread semantics, so
  the threads' cross-check is that the files load and read. `thumbwheel_notes_add`'s callouts
  read on the faces the reader puts each note on; its one difference is specify-core's own kept
  Ø10 (`#651`, not written by haecceity), OpenCascade's known misread of two deviations below
  nominal (pinned rust-correct).

## (b) Recognisers

Python quiddity (`/Users/paul/repos/quiddity/src`) defines 46 `def recognise_*` entry points
(36 exported from `quiddity/__init__.py`; the other 10 are the recess source families and
passages: `channels`, `edge_open_circular_pockets`, `edge_open_prismatic_recesses`, `passages`,
`pocket_patterns`, `pockets`, `prismatic_pockets`, `rectangular_blind_slots`,
`round_bottom_blind_slots`, `section_passages`). All 46 have a `pub fn recognise_*` of the same
name in `src/features` (compared by name), each checked against captured Python calls
(README's family table; section recesses in `tests/section_recesses.rs`, 400 of 414 documents
agree at `c2ce06a`; 19 known differences at `a2e9721`, 24 before `0324cc4`).

`features::recognise` (and so `correspondence::recognise`, `recognise_placed` and the
recognition document) carries 44 families: step levels and risers (Python's
`RecognitionResult.step_levels` and `.risers`) joined at `36483c1` (`204b29f`). Not in
`Features`:

- `recognise_section_recesses` (Python's `RecognitionResult.section_recesses`, with
  `section_recess_refusals` and `section_recess_patterns`): library API
  (`section_recess_family.rs`, `9ec0209`) and, since `36483c1` (`e4b0fd8`), listed in the
  recognition evidence view (`evidence_view::build_recognition_evidence`), but not in
  `Features`, `correspondence::recognise` or the recognition document; that wiring is the
  backlog item. Whether the
  recognition document then carries section recesses instead of section passages is an open
  maintainer question (status round 25).
- `recognise_passages`: not applicable to the gate; Python's `RecognitionResult` has no
  passages field (its passages are published through the section-recess projection; the legacy
  result's `section_passages` is in `Features`).

## (c) specify-core-rust's upstream needs

Each against its "met when" in `upstream-needs.md`.

- **U1** a part in its own frame: *met*, `step::read_part` (`900c556`); `step_parts.rs`
  `an_assembly_part_is_read_in_its_own_coordinates`, `nist_parts_are_their_files_read`,
  `a_part_of_another_file_is_refused`.
- **U2** constituent faces and pattern members: *met* by the recognition evidence view
  (`evidence_view::build_recognition_evidence`, `b332e85` and `e4b0fd8`, merged `36483c1`):
  constituent and host faces per accepted feature, hole-pattern members as positions;
  `recognition_evidence.rs` `view_agrees_with_python`, `view_is_placement_independent`. The view
  refuses 13975 and 14052 outright (rust-wrong, local-degradation retry not ported). `Features`
  itself still gives defining faces only and patterns hold member copies.
- **U3** notes and surface finish: *met* in the decided forms (`c228f95`);
  `notes_and_surface_textures_are_standard_forms`. `PartPmi.notes` (the 'manufacturing
  requirement' route) stays refused by decision 5.
- **U4** face triangulation: *met*, `Part::mesh` / `Part::triangulate` (`d691df3`);
  `mesh.rs` `corpus_faces_mesh_watertight_outward_and_within_deflection`,
  `fixture_faces_mesh_watertight_alone_as_in_the_part`. 2 corpus faces are refused, each pinned
  (`KNOWN_REFUSALS`: cgb202 face 399, 14052 face 1); 21 before `352a860` (merged `36483c1`).
- **U5** coplanar overlap area: *met*, `overlap::common_area` (`b0f42f3`); `overlap.rs`
  `the_stacks_common_areas_are_the_exact_areas`, `what_cannot_be_measured_is_refused`.
- **U6** a point on the trimmed face: *met*, `Part::face_anchor` (`b0f42f3`); `anchor.rs`
  `every_corpus_face_has_a_point_on_its_trimmed_region`, 1 corpus face refused and pinned (cgb202 face
  399; 4 before `352a860`).
- **U7** read-back verification: *met*, `pmi::verify` (`900c556`); `pmi_verify.rs`
  `add_reads_back_as_before_plus_exactly_the_items_written`,
  `replace_reads_back_as_exactly_the_items_written`, `other_parts_and_findings_are_as_before`.
- **U8** thread pitch and 'number of threads': *deferred*, blocked on the maintainer's answer
  to `docs/step-ap242.md` question 6.
- **U9** display names and one read: *met*, `PartDefinition.name`, `StepFile::read`
  (`900c556`); `step_parts.rs` `a_wrapped_part_takes_its_assemblys_name`,
  `an_empty_product_name_is_its_id`.
- **U10** specify-core's notes on faces: *met* (`0b56d47`); `pmi_read.rs`
  `specify_core_notes_on_faces_are_anchored`; OpenCascade reads `thumbwheel_notes_add`
  (`d4e9a45`, `opencascade_reads_the_written_files`).
- **U11** face moments to 1e-6: (a) *met*, every corpus face has moments (`f0e2104`;
  `face_moments.rs` `cgb217_face_29_has_moments`). (b) *deferred*, blocked on a maintainer
  decision: `face_moments_match_opencascade_adaptive` pins 1358 faces where haecceity and
  OpenCascade's adaptive integral differ by more than 1e-6, 1345 rust-correct (an independent
  Green's-theorem integration agrees with haecceity) and 13 undetermined
  (`known_face_moments.json`). The "met when" reads "within 1e-6 relative of an adaptive
  reference"; taking that reference to be OpenCascade's `BRepGProp` integral (eps 1e-9, the one
  its "Missing" paragraph uses), a correct kernel cannot meet it. Which reference is meant is
  question 2; on any reading the 13 undetermined faces are open.
- **U12** missing families: *partly met*: `step_levels` and `risers` are in `Features` and so
  `correspondence::recognise` (`204b29f`, merged `36483c1`); `section_recesses` is in the
  evidence view only, not in `correspondence::recognise`, which the "met when" names: the
  wiring backlog item and question 5 (see (b)).
- **U13** simple parameter-space loops: *deferred*. cgb202 face 399 (a neck closed within
  15 µm whose samples cross on the surface) stays refused even after `352a860`; meeting it
  needs a tolerance decision (question below). The other faces U13 names (cgb242 715, 726)
  triangulate at `a2e9721`. Its alternative "met when", U4 met, is met: whether that closes U13
  is for specify-core-rust to state.
- **U14** replace and remove of a written thread: *met* (`0b56d47`); `pmi_write.rs`
  `a_written_thread_is_replaced_and_removed`,
  `removal_takes_a_feature_definitions_shape_only_with_its_feature`; `thread_replaced` and
  `thread_removed` load and read in OpenCascade (`d4e9a45`, load and read only: XCAF has no
  thread semantics).
- **U15** OpenCascade crash on datum symbols: *met* for specify-core's loader by decision 1
  (`c228f95`); `opencascade_reads_the_written_files`. The crash itself is specify-core's
  `load.py` (`docs/step-ap242.md` question 1).

## (d) CI

Latest run at `c2ce06a`: CI run 38007924732, green on both platforms (Linux and macOS lint,
build and 15 test shards each). Wall time from the first job's start to the platform's last
job's end: **Linux 10.8 min** (build 3.6, slowest shard 7.2), **macOS 24.2 min** (build 3.3,
shards 2.0–6.7 min each, but started in waves). It is the first run with q-ci-test-cost's
regenerated shard times (`1c2e4cb`); the two before took 9.9/23.9 (`bfecbc2`) and 11.2/25.0
(`15b86fc`). The macOS figure is GitHub's limit of 5 concurrent macOS jobs on this account
over about 80 macOS runner-minutes (status round 22, `91a352a`): reaching about 10 minutes
needs a maintainer decision (question below). Stream q-fills-at-most-callers is reducing test
cost meanwhile.

## Questions

1. **macOS CI time** (gate d): a higher-concurrency plan, larger paid macOS runners or another
   provider, or accept about 24 minutes for alpha 1?
2. **U11 (b)'s reference**: is the "adaptive reference" OpenCascade's `BRepGProp` integral (the
   inaccurate side on 1345 faces) or an independent one (Green's-theorem integration agrees with
   haecceity there)? And are the 13 undetermined faces known limitations for alpha 1?
3. **U13, cgb202 face 399**: refuse the face (as now), or adopt a tolerance under which a neck
   closed within 15 µm is meshed?
4. **U8**: `docs/step-ap242.md` question 6 (where the pitch goes; what 'number of threads'
   counts).
5. **Section recesses in the recognition document** (gate b): carry them instead of, or beside,
   section passages?
