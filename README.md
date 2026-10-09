# quiddity-rust

A pure-Rust port of [quiddity](https://github.com/pzfreo/quiddity): deterministic,
geometry-only feature recognition for STEP B-Rep. No OpenCascade: STEP is read with
[`step-io`](https://crates.io/crates/step-io) and everything the Python implementation asks of
OpenCascade is reimplemented in `crates/haecceity` (re-exported as `quiddity::kernel`).

**Status: prototype.** Ported so far, each checked against Python call by call:

| Family | Python entry point | Captured test calls | Not captured | Corpus runs |
|---|---|---|---|---|
| Fillets | `recognise_fillets` | 108/109 match (1 known divergence) | 0 | 199/200 (1 known) |
| Holes | `recognise_holes` | 211/214 match (3 known divergences) | 0 | 190/200 (10 known) |
| Chamfers | `recognise_chamfers` | 107/107 | 0 | 200/200 |
| Countersinks | `recognise_countersinks` | 77/77 | 0 | 100/100 |
| Angled steps | `recognise_angled_steps` | 105/105 | 0 | 100/100 |
| Flats | `recognise_flats` | 88/88 | 0 | 99/100 (1 known) |
| Paired ramp steps | `recognise_paired_ramp_steps` | 109/109 | 0 | 100/100 |
| Bosses | `recognise_bosses` | 133/133 | 0 | 100/100 |
| Hole patterns | `recognise_hole_patterns` | 470/470 | 0 | 99/100 (1 known) |
| Oriented chamfers | `recognise_oriented_chamfers` | 15/15 | 0 | 100/100 |
| Face levels | `recognise_face_levels` | 36/36 | 0 | — |
| Risers | `recognise_risers` | 66/66 | 0 | — |
| Circular face patterns | `recognise_circular_face_patterns` | 5/5 | 0 | 99/100 (1 known) |
| Thin-wall bodies | `recognise_thin_wall_bodies` | 10/14 (4 known divergences) | 0 | 97/100 (3 known) |
| Interior voids | `recognise_interior_voids` | 6/6 | 2 | 98/100 (2 known) |
| Through steps | `recognise_through_steps` | 141/141 | 7 | 100/100 |
| Oblique through steps | `recognise_oblique_through_steps` | 10/10 | 1 | 100/100 |
| Circular blind steps | `recognise_circular_blind_steps` | 36/36 | 2 | 100/100 |
| Turned steps | `recognise_turned_steps` | 286/287 (1 known divergence) | 18 | 98/100 (2 known) |
| Round-bottom blind slots | `recognise_round_bottom_blind_slots` | 25/27 (2 known divergences) | 1 | 100/100 |
| Rectangular blind slots | `recognise_rectangular_blind_slots` | 21/23 (2 known divergences) | 1 | 100/100 |
| Gusset ribs | `recognise_gusset_ribs` | 26/26 | 0 | 100/100 |
| Gusset rib patterns | `recognise_gusset_rib_patterns` | 18/18 | 0 | 100/100 |
| Grooves | `recognise_grooves` | 71/72 (1 known divergence) | 3 | 99/100 (1 known) |
| Plates | `recognise_plates` | 175/178 (3 known divergences) | 4 | 98/100 (2 known) |
| Double-D bores | `recognise_double_d_bores` | 71/78 (7 known divergences) | 1 | 100/100 |
| Edge-open circular pockets | `recognise_edge_open_circular_pockets` | 9/9 | 0 | 100/100 |
| Edge-open prismatic recesses | `recognise_edge_open_prismatic_recesses` | 23/23 | 2 | 100/100 |
| Blends | `recognise_blends` | 52/52 | 0 | 94/100 (6 known) |
| Sheet-metal bodies | `recognise_sheet_metal_bodies` | 2/5 (3 known divergences) | 0 | 98/100 (2 known) |
| Slots | `recognise_slots` | 42/42 | 0 | 98/100 (2 known) |
| Pockets | `recognise_pockets` | 19/19 | 1 | 98/100 (2 known) |
| Channels | `recognise_channels` | 44/45 (1 known divergence) | 3 | 100/100 |
| Slot patterns | `recognise_slot_patterns` | 272/272 | 0 | 100/100 |
| Pocket patterns | `recognise_pocket_patterns` | 58/58 | 0 | 100/100 |
| Repeating radial profiles | `recognise_repeating_radial_profiles` | 59/60 (1 known divergence) | 1 | 100/100 |
| Freeform surfaces | `recognise_freeform_surfaces` | 3/3 | 0 | 98/100 (2 known) |
| Polygonal bosses | `recognise_polygonal_bosses` | 133/139 (6 known divergences) | 4 | 100/100 |
| Polygonal stock | `recognise_polygonal_stock` | 70/70 | 1 | 100/100 |
| Section passages | `recognise_section_passages` | 83/83 | 3 | 100/100 |
| Passages (legacy roster) | `recognise_passages` | 60/60 | 2 | 100/100 |
| Prismatic pockets | `recognise_prismatic_pockets` | 82/82 | 6 | 100/100 |
| Oriented slots | `recognise_oriented_slots` | 33/33 | 1 | 100/100 |
| Oriented slot patterns | `recognise_oriented_slot_patterns` | 26/26 | 0 | 100/100 |
| Rectangular pads | `recognise_rectangular_pads` | 150/155 (5 known divergences) | 5 | 100/100 |
| Planar outer profiles (evidence) | `RecognitionEvidence.planar_outer_profile` | 432/433 faces (1 known difference) | 5 | 95/100 (5 known) |

*Captured test calls*: the Python suite's calls replayed by `tests/captured.rs`. *Not captured*:
calls the suite makes that the capture could not record, outside the replay and pinned by it
(`tools/capture_plugin.py` now lists each with its test and reason; the counts here predate
that: the committed `calls.json` still holds them as counts per function, all
`capture failed: RuntimeError`, and a capture run carries them over unchanged for every function
it does not recapture). Open question for the maintainer: re-running the whole capture to give
them test ids changes the call content, not only the skips (4210 calls against 2482, 685 of the
old call identities gone, 5 shared calls with different Python answers), so it needs every
family re-verified; whether to do that or to recapture family by family over each family's own
test files is not decided. *Corpus runs*: runs whose records match Python's over the 100-file
corpus in `tests/corpus.rs` (one per file and option set; face levels and risers are not in the
corpus export). Every difference, in records, defining faces or kernel answers, is listed with
its verdict and reason in a verdict file:

| Verdict file | Checked by | Entries | rust-correct | rust-wrong | equivalent | undetermined | not-applicable |
|---|---|---|---|---|---|---|---|
| `tests/fixtures/known_divergences.json` | `tests/corpus.rs` | 140 (907 problems) | 109 | 8 | 22 | 0 | 1 |
| `tests/fixtures/captured/known_divergences.json` | `tests/captured.rs` | 39 (40 calls) | 4 | 1 | 3 | 3 | 28 |
| `tests/fixtures/known_invariance.json` | `tests/invariance.rs` | 21 | 0 | 21 | 0 | 0 | 0 |
| `tests/fixtures/captured/known_frames.json` | `tests/frames.rs` | 46 | 18 | 10 | 0 | 18 | 0 |
| `tests/fixtures/known_correspondence.json` | `tests/correspondence.rs` | 35 | 9 | 26 | 0 | 0 | 0 |
| `tests/fixtures/known_probes.json` | `crates/haecceity/tests/probes.rs` | 8 (20 probes) | 7 | 0 | 0 | 0 | 1 |
| `tests/fixtures/known_drawings.json` | `crates/haecceity/tests/drawings.rs` | 63 | 58 | 0 | 0 | 1 | 4 |
| `tests/fixtures/known_classify.json` | `crates/haecceity/tests/classify.rs` | 40 | 5 | 0 | 2 | 0 | 33 |
| `tests/fixtures/known_face_areas.json` | `crates/haecceity/tests/face_areas.rs` | 1831 | 1829 | 0 | 0 | 1 | 1 |
| `tests/fixtures/captured/known_sections.json` | `tests/sections.rs` | 1 | 0 | 0 | 0 | 0 | 1 |
| `tests/fixtures/captured/passages/known_divergences.json` | `tests/passages.rs` | 0 | 0 | 0 | 0 | 0 | 0 |
| `tests/fixtures/captured/known_section_recess_helpers.json` | `tests/section_recess_helpers.rs` | 4 | 0 | 4 | 0 | 0 | 0 |
| `tests/fixtures/captured/section_recess/known_differences.json` | `tests/section_recess.rs` | 4 | 0 | 0 | 0 | 0 | 4 |
| `tests/fixtures/captured/section_recess_geometry/known_differences.json` | `tests/section_recess_geometry.rs` | 10 | 0 | 8 | 1 | 0 | 1 |
| `tests/fixtures/captured/known_section_geometry.json` | `tests/section_geometry.rs` | 2 | 0 | 2 | 0 | 0 | 0 |
| `tests/fixtures/captured/outer_profiles/known_differences.json` | `tests/outer_profiles.rs` | 7 (2193 faces) | 6 | 1 | 0 | 0 | 0 |
| `tests/fixtures/captured/known_effective_surfaces.json` | `tests/effective_surfaces.rs` | 118 (2920 answers) | 98 | 8 | 0 | 9 | 3 |
| `tests/fixtures/captured/local_degradation/known.json` | `tests/local_degradation.rs` | 9 | 3 | 3 | 0 | 0 | 3 |
| `tests/fixtures/captured/reconcile/known.json` | `tests/reconcile.rs` | 35 (0 decisions, 9 accepted, 26 motions) | 4 | 9 | 0 | 0 | 22 |
| `tests/fixtures/known_face_sources.json` | `crates/haecceity/tests/face_sources.rs` | 134 | 90 | 0 | 0 | 43 | 1 |
| `tests/fixtures/known_pmi.json` "nist" | `crates/haecceity/tests/pmi_read.rs` | 74 | 65 | 0 | 9 | 0 | 0 |
| `tests/fixtures/known_pmi.json` "occt" | `crates/haecceity/tests/pmi_read.rs` | 403 | 402 | 1 | 0 | 0 | 0 |
| `tests/fixtures/known_pmi.json` "specify" | `crates/haecceity/tests/pmi_read.rs` | 38 | 20 | 0 | 0 | 0 | 18 |
| `tests/fixtures/known_pmi_draftwright.json` | `crates/haecceity/tests/pmi_draftwright.rs` | 117 | 111 | 0 | 0 | 0 | 6 |
| `tests/fixtures/known_pmi_write.json` "occt" | `crates/haecceity/tests/pmi_write.rs` | 6 | 6 | 0 | 0 | 0 | 0 |
| `tests/fixtures/known_pmi_write.json` "corpus" (patterns) | `crates/haecceity/tests/pmi_write.rs` | 9 | 7 | 0 | 0 | 2 | 0 |

The undetermined entries are the backlog: differences not yet shown to be either side's error.
The rust-wrong entries are known port defects. The AP242 PMI lists are against NIST's expected PMI
(`"nist"`), OpenCascade XCAF (`"occt"`, counted over the 7 committed NIST files and the
specify-core outputs, plus the other 10 NIST files when `HAECCEITY_NIST_PMI` is set),
specify-core's intent (`"specify"`) and draftwright's extraction; the PMI reader's own findings
per file are pinned with a reason per kind in `tests/fixtures/ap242/read/findings.json`, and the
AP242 schema's per-file violations in `tests/fixtures/ap242/express/known_nist_violations.json`
and its named WHERE/UNIQUE rule violations in
`tests/fixtures/ap242/rules/known_nist_rule_violations.json` (pins with a reason each, verdict
`file-wrong`). `known_pmi_write.json` holds the writer's refusals and its differences from
specify-core's files and from OpenCascade's reading of what it writes.

For specify-core and draftwright, haecceity reads and writes AP242 semantic PMI without
OpenCascade (design: `docs/step-ap242.md`): a lossless Part 21 document with byte-exact edits
(`p21.rs`), the AP242 EXPRESS schema as data with a validator (`express.rs`), each face and edge's
source instance (`step::read_part_definitions`), and a plain semantic PMI model with one reader
(`pmi/`). The reader keeps every value as the file states it, in its own unit; reports what it
does not read (`pmi::Finding`), never drops it; and records the instances behind every item
(`pmi::Provenance`). A removal plan (`removal.rs`) works out what replacing a part's PMI removes,
with its presentation or refusing; named EXPRESS WHERE and UNIQUE rules (`express_rules.rs`)
back the writer. The writer (`pmi::write`) maps every part's PMI to one edit of the file
(add, replace, remove) through a typed emission layer, keeping every other byte, with a
datum feature symbol derived for each datum it adds; refusals, round trips and OpenCascade's
reading of written files are pinned in `tests/fixtures/known_pmi_write.json`. `pmi::verify`
checks a write by reading it back (what `quiddity pmi write` does), naming each difference. A file
read once (`step::StepFile`) gives its parts with XCAF's display names, its assemblies, and each
part as a `Part` in its own coordinates (`step::read_part`).
specify-core and draftwright reach all of this through the `quiddity` command line, in a
versioned JSON form of the model (`src/pmi_json.rs`): `quiddity parts`, `quiddity pmi read`,
`quiddity pmi check` and `quiddity pmi write` (see [Running](#running)).

Python's default inventory runs strictly and, when that refuses an unproved hole on a part whose
solids are not all valid under BRepCheck, runs again with `local_degradation` set, where a
family skips a record that proves no one valid solid instead of refusing it.
`tools/capture_local_degradation.py` records it over the whole corpus
(`captured/local_degradation/capture.json`): 3 of 100 parts take the retry (cgb202, 13975,
14052) and every retried inventory completes, skipping 11 records: 2 fillets (cgb202), 2 pockets,
1 hole, 1 riser and 2 face levels (13975), 1 hole and 1 plate (14052). Countersinks, prismatic
pockets, double-D bores, through steps, angled steps, circular blind steps and seven other
families check the flag there and skip nothing; passages, oriented slots, repeating profiles,
polygonal bosses, sheet metal, thin walls, pads and oblique through steps, among others, never
reach their check. Ported: the holes' degraded evidence path
(`holes::discover_locally_degraded`), which skips 13975's hole as Python does and publishes
14052's on a solid it calls valid (rust-wrong), and the pockets' one
(`pockets::discover_locally_degraded`), which skips 13975's two pockets as Python does. Not
ported: the retry itself (the port's `recognise` checks no family's evidence, so there is no
refusal to retry on; whether it should panic or carry a refusal is an open maintainer
question), the fillet and plate skips (in `fillets.rs` and `plates.rs`), and Python's admission
of an invalid solid with at most three bad faces, which needs a per-face geometric validity
check the kernel does not have. `tests/local_degradation.rs` compares which parts retry and
every skip against the port's evidence paths, with each difference's verdict in
`captured/local_degradation/known.json`, and checks the degraded holes and pockets under two
rigid motions.

The aggregate's cross-family reconciliation (`_reconcile_existing`: recess precedence, bevels,
circular-step fillets, blends, Double-D bores, bosses and turned steps, steps and grooves,
oriented-slot passages, thin walls) is ported (`src/features/reconcile.rs`) and applied:
`features::inventory` makes the decisions on one run's candidates, and `features::recognise` (so
the caller-space and default recognitions, the recognition document and correspondence) drops
every rejected candidate with its defining faces and derives hole, slot, pocket, gusset-rib and
oriented-slot patterns from accepted members only, as Python's `_take_inventory_once` does.
Reconciliation adds no records (step levels and risers are physical families in Python, not
derived from it), so the document keeps its shape (`recognition/2`); a reconciliation refusal
panics, as the passages' does. `tools/capture_reconcile.py` records
every disposition Python's default inventory makes over the corpus, local-degradation retry
included (`captured/reconcile/capture.json.gz`): 261 decisions on 62 of 100 parts (127 blends
superseded by fillets, 27 bosses by turned steps, 27 rings by pockets, 24 fillets by circular
blind steps, 13 chamfers by angled steps, 10 risers, 9 bosses and 1 plate by thin walls, 10
pockets by passages, 6 step/groove relations, 7 other recess decisions). `tests/reconcile.rs`
compares the port's decisions part by part, keyed by family and defining faces, with outcome,
reason and winners: all 261 agree. Thirteen synthetic scenarios run through Python's own
`_reconcile_existing` (`captured/reconcile/scenarios.json`) cover the branches the corpus never
reaches, including a candidate two rules decide, which Python refuses; all agree. The accepted
records are compared too, family by family among those a rule reads, with Python's candidates
less its rejected ones: all agree but 9 candidate differences no rule causes (listed under
`accepted`): 3 holes whose spotface or drilling end the port reads differently and 2 cgb202
fillets on a solid only OpenCascade calls invalid (rust-correct); 13975's hole and pockets and
14052's hole and plate, which Python's local-degradation retry skips and `recognise`, which takes
no retry, publishes, and GRM-03's plate on a solid that owns turned steps, which Python's
aggregate never offers to plates (`excluded_solids`, not ported) (rust-wrong). Over the corpus
caller-space recognition loses Python's 245 rejected records of carried families on 62 parts
(127 blends, 36 bosses, 27 prismatic pockets, 24 fillets, 13 chamfers, 12 pockets, 3 slots, 2
section passages, 1 plate) and the default document 233 on 63 (114 blends, 35 bosses, 14
pockets; the rest as caller space), because its frame finds other candidates on five parts
(`known_framed_document.json`, undetermined): on cgb202, cgb207 and cgb217, whose frames are
not the file's axes, other fillets, so it keeps 16 blends caller space rejects and rejects 3
caller space keeps; on cgb217 no turned step for one boss; on 10060 and 10103 an edge-open
circular pocket that rejects one pocket each. No pattern changes. `tests/recognition.rs` checks
that no rejected candidate is in `recognise` or the document on parts where every rejecting rule
the corpus reaches fires, and that a pattern loses a member reconciliation rejects. Risers, which
`recognise` does not run, are found for the thin-wall rule with the aggregate's options.

The kernel also answers the questions the unported families ask of OpenCascade's booleans,
checked against every one Python asks over the corpus: the volume a probe shares with a solid
(`crates/haecceity/src/volume.rs`, `crates/haecceity/tests/probes.rs`) and whether faces cover
a face (`crates/haecceity/src/cover.rs`, `crates/haecceity/tests/patches.rs`). Faces stored as
B-splines that are exactly planes, cylinders, cones or spheres are recovered as such
(`crates/haecceity/src/recover.rs`), as Python's `_effective_surfaces` does.

The shared machinery under passages, section passages, oriented slots and section recesses is
ported ahead of those families and replayed call by call (`tests/sections.rs`,
`tools/capture_sections.py`): canonical sections, frames and the published occurrence shape
(`_sections`, 2268 calls from its Python tests, refusals by message), polygon `covered_patch`
questions (445), and `section_ring_proposals` with every `prove_entry_treatments` question it
asks (369, 31 proved) on 132 parts the Python tests build, the two golden passage fixtures and
the 100 corpus files (168 proposals). All agree; proposals whose sort keys tie to round-off are
compared as a set, since Python's order between them changes from run to run. Not captured: 56
`covered_patch` questions on curved or holed faces, 4 value calls whose inputs are not values (a
body reference foreign to the run or mutated, which Python checks by object identity and the
port's types rule out, or a malformed boundary), and 5 objects handed to
`section_ring_proposals` that STEP export refuses (such as the tests' shallow part views).

Passages publish from those proposals (`src/features/passages.rs`): `recognise_section_passages`
gives Python's `SectionPassage` records, and `recognise_passages` the frozen legacy roster over
`_rings` rings, whose value each matching section passage must reproduce. Python's internal
`ValueError`s are `PassageError` refusals with its messages (one test part refuses, as in
Python); `recognise` panics with the message, as Python's aggregate raises. `tests/passages.rs`
replays the entry calls and, on 359 parts (the calls' parts, the section tests' parts, every
golden fixture, the corpus), every ring, the legacy roster's walls, the records and defining
walls, and 274 compatibility calls (`tools/capture_passages.py`): all agree. `Features` carries
them as `section_passages`, the field of Python's legacy inventory (`_LegacyRecognitionResult`;
the public `RecognitionResult` publishes passages through `section_recess`), less those
reconciliation rejects for a slot or an oriented slot.
Open question for the maintainer: Python 0.4 publishes passages through the unified
`section_recess` projection rather than as a family of their own; whether the recognition
document should keep `section_passages` as a family (as now) or wait for that projection is not
decided.
Three of the helpers section recesses compose are ported the same way
(`tests/section_recess_helpers.rs`, `tools/capture_section_recess_helpers.py`):
`_cylindrical_end_surface` (257 values from the Python tests, refusals by message),
`cylindrical_seat_proofs` and `plane_envelope_passage_proofs` on 204 parts the Python tests
build (60 of them parts where Python proves nothing, so the port must prove nothing too), the
two golden passage fixtures and the corpus, with every private `_prove` question replayed on its
own (2121 seat questions, 120 proved; 14459 envelope questions, 44 proved, none on the corpus).
All agree; an envelope proof's two roof terms whose heights tie to round-off are compared in
gradient order, and a seat's arc in one direction (Python's follows OCCT's per-process shape hash). Not
captured: 1 value with a boolean radius (refused by type), 16 test parts STEP export refuses,
and 428 more test parts on which Python proves nothing outside the helpers' own tests (all 474
would add about 4 MB of STEP; 4 per test are kept). Under the invariance motions 4 seats
at scale 0.1 are refused turned 90° about x, where the kernel's volume probe cannot answer a
grazing end probe (rust-wrong).
The section-recess records themselves are ported ahead of the family
(`src/features/section_recess.rs`, `tests/section_recess.rs`,
`tools/capture_section_recess.py`): `_section_recess`'s geometry, closed and open profiles,
planar, plane-envelope and cylindrical ends, classification, evidence, refusals, patterns and
the schema-4 document, with every `__post_init__` check refusing by Python's message. 5378
distinct constructions (94 refused) from the section-recess Python tests and
`build_section_recess_document` on the corpus are rebuilt from their inputs: all agree. Not
captured: 18 test constructions whose inputs the port's types cannot carry (booleans, lists,
strings, negative indices, wrongly sized tuples, which Python refuses by type). Patterns are
also built under the invariance motions and geometries on a principal run under the
translation (1060 moved records); 4 channel values whose cylinder centroid height is a
three-decimal rounding tie disagree with their shifted interval once moved, as in Python
(not-applicable).
Their provers and projections (`_section_recess_geometry`) and the discovery that numbers them
(`_section_recess_discovery`) are ported too (`src/features/section_recess_geometry.rs`,
`section_recess_discovery.rs`, `tests/section_recess_geometry.rs`,
`tools/capture_section_recess_geometry.py`): on 138 parts the Python tests hand `_candidates`
or `has_physical_planar_floor`, the two golden fixtures and the corpus, `_candidates` (240
calls), the obround, polygonal and mixed floor readers asked of every planar face on its own
(17157 calls), the seat, cylindrical-pocket, cylindrical-passage and plane-envelope projections
of Python's own proofs (13, 22, 33 and 11 calls), 22 `has_physical_planar_floor` questions, and
`cylindrical_channel_geometry` of the 35 channel proofs `tests/section_geometry.rs` replays.
All agree but 9: three tilted mixed pockets the kernel's volume probe cannot measure and the
malformed 14052's triangular pocket Python refuses on BRepCheck validity (rust-wrong), and one
passage origin on a three-decimal rounding tie (equivalent). The candidates are unchanged under
the invariance motions (re-expressed in the moved run's canonical frame) but one translated
pocket whose publication bound Python's projection also exceeds there (not-applicable). Not
captured: 16 test shapes STEP export refuses, 86 test parts where Python finds nothing (2 per
test are kept), and cgb203's floor questions (its projection passed 600 s). Section recesses
remain pending: `recognise_section_recesses` / `build_section_recess_document` (Python's
aggregate reconciliation) and the family's wiring into recognition are not ported.
The effective-surface query the rectangular pads, cylindrical channels, pockets and passages
and section recesses read (`_effective_surfaces`: each face's native or recovered analytic fact
or typed refusal, `recovery_nominal` / `recovery_tolerance`, and surface uses with a
material-side certificate) is ported ahead of them (`src/features/effective_surfaces.rs`) and
replayed face by face (`tests/effective_surfaces.rs`, `tools/capture_effective_surfaces.py`):
12275 faces of 260 parts its consumers' Python tests build, the two golden fixtures and the
corpus, 36825 answers. 33905 agree; the 2920 that differ are listed (chiefly OpenCascade's
coarse face areas and edge lengths in the nominal, and sides Python cannot certify where its
mesh samples stray off the face or BRepCheck rejects cgb202's healed solid). Not captured: 3
certificates OpenCascade took over two minutes to mesh, 16 test shapes STEP export refuses,
and test parts past the first three per test function. Every certificate the port issues also
agrees with its face's own orientation, and the answers are unchanged under two rigid motions
(1 listed kernel face: cgb242 face 726).
The three cylindrical proofs section recesses compose on top of it are ported the same way
(`tests/section_geometry.rs`, `tools/capture_section_geometry.py`): `prove_cylindrical_channel`,
`cylindrical_pocket_proofs` and `cylindrical_passage_proofs`, on 178 parts the Python tests
build, the two golden fixtures and the corpus, with every private question replayed on its own
(148 channel questions, 35 proved, on test parts those the tests ask and elsewhere those
Python's recognition asks; 12703 pocket floors, 69 proofs; 25583 passage cells, 90 proofs).
The removed cell Python builds by a boolean (a box or extrusion less or within the cylinder) is
built exactly as the section swept from its plane to the cylinder branch, with conic edges.
All agree but one pocket (rust-wrong): a test part whose end wall's crest vertex lies 0.003 off
its own circle edges, which the kernel pins its samples to. Not captured: 18 test shapes STEP
export refuses, and 206 test parts on which Python proves nothing outside the proofs' own tests
(left out to keep the fixtures small). The proofs are unchanged under the invariance motions.
Planar outer-profile evidence (`_outer_profile`, `_outer_profile_geometry`,
`RecognitionEvidence.planar_outer_profile`), which draftwright's profile angles read, is
`features::outer_profile::planar_outer_profile(part, face)`: one face's line/arc outer wire as
ordered supports with the material on the left about the outward normal, its inner loops counted,
the faces of its one valid body and each support's source edge (an index into `Part::edges`), or
Python's refusal. `tests/outer_profiles.rs` replays every face of the parts quiddity's
outer-profile tests build, four golden fixtures and the corpus (`tools/capture_outer_profiles.py`,
11154 faces) and checks every profile's supports against its source edges, the ported test
cases, and invariance under two rigid motions and a generic rotation. 2193 faces differ, listed
in `captured/outer_profiles/known_differences.json`: cgb202's and 14052's bodies (BRepCheck, as
elsewhere), circles and lines OpenCascade's import heals, and tangent cusps. Python signs a cusp
(a support turning straight back) by the round-off of `atan2` on antiparallel tangents, which
follows the placement; the port signs it by the supports' curvatures, and its answers are
unchanged under every motion. Not captured: test parts past four per test, and two solids sharing
one shell, which STEP export writes as two. Open question for the maintainer: whether the
recognition document (`quiddity-rust/recognition/2`) should carry outer profiles; Python's
document does not, and they are not in it.

For draftwright-rust, the kernel draws a part's views without OpenCascade's hidden-line
algorithm or booleans (`crates/haecceity/src/hlr.rs`): every visible and hidden edge and
silhouette, and a section view's kept half and cut outline. `crates/haecceity/tests/drawings.rs` checks them
against OpenCascade's drawings of the corpus: 85 parts in four views and 98 sections. The cut
outlines agree on 95 of 99 parts, where the other four are OpenCascade boolean failures. Where
faces lie in the cutting plane, the cut face is the material on both sides of it (lying in the
plane decided within the classifier's 1e-6 tolerance). The
views that differ by more than 1e-3 mm are listed in `tests/fixtures/known_drawings.json`.
Most of them agree within 0.1 mm, where OpenCascade's hidden-line output approximates
projected curves: each such entry carries that evidence (`approximation`), checked on every run
against the part's own edges. `tools/drawing_evidence.py` asks OpenCascade independently of the
boolean the section view relies on (plane sections, its classifier, its ray intersector).

## Layout

```
crates/haecceity/    the geometry kernel (what OpenCascade is to the Python code), its own
  src/               crate so draftwright-rust can share it; quiddity re-exports it as
                     `quiddity::kernel`
    step.rs          STEP → Part: assemblies flattened with placements, units, voids,
                     reachable surface models, vertex loops, pcurve control polygons; each
                     face's and edge's source instance; read_part_definitions (the distinct
                     parts, their faces and edges in specify-core's numbering, placements)
    brep.rs          Part: faces, loops, edges, solids in OpenCascade's traversal order;
                     cached neighbours, validity, face bounds, extents along any direction
    geom.rs          vectors, frames, Bounds, analytic surfaces and curves, Python rounding
    nurbs.rs         rational B-spline curves/surfaces: evaluation, derivatives, inversion,
                     ray intersection
    sampling.rs      edges as polylines (adaptive, 0.2 µm chord tolerance)
    uv.rs            faces in parameter space: unwrapped loops, singular points, OpenCascade-
                     compatible UV ranges (BRepTools::UVBounds), point containment
    cloud.rs         faces as point clouds (mesh-like density on the exact geometry), k-d tree
                     nearest neighbours, exact point-to-face distance
    cover.rs         whether faces cover a face (covered_patch), in the face's parameter space
    mesh.rs          faces as triangle meshes to a chordal and angular deflection
                     (BRepMesh_IncrementalMesh): constrained Delaunay in parameter space,
                     edges discretised once for both faces so closed shells are watertight
                     (tests/mesh.rs)
    classify.rs      point-in-solid (whole part or one solid) by the parity of rays.rs
                     crossings, with on-boundary detection (BRepClass3d)
    rays.rs          every hit of a ray on a solid's trimmed faces, through a box hierarchy
                     (IntCurvesFace_ShapeIntersector); cracks the file leaves between faces
    poly.rs          real polynomial roots by isolation (exact ray-torus hits)
    recover.rs       planes, cylinders, cones and spheres stored as B-splines, fitted and
                     certified by their largest deviation (ShapeAnalysis_CanonicalRecognition)
    sweep.rs         a planar face swept into a prism solid, the probe shape the recognisers
                     build with Solid.extrude (BRepPrimAPI_MakePrism)
    volume.rs        the volume a probe (box, solid, or a planar region swept along an axis)
                     shares with a solid, without a boolean: exact ray lengths integrated
                     between the corners of the solid ∩ probe arrangement
    mass.rs          exact solid volume and area: Green's theorem along the exact edges,
                     adaptive Gauss-Kronrod quadrature (BRepGProp); every corpus face is
                     checked against OpenCascade and under motion (tests/face_areas.rs)
    hlr.rs           hidden-line projection and section views (HLRBRep, draftwright's
                     section A–A): edges and traced silhouettes cut where their projections
                     cross, each piece's visibility by one ray; section contours traced
                     across the faces
    py.rs            Python's numeric semantics: fsum, compensated sum, hypot, %, rounding,
                     tuple ordering — wherever results must agree to the bit
    p21.rs           a lossless Part 21 document: every instance's record and byte range,
                     edits (add, replace, remove, FILE_SCHEMA) written with every untouched
                     byte copied; complex records sorted; strings escaped and decoded
    express.rs       the AP242 EXPRESS schema (editions 1 and 4) as data: declarations, an
                     instance and document validator, entity families (semantic PMI,
                     presentation, validation property); express_table.rs is generated by
                     tools/express_table.py
    express_rules.rs named WHERE and UNIQUE rules the writer depends on (thread, knurl,
                     tolerance table, datum, datum target, datum system, tolerance,
                     plus_minus_tolerance, item_identified_representation_usage), each citing
                     its schema label, three-valued
    removal.rs       the removal plan: the instances a replace removes (seeds plus forward
                     dependencies nothing kept holds), presentation removed or refused by
                     policy, draughting models and views rewritten
    pmi/             semantic PMI (docs/step-ap242.md): model.rs (the model: values as stated
                     decimals in their own units, features of faces, edges and supplemental
                     geometry, datums, targets, systems, dimensions, tolerances, relations,
                     general tolerances, threads, knurls, material, notes, attributes, with
                     their invariants), read.rs (the AP242 reader: findings, provenance,
                     accounting), standards.rs (ISO 2768 classes, ISO 2768-1 Table 1),
                     write.rs (the AP242 writer: typed emission, add/replace/remove, refusals
                     by name, schema and rule backstops; differences() compares PMI by meaning)
src/
  features/          the recognisers, one module per family, plus what they share
    context.rs       Context: one run over one part; box, classifier, cylinder inventory
                     computed once, on first use
    evidence.rs      Occurrence<R> (record + defining faces) and the valid-solid check
    body.rs          body keys: a solid's box, volume and area, unique within the part
    policy.rs        the recognisers' tolerances and thresholds (`quiddity._geometry`):
                     `length_tol`, `clears_threshold`, `cluster_coordinates`, `AXIS_*_COS`
    graph.rs         face-graph readings (`FaceGraph`): planarity, normal, span, vertices,
                     common neighbours, shared-edge occurrences, faces walked as one chain
    cylinders.rs     the cylinder inventory, runs, segments, coaxial keys
    planes.rs        nearest axis-aligned neighbour planes; a face's effective plane (native,
                     or a B-spline certified as one)
    bevel.rs         the single-face bevel read and the convex-corner probe
    probes.rs        the five fixed interior probes the whole-body families sample faces at
    turned.rs        what turned-stock treatments share: coaxial external cylinders, cone rims,
                     the turned-profile key steps and grooves publish
    stacks.rs        coaxial segments read at their ends (open / flat / drill point)
    regions.rs       logical plane/cylinder face regions, their boundary wire and its runs,
                     the stock face a recess opens through, and the empty-sweep probe
    fillets.rs  chamfers.rs  holes.rs  countersinks.rs  hole_patterns.rs  bosses.rs  angled_steps.rs  flats.rs
    paired_ramp_steps.rs  oriented_chamfers.rs  levels.rs  circular_face_patterns.rs
    thin_walls.rs  interior_voids.rs  through_steps.rs
    oblique_through_steps.rs  circular_blind_steps.rs
    turned_steps.rs  round_bottom_slots.rs  rectangular_blind_slots.rs  gussets.rs
    grooves.rs  plates.rs  profiled_bores.rs
    reconcile.rs     the aggregate's cross-family reconciliation decisions (`_reconcile_existing`),
                     applied by `recognise`
    edge_open.rs     what the two edge-open recess families share: `_rings.SPAN_EPS`, principal
                     planes, the mouth capping a wall chain, and the floor proof by swept-face
                     probes
    edge_open_circular.rs  edge_open_prismatic.rs
    analytic_surfaces.rs  canonical plane/cylinder/cone/sphere parameters, their equivalence, and a
                     face's effective (native or recovered) analytic surface
    effective_surfaces.rs the run's effective-surface query (`_effective_surfaces`): typed fact
                     refusals, recovery nominal and tolerance, surface uses with a certified
                     material side, `cylinder_surface_dependency`
    blend_view.rs    native cylindrical blend chains, with the face graph readings they need:
                     paired edge occurrences, their solid ownership, a smooth join's side
    blends.rs
    sheet_metal.rs   developable thin-wall bodies as flanges and bends, unfolded into a flat pattern
                     (its own planar-face triangulation) checked for overlap
    solid_properties.rs  a solid's box, validity, volume and area (`_solid_properties`), read
                     from the per-face values `Part` caches
    recess_records.rs  the Slot, Pocket and Channel records, and `Recess`, what the reductions
                     read of a slot or pocket
    recess_faces.rs  the planar faces and principal cylinders the recess families pair, floor
                     caps, side walls (`_recess_faces`)
    recess_reduce.rs proposals with their faces: co-located merge, collinear arms rejoined,
                     body keys (`_recess_reduce`)
    recess_obround.rs  half-cylinder end caps: obround recesses recovered from them, wall-found
                     ones extended to them (`_recess_obround`)
    recess_radii.rs  the four-corner radius proof (`_recess_radii`)
    wire_seed.rs     the neighbours on a face's inner wire (`_wire_seed`)
    recess_core.rs   the wall-pair candidates, corner notches, and each family's per-solid scan
                     (`_recess_core`)
    slots.rs  pockets.rs  channels.rs
    pattern_geometry.rs  linear arrays and rectangular grids among any located records, shared by
                     the pattern families (`_pattern_geometry`)
    recess_patterns.rs  slot and pocket arrays and grids (`_recess_patterns`)
    repeating_profiles.rs  complete outer wires repeating under one sector rotation, measured
                     along each edge's exact arc length (`Edge.position_at`)
    freeform_surfaces.rs  native B-spline supports as OpenCascade's reader holds them (closed
                     pole rows made periodic), continuity links, thin-wall offset partners
    experimental_geometry.rs  the read-only geometry facade (`GeometryGraph`): a solid's or the
                     part's faces with their readings, effective analytic surfaces, blend facts
                     and the support bridges selected blend chains collapse to
    geometry_evidence.rs  occurrences issued from facade faces, published only on one valid solid
    polygonal_bosses.rs  regular square/hexagonal bosses and hexagonal whole stock
    volume_probe.rs  axis-aligned prism probes (`prism_is_empty`) over haecceity's volume.rs
    sections.rs      canonical line/arc sections, run-local frames, body references and the
                     published occurrence shape (`_sections`), with Python's refusals
    support_patches.rs `covered_patch` over haecceity's cover.rs, and the polygon faces and
                     polyhedra (ruled prisms, convex cells) the section proofs build
    entry_treatments.rs planar entry bevels that explain a wall ring's missing patches
    section_passages.rs constant-section planar-wall rings on any run (`section_ring_proposals`),
                     the proposals passages and oriented slots will publish from
    rings.rs         closed principal-axis rings of planar walls, their sections, spans and
                     caps (`_rings`), for passages and prismatic pockets
    passage_compat.rs the legacy `Passage` view of a section passage (`_passage_compat`)
    passages.rs      section passages and the frozen legacy roster they must reproduce
    cylindrical_seats.rs  open at-most-semicircular cylindrical troughs proved on original faces
    cylindrical_end_surface.rs  a cylinder branch as a section end's height over the section
    plane_envelope_passages.rs  polygonal passages through a convex two-plane roof
    section_recess.rs  the section-recess records and their validation (`_section_recess`)
    section_recess_geometry.rs  section-recess candidates from seats, intact floors and the
                     cylindrical and plane-envelope proofs, and their published geometry
    section_recess_discovery.rs  the candidates numbered as section-recess records
    cylindrical_channels.rs  three-support channels ending on a native bore, and the exact
                     cell (a polygon swept between planes or cylinder branches) the three
                     cylindrical proofs build and probe
    cylindrical_pockets.rs  polygonal pockets whose open end is a native cylinder
    cylindrical_passages.rs  polygonal passages ending on a native cross-bore
    prismatic_pockets.rs  rings capped at one end, and rings a mouth treatment interrupted,
                     recovered from their mouth or their floor and proved by swept-section probes
    pads.rs          rectangular raised pads on their four walls, sharp or corner-blended, each
                     top's material side certified (`_effective_surfaces`)
  frames.rs          part-relative recognition (`quiddity.frames`): the frame inferred from the
                     part's plane normals and cylinder axes, the part re-read into it, recognised
  framed_records.rs  records found in the frame carried back to the file's coordinates, fields
                     with no file axis left in the frame and labelled
  correspondence/    revision matching (docs/correspondence.md): fingerprint.rs (features and
                     faces), align.rs (rigid alignment), assign.rs (Hungarian with an
                     unmatched option), faces.rs (seeded propagation), mod.rs (`correspond`)
  pmi_json.rs        the versioned JSON form of haecceity's PartPmi, both directions: values
                     as stated decimal text with their units, anchors as face and edge numbers
                     bound to the file's sha256 and the reader version, enums as standard
                     terms; refusals name the JSON path; a write's report
  recognition.rs     the versioned recognition document for draftwright (`quiddity-rust/
                     recognition/2`): the part's frame, and per feature its family, Python
                     record type, faces, record and the fields left in the frame
  serve.rs           `quiddity serve`: recognise and correspond as a JSON-lines service, each
                     request's id echoed, failures as structured errors
  bin/quiddity.rs    `quiddity part.step` → JSON with fingerprints and the document;
                     `quiddity correspond old new` → the correspondence as JSON;
                     `quiddity serve` → serve.rs on stdin/stdout;
                     `quiddity parts`, `quiddity pmi read`, `quiddity pmi check` → PMI JSON;
                     `quiddity pmi write` → AP242 PMI added, replaced or removed, verified by
                     reading the output back, and the write's report
tests/
  captured.rs        replays every recogniser call the Python test suite makes
  corpus.rs          every ported recogniser over the shared 100-file STEP corpus
  evidence.rs        fillet defining faces and evidence refusals on hand-built cases
  invariance.rs      every corpus part moved and turned: each family must find the same faces;
                     and under a generic rotation, recognised in its own frame; repeating
                     radial profiles (none in the corpus) on their captured parts
  frames.rs          the frames Python derives (fixtures, corpus, Python's frame-test parts),
                     as read and under a generic rotation, the frame moving with the part, and
                     the re-read part's faces being the caller's
  correspondence.rs  every corpus part corresponds with itself moved, everything carried; the
                     build123d revision pairs get their expected classes
  effective_surfaces.rs every face's effective fact, recovery lengths and material side against
                     Python's (tests, fixtures, corpus), and unchanged under rigid motion
  sections.rs        the section helpers' captured calls (values, covered patches, ring
                     proposals and entry treatments on test parts, fixtures and the corpus)
  passages.rs        the passage entry points' captured calls (captured/passages/calls.json),
                     and rings, legacy roster, records, defining walls and compatibility calls
                     on test parts, fixtures and the corpus
  section_recess_helpers.rs  seat and envelope-passage proofs and cylindrical end values against
                     Python's captured calls, and the proofs under the invariance motions
  section_recess.rs  every captured section-recess record construction rebuilt (dictionary or
                     refusal), and the geometric records moved
  section_recess_geometry.rs  section-recess candidates, floor readings, projections and floor
                     questions against Python's captured calls, and under rigid motion
  prismatic_pockets.rs  prismatic pocket walls and consulted floors on Python's evidence-test
                     parts
  local_degradation.rs  which corpus parts take Python's local-degradation retry and what it
                     skips there, against the port's evidence paths, and under rigid motion
  reconcile.rs       every reconciliation decision Python's inventory makes over the corpus and
                     13 synthetic scenarios, against the port's decisions, and under rigid motion;
                     the accepted records against Python's
  pads.rs            each pad's top and four walls on Python's evidence-test parts and the golden
                     fixture; tolerances the capture cannot record (NaN, infinity)
  corpus_files.rs    the corpus on disk is the one corpus.json was exported from (quiddity
                     revision, every file and its sha256)
  determinism.rs     every corpus part's recognition (what the CLI prints, document included) and
                     correspondence JSON byte for byte the same twice in one process (fresh
                     hash seeds)
  wiring.rs          every family's defining faces wired, and every record field given a role
                     in the fingerprint table, over the fixtures and the corpus
  recognition.rs     the document: a record type for every family, order, faces, round trip,
                     labels; over the corpus, moved and turned (carried back) and against
                     caller-space recognition (known_framed_document.json)
  cli.rs             the binary's usage, exit codes and refusal of broken STEP files
                     (fixtures/broken/), for every subcommand
  serve.rs           `quiddity serve`: results equal to the CLI's, every error code, many
                     requests on one stream, the same bytes twice
  pmi_cli.rs         `parts`, `pmi read`, `pmi check` and `pmi write` on the AP242 fixtures (and
                     every NIST AP242 file with HAECCEITY_NIST_PMI): the JSON equals the reader's
                     model, every read part round trips through JSON with stable bytes, inch
                     values keep their text, deterministic output, refusals with the JSON path;
                     read → write replace → read through the CLI against the writer's pins
                     (known_pmi_write.json "roundtrip"), add then remove on a corpus part,
                     refusals that create nothing
  common/parallel.rs the corpus loops' per-file work on every core, results in corpus order
  fixtures/          STEP parts and Python's recorded answers, shared by both crates
crates/haecceity/tests/
  patches.rs         every covered_patch question Python asks over the corpus, answered by
                     cover.rs (tools/capture_patches.py records them)
  probes.rs          every volume probe Python's recognisers ask over the corpus, answered by
                     volume.rs (tools/capture_probes.py records them)
  face_areas.rs      every corpus face's area against OpenCascade's, and against itself moved
                     (tools/capture_face_areas.py records them)
  classify.rs        point classification against OpenCascade's at 300 seeded points per corpus
                     file, every clean ray agreeing on parity (tools/capture_classify.py)
  kernel.rs          kernel behaviour on real parts
  drawings.rs        projections and section views against OpenCascade's drawings
                     (tools/capture_hlr.py, tools/capture_section.py record them)
  p21.rs             Part 21 documents byte for byte, edits confined to their spans, over the
                     fixtures, the corpus and the NIST files
  express.rs         the schema tables against their sources, hand-written valid and invalid
                     instances, every NIST file's violations (pinned)
  face_sources.rs    face and edge source instances against OpenCascade's, per part
                     (known_face_sources.json)
  pmi_read.rs        the PMI reader against NIST's expected PMI, OpenCascade's reading and
                     specify-core's intent (known_pmi.json); every file read and accounted for
                     (ap242/read/findings.json); inch values keep their text; assemblies;
                     determinism; the model's invariants; threads, knurls, tables, material,
                     standards and bad references on hand-built additions
  pmi_draftwright.rs everything draftwright reads is in the model (known_pmi_draftwright.json)
  removal.rs         the removal plan with both presentation policies on every NIST file
                     (ap242/removal/: counts pinned, kept instances byte-identical) and a
                     hand-made fixture
  express_rules.rs   the named rules on hand-written valid and invalid files (ap242/rules/) and
                     every NIST file's violations (known_nist_rule_violations.json)
  pmi_write.rs       the PMI writer: one test per anti-requirement, add keeping every original
                     byte, remove, refusals, determinism, specify-core's intents onto the
                     original corpus files and OpenCascade's reading of written files
                     (known_pmi_write.json, ap242/write/)
  pmi_roundtrip.rs   read → replace → read over every NIST file and specify-core input, values
                     as stated (known_pmi_write.json "roundtrip")
crates/haecceity/examples/
  hlr_compare.rs     the drawings comparison, every score printed, with listings of the
                     curves either side draws differently
tools/
  capture_probes.py  records every volume probe Python asks over the corpus
  capture_patches.py records every covered_patch question Python asks over the corpus
  capture_face_areas.py records OpenCascade's area of every corpus face
  capture_classify.py records OpenCascade's classifier state at seeded points of every corpus part
  face_area_evidence.py independent areas over the 3D edges, the evidence for face-area verdicts
  known_face_areas.py writes known_face_areas.json from face_areas.rs's differences
  capture_hlr.py     records OpenCascade's hidden-line projection of every corpus part
  capture_section.py records draftwright's section view of every corpus part
  drawing_evidence.py OpenCascade's plane sections, classifier and rays, the evidence for
                     drawing verdicts its booleans or hidden-line output cannot give
  capture_plugin.py  pytest plugin that records the Python suite's recogniser calls
  export_corpus.py   records Python's answers over the corpus, with the quiddity revision and
                     each corpus file's sha256 (_provenance.py; the capture tools record the
                     revision too)
  export_fixtures.py hand-built fillet evidence cases
  express_table.py   generates crates/haecceity/src/express_table.rs from the AP242 long forms
  capture_face_sources.py records OpenCascade's face and edge instances per part
  nist_expected.py   NIST's expected PMI (tests/fixtures/ap242/nist/*.expected.json) from the
                     STEP File Analyzer's spreadsheets
  capture_pmi_occt.py records OpenCascade XCAF's PMI reading (tests/fixtures/ap242/occt/)
  capture_pmi_draftwright.py records draftwright's PMI extraction (tests/fixtures/ap242/draftwright/)
  make_specify_inputs.py runs specify-core on corpus parts (tests/fixtures/ap242/specify/)
  check_pmi_occt.py  records OpenCascade XCAF's reading of the writer's files
                     (tests/fixtures/ap242/write/)
  capture_revisions.py builds the revision pairs in build123d, with their expected classes
  capture_sections.py records the section helpers' calls in their Python tests, and their ring
                     proposals over those tests' parts, the golden fixtures and the corpus
                     (captured/sections/)
  capture_passages.py records rings, the legacy roster, section passages and their compatibility
                     calls on the passage tests' parts, fixtures and the corpus
                     (captured/passages/helpers.json.gz)
  capture_section_recess_helpers.py records the seat, envelope-passage and cylindrical-end
                     calls (captured/section_recess_helpers/)
  capture_section_recess.py records every section-recess record construction in the
                     section-recess tests and the corpus documents (captured/section_recess/)
  capture_section_recess_geometry.py records the section-recess candidates, floor readers,
                     projections and floor questions (captured/section_recess_geometry/)
  capture_effective_surfaces.py records `_effective_surfaces`'s answers for every face of its
                     consumers' test parts, the golden fixtures and the corpus
                     (captured/effective_surfaces/)
  capture_local_degradation.py records which corpus parts take Python's local-degradation retry
                     and every record it skips there (captured/local_degradation/)
  capture_reconcile.py records every disposition Python's default inventory makes over the
                     corpus, and synthetic scenarios (captured/reconcile/)
  capture_frames.py  records Python's part frames (captured/frames.json, the built parts as
                     STEP in captured/frames/); --compare runs a Python recogniser on a corpus
                     part's framed working part, as read and turned
```

## Running

```
cargo build --release
./target/release/quiddity part.step > part.json            # records, fingerprints, document
./target/release/quiddity correspond old.json new.json      # or two STEP files
./target/release/quiddity serve < requests.jsonl             # JSON lines, see below
./target/release/quiddity parts part.step                   # distinct parts and the binding
./target/release/quiddity pmi read part.step [--part N]     # semantic PMI and findings as JSON
./target/release/quiddity pmi check part.step pmi.json      # a PMI document checked against the file
./target/release/quiddity pmi write part.step pmi.json -o out.step [--mode add|replace|remove] \
  [--presentation refuse|remove]                            # AP242 PMI written, verified, reported
QUIDDITY_CORPUS_REQUIRED=1 cargo test --workspace --release
# corpus tests read ../quiddity/tests/corpus or $QUIDDITY_CORPUS; without
# QUIDDITY_CORPUS_REQUIRED=1 they pass by skipping when the corpus is missing
HAECCEITY_NIST_PMI=<NIST-PMI-STEP-Files> HAECCEITY_NIST_PMI_REQUIRED=1 \
  cargo test --release -p haecceity --test p21 --test express --test face_sources --test pmi_read \
  --test pmi_write --test pmi_roundtrip
# the AP242 tests read all 17 NIST AP242 test files from that directory (NIST's
# NIST-PMI-STEP-Files.zip); without it they check the 7 committed ones; tests/pmi_cli.rs
# reads it too
```

`quiddity` exits 0 with the JSON on stdout; 1 with the error on stderr when a file cannot be
read, a PMI document is refused, or a write is refused or does not verify; 2 with the usage on
stderr for bad arguments (`-h`/`--help` prints it on stdout and exits 0).
A STEP file is refused, never recognised as empty, when it does not parse (a cut-off
write), has no solid or shell, has shape geometry or topology that is missing or that step-io
dropped (a deleted face, a file cut at an entity and closed), or has a closed edge whose curve
the kernel cannot resolve. A face or open edge whose geometry does not resolve is kept and
recorded (`Part::unresolved_faces`, `unresolved_edges`): its solid is not valid, so its
features are not recognised, `hlr` refuses to draw the part, and the CLI warns on stderr. Python
(OpenCascade) reads a file with a deleted face as a loose shell, and resolves hyperbolas and
offset surfaces, which this kernel does not model.

`pmi write` writes every part the document lists in one edit: `add` (the default) beside the
part's PMI, `replace` in place of it (afterwards `pmi read` gives the document's PMI), `remove`
(each part's `pmi` must be `{}`). Every byte the edit does not concern is kept; an AP214/AP203
file that gains PMI becomes AP242 only if all its instances are valid AP242 (else refused,
naming them). Replace and remove take the replaced PMI's presentation with it
(`--presentation remove`, the default, each instance listed in the report) or refuse naming it
(`--presentation refuse`). The output goes to a temporary file beside `out.step` (gzipped for a
`.gz` name), is read back and compared semantically with what was written (add: the part's PMI
before plus exactly the new items; replace, remove: exactly the document's, and nothing the
reader consumed for the part survives; other parts and findings as before), and is renamed to
`out.step` only then (an existing `out.step`, the input itself included, is then replaced in
place, through a symbolic link to its target, keeping its permissions). In add, a standard the
part already states is not written again; a feature equal to one the part has is written as a
second shape aspect, which reads back as that feature; a second material is refused by the
read-back (the reader finds two material names). The report (`"format": "quiddity-pmi-write"`) gives the parts written,
the input's and the output's bindings (so the output can be read and written again), instance
counts (added, replaced, removed), the presentation removed by id and type, the `FILE_SCHEMA`
kept or changed, the datum feature symbols written, the edition table used, the original's
schema and rule violations (reported, not repaired) and the findings of reading the output.
Exit 1, creating nothing, for: a document bound to another file or reader, items the writer
does not write (datum targets, tolerance relations, notes, general tolerance tables, material
density, …, each named by part and item), a value that is not a Part 21 REAL (`62.`, not `62`),
presentation blockers under `--presentation refuse`, an edition upgrade refused, or a read-back
that differs.

The PMI JSON (`src/pmi_json.rs` describes the form) is a document `{"format": "quiddity-pmi",
"version": 1, "binding": {"sha256", "reader"}, "parts": [{"part", "name", "pmi"}],
"findings": [...]}`. Every value is `{"value": "<decimal text as stated>", "unit": "mm"|"in"|
"deg"|...}`, never a JSON number; anchors are `{"face": n}` / `{"edge": n}` in the numbering
`quiddity parts` reports, from 0; references between items are indices into the part's lists;
tolerance kinds are ISO 1101's names, fits `{"deviation": "H", "grade": "IT7"}`, schema
enumerations their lower-case EXPRESS values; a size or location kind, zone form or qualifier the
practice does not list is `{"other": "<name>"}`, so a misspelt standard term is refused rather
than kept as a name. `binding.sha256` is the sha256 of the STEP text
(after gunzip for a `.gz` file) and `binding.reader` the reader version: `pmi check` refuses a
document bound to another file or reader, an unknown field or term, a key named twice, a face or edge the part
does not have, or a violated model invariant, naming the JSON path. `pmi read --part N` keeps
the findings of part N and those of no part. A tessellated-only file (no B-rep part to anchor
PMI to) is refused, not read as empty.

`quiddity serve` reads one JSON request per line on stdin and writes one response line per
request on stdout, in order, until stdin ends (`src/serve.rs` has the protocol):

```
{"id": 1, "op": "recognise", "step": "part.step"}
{"id": 2, "op": "correspond", "old": "old.step", "new": {...a recognise result...}}
```

A success is `{"id": …, "result": …}`, the result being exactly what `quiddity part.step` or
`quiddity correspond` writes (with `"warnings"` when geometry did not resolve). A failure is
`{"id": …, "error": {"code": …, "message": …}}`, with codes `bad_json`, `bad_request`,
`missing_field`, `unknown_op`, `unreadable`, `broken_step` (the CLI's STEP refusals),
`bad_document` and `internal` (a panic, caught; the server keeps serving). Python quiddity has
no such service; its correspondence API (opaque receipts) differs in shape, and the Rust
documents are kept.

The corpus must be quiddity's at the revision `tests/fixtures/corpus.json` records
(`quiddity_revision`), which CI checks out; `tests/corpus_files.rs` fails on any file added,
missing or changed. After re-exporting at a new revision, update the `ref:` in
`.github/workflows/ci.yml` to match.

The corpus tests spread their parts over every core (`tests/common/parallel.rs`) and report in
corpus order. Pins that depend on float round-off across a threshold (`faces_at_most` in
`known_correspondence.json`) are set so CI's Linux job passes: Linux is the reference platform
for them, and macOS may give a different count within the bound. CI runs the suite on macOS as
well, so the bounds must hold there too.

## How a family is ported

1. **Capture Python's behaviour.** From the quiddity checkout, run its tests for the family
   with the capture plugin (add the function to `QUIDDITY_CAPTURE`):

   ```
   PYTHONPATH=../quiddity-rust/tools \
   QUIDDITY_CAPTURE=recognise_fillets,recognise_holes,recognise_hole_patterns,recognise_countersinks,<new> \
   QUIDDITY_CAPTURE_OUT=../quiddity-rust/tests/fixtures/captured \
   .venv/bin/python -m pytest -p capture_plugin -p no:cacheprovider <test files>
   ```

   Every call whose arguments are a part plus plain options (or records) is stored with the
   part as gzipped STEP and Python's answer on the re-imported STEP. Add the family to
   `tools/export_corpus.py` and rerun it.
2. **Port the module** into `src/features/<family>.rs`: a public `recognise_<family>(part,
   opts)` returning records that serialise to the same JSON as the Python dataclass, and a
   `discover(ctx, …) -> Vec<Occurrence<Record>>` that takes the shared `Context`. Reuse
   `Context` for anything a second family will also need (put it there rather than recomputing).
3. **Wire it in**: the module in `src/features/mod.rs` (and, for a family with occurrences,
   a `Features` field filled in `recognise`), the entry point re-exported from `src/lib.rs`,
   match arms in `tests/common/mod.rs` (`recognise`, and `defining` for an evidence path), a
   `#[test]` in `tests/captured.rs`, and a row in the table above. `tests/invariance.rs` and
   `tests/correspondence.rs` take the family from `Features`.
4. **Explain every difference** that remains in the relevant `known_divergences.json`. An entry
   names its difference exactly (the corpus: file, family and problem key, optionally limited to
   listed faces when one key's faces need different verdicts; captured calls: test node, file
   and options) with how many problems it covers; the tests fail on any unexplained
   difference, on a count that changed, and on two entries for one difference, and a failing
   run prints the entries it needs (the formats are described at the top of `tests/corpus.rs`
   and `tests/captured.rs`).

Float comparison: values must agree to one part in a million (sub-micron at part scale);
Python-rounded fields agree exactly. Below that the kernels' parameter-range arithmetic
differs in its last digits.

## Beyond parity

Matching Python is a stepping stone; the goal is the right answer. Every listed difference (the
verdict files in the table above) carries a `verdict`: `rust-correct` (the port has the true
geometry, and the reason gives the evidence), `rust-wrong` (a port limitation), `equivalent`
(one geometry, two representations), `undetermined`, or `not-applicable`. The tests refuse an
entry without one.

`tests/invariance.rs` checks what Python cannot vouch for: every corpus part is re-read moved by
a translation and right-angle rotations (`read_step_placed`), and each family must find the same
features on the same faces (every `Features` family by its defining faces, and face levels and
risers, which are read along world Z, by the faces each is read from under the motions that keep
Z vertical). Each listed exception pins the most occurrences it may differ by. Reflections are
refused by the reader for now, which rebuilds frames right-handed.

Recognition by default is in the part's own frame and reported in the file's coordinates (the
maintainer's decision on review M8): `correspondence::recognise_placed`, which the CLI, `quiddity
serve` and the recognition document use, infers the frame (`src/frames.rs`, Python's
`quiddity.frames`) from the part's plane normals and cylinder axes, ranked by area and by where
the faces sit about the part's centroid, re-reads the part placed in it, recognises it there,
and carries every record back (`src/framed_records.rs`): points by the frame, directions by its
rotation, axis letters and coordinates along them exactly where the frame's axes are the file's
up to order and sign. Where they are not, those fields stay in the frame and the document lists
them per record (`local`), with the frame itself (`quiddity-rust/recognition/2`). A part with no
frame (no plane or cylinder, no material) is recognised as placed, with a warning.

Parity is still checked where Python's entry points work: `features::recognise` and every
`recognise_<family>` stay caller-space, as Python's do, and the parity tests (`corpus.rs`,
`captured.rs`, `invariance.rs`) call them. `tests/frames.rs` checks the frame against Python's on
every fixture and corpus part, as read and turned, that it moves with the part, and that the
re-read part's faces are the caller's under the same indices; `tests/invariance.rs` checks that
every family, face levels and risers included, finds the same faces on the corpus part turned by
a generic rotation (37° about (1, 2, 3)) as unmoved, each in its frame (4 listed exceptions: two
recogniser thresholds Python shares, two kernel face-box defects). Caller-space recognition does
not survive that rotation (the same test prints by how much). `tests/recognition.rs` checks the
default document itself: under every invariance motion and the generic rotation, carried back,
it finds the same features on the same faces with the same values, and on the corpus as read it
is compared with caller-space recognition, every difference listed with a verdict in
`tests/fixtures/known_framed_document.json`.

## What parity means

Python's *specified* numeric behaviour is matched exactly: rounding grids, summation order,
tie-breaking, thresholds (`crates/haecceity/src/py.rs`). OpenCascade's *accidents* are not
reproduced: where it approximates (pcurves it builds on import, a fixed Gauss order on B-spline faces), heals a
file, or decides a degenerate case by round-off, the port computes the true geometry and the
difference is recorded with its reason. Some older emulation remains where removing it would
cost many records (face ranges boxed by B-spline pcurve poles, which hole depths follow).

Circular face patterns compare sampled surfaces, as Python compares OpenCascade meshes. The port
samples the exact geometry as densely as such a mesh, so congruent copies sample congruently and
an exact repeat measures 0, as in Python; on parts whose copies are parameterised differently
`fit_error` is each side's own sampling noise. Whether a face near the axis turns onto itself
(and so is not part of the pattern) is decided exactly: Python's mesh test answers by whether
its point counts happen to divide by the pattern count.

The corpus test also checks kernel answers directly against OpenCascade on every file: every
solid's volume and area wherever the solid counts agree (99 of 100 files; 13975 reads as one
solid where OpenCascade's import gives none, recorded as a `solid count` difference), to 1e-9
(a file that differs records its worst difference as a power of ten), and, wherever the faces align with OpenCascade's (99 of 100 files; 13975's are
reordered by its healing), every face's `BRepTools::UVBounds` and the arc between every pair of
neighbours.

## Notes on matching OpenCascade

OpenCascade silently heals files on import, and Python's answers depend on it. Things the
reader reproduces:

- `FACE_BOUND` orientation flags are not trusted: OpenCascade writes `.F.` on some toroidal
  faces whose edges are already listed in the face's sense and fixes the result on reading.
  Face containment therefore uses orientation-free crossing parity; orientation only breaks
  genuine ties (which way round a sphere's pole a boundary goes, and which side of its two
  loops a seamless torus band lies — files written by OpenCascade always have the seam).
- A void shell used reversed (`ORIENTED_CLOSED_SHELL` `.F.`) reverses each of its faces with
  its loops, as OpenCascade's explorer does, not just the faces' normals.
- A placement without a reference direction takes `gp_Ax2`'s default x axis.
- Face UV ranges include the control polygons of B-spline pcurves that span their edge,
  because OpenCascade boxes pcurves by their poles; they also hold the edges themselves (a
  file's pcurve may stray from its edge), except where a pcurve holds a parameter constant.
- Closed edges (full circles) are exempt from the orientability check: their recorded
  direction is not evidence.

Known kernel limitations: OpenCascade's healing adds missing seam edges to periodic faces (and splits closed edges they cross), which the reader does not; sphere patches that pass through a pole have approximate interior
bounding boxes; closed surfaces of revolution/extrusion in NURBS form are treated as
non-periodic, and a face on such a surface whose own seam is not the surface's (its boundary
crosses the surface's seam mid-edge, cgb217 face 29) has no area, so its solid has no mass. A face swept into a probe solid (`crates/haecceity/src/sweep.rs`, Python's
`Solid.extrude`) may be bounded only by lines and by circles and arcs about the sweep: the
edge-open recess floor proof declines a floor with any other edge (rust-wrong; no captured call
or corpus part has one).

Rays meet a face where they cross it inside its trim, or within the band by which one of its
edges strays from its surface: B-spline faces exported as approximations of their neighbours
leave cracks up to a few tenths of a millimetre wide, which OpenCascade covers with the edge
tolerances it sets on import. A hit found only in that band gives way to the face across the
crack. Where such an edge joins two analytic faces and overshoots (a B-spline edge for the
intersection of two cylinders, displaced into the opening it bounds), a hit within a few bands
of it that lies past the other face's surface is not on the face, whatever its trim says. A
line that only touches a face at its edge (a tangent blend) bounds no material there. A sphere
bounded only by a vertex loop at a pole is the whole sphere.
