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
| `tests/fixtures/known_divergences.json` | `tests/corpus.rs` | 127 (895 problems) | 64 | 18 | 13 | 32 | 0 |
| `tests/fixtures/captured/known_divergences.json` | `tests/captured.rs` | 23 (24 calls) | 4 | 0 | 1 | 2 | 16 |
| `tests/fixtures/known_invariance.json` | `tests/invariance.rs` | 7 | 0 | 7 | 0 | 0 | 0 |
| `tests/fixtures/known_correspondence.json` | `tests/correspondence.rs` | 18 | 3 | 15 | 0 | 0 | 0 |
| `tests/fixtures/known_probes.json` | `crates/haecceity/tests/probes.rs` | 28 (53 probes) | 6 | 12 | 0 | 10 | 0 |
| `tests/fixtures/known_drawings.json` | `crates/haecceity/tests/drawings.rs` | 64 | 13 | 0 | 0 | 51 | 0 |
| `tests/fixtures/known_classify.json` | `crates/haecceity/tests/classify.rs` | 45 | 1 | 5 | 2 | 4 | 33 |
| `tests/fixtures/known_face_areas.json` | `crates/haecceity/tests/face_areas.rs` | 1835 | 1789 | 33 | 0 | 12 | 1 |
| `tests/fixtures/known_face_sources.json` | `crates/haecceity/tests/face_sources.rs` | 137 | 93 | 0 | 0 | 43 | 1 |
| `tests/fixtures/known_pmi.json` "nist" | `crates/haecceity/tests/pmi_read.rs` | 74 | 65 | 0 | 9 | 0 | 0 |
| `tests/fixtures/known_pmi.json` "occt" | `crates/haecceity/tests/pmi_read.rs` | 403 | 402 | 1 | 0 | 0 | 0 |
| `tests/fixtures/known_pmi.json` "specify" | `crates/haecceity/tests/pmi_read.rs` | 38 | 20 | 0 | 0 | 0 | 18 |
| `tests/fixtures/known_pmi_draftwright.json` | `crates/haecceity/tests/pmi_draftwright.rs` | 117 | 111 | 0 | 0 | 0 | 6 |
| `tests/fixtures/known_pmi_write.json` | `crates/haecceity/tests/pmi_write.rs`, `pmi_roundtrip.rs` | — | — | — | — | — | — |

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
specify-core's files and from OpenCascade's reading of what it writes; it comes with the writer
stage, and its counts are filled in when that stage is integrated.

For specify-core and draftwright, haecceity reads and is to write AP242 semantic PMI without
OpenCascade (design: `docs/step-ap242.md`): a lossless Part 21 document with byte-exact edits
(`p21.rs`), the AP242 EXPRESS schema as data with a validator (`express.rs`), each face and edge's
source instance (`step::read_part_definitions`), and a plain semantic PMI model with one reader
(`pmi/`). The reader keeps every value as the file states it, in its own unit; reports what it
does not read (`pmi::Finding`), never drops it; and records the instances behind every item
(`pmi::Provenance`). A removal plan (`removal.rs`) works out what replacing a part's PMI removes,
with its presentation or refusing; named EXPRESS WHERE and UNIQUE rules (`express_rules.rs`)
back the writer (`pmi/write.rs`: add, replace or remove the PMI of every part in one edit).
specify-core and draftwright reach all of this through the `quiddity` command line, in a
versioned JSON form of the model (`src/pmi_json.rs`): `quiddity parts`, `quiddity pmi read`
and `quiddity pmi check` (see [Running](#running)).

The kernel also answers the questions the unported families ask of OpenCascade's booleans,
checked against every one Python asks over the corpus: the volume a probe shares with a solid
(`crates/haecceity/src/volume.rs`, `crates/haecceity/tests/probes.rs`) and whether faces cover
a face (`crates/haecceity/src/cover.rs`, `crates/haecceity/tests/patches.rs`). Faces stored as
B-splines that are exactly planes, cylinders, cones or spheres are recovered as such
(`crates/haecceity/src/recover.rs`), as Python's `_effective_surfaces` does.

For draftwright-rust, the kernel draws a part's views without OpenCascade's hidden-line
algorithm or booleans (`crates/haecceity/src/hlr.rs`): every visible and hidden edge and
silhouette, and a section view's kept half and cut outline. `crates/haecceity/tests/drawings.rs` checks them
against OpenCascade's drawings of the corpus: 85 parts in four views and 98 sections. The cut
outlines agree on 95 of 99 parts, where the other four are OpenCascade boolean failures. The
views that differ by more than 1e-3 mm are listed in `tests/fixtures/known_drawings.json`.
Most of them agree within 0.03 mm, where OpenCascade's hidden-line output approximates
projected curves.

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
                     write.rs (the AP242 writer: a typed emission layer, add / replace /
                     remove for all parts in one p21 edit, validated before it is returned)
src/
  features/          the recognisers, one module per family, plus what they share
    context.rs       Context: one run over one part; box, classifier, cylinder inventory
                     computed once, on first use
    evidence.rs      Occurrence<R> (record + defining faces) and the valid-solid check
    body.rs          body keys: a solid's box, volume and area, unique within the part
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
    edge_open.rs     what the two edge-open recess families share: `_rings.SPAN_EPS`, principal
                     planes, the mouth capping a wall chain, paired shared-edge occurrences, and
                     the floor proof by swept-face probes
    edge_open_circular.rs  edge_open_prismatic.rs
    volume_probe.rs  axis-aligned prism probes (`prism_is_empty`) over haecceity's volume.rs
  correspondence/    revision matching (docs/correspondence.md): fingerprint.rs (features and
                     faces), align.rs (rigid alignment), assign.rs (Hungarian with an
                     unmatched option), faces.rs (seeded propagation), mod.rs (`correspond`)
  pmi_json.rs        the versioned JSON form of haecceity's PartPmi, both directions: values
                     as stated decimal text with their units, anchors as face and edge numbers
                     bound to the file's sha256 and the reader version, enums as standard
                     terms; refusals name the JSON path
  bin/quiddity.rs    `quiddity part.step` → JSON with fingerprints;
                     `quiddity correspond old new` → the correspondence as JSON;
                     `quiddity parts`, `quiddity pmi read`, `quiddity pmi check` → PMI JSON
tests/
  captured.rs        replays every recogniser call the Python test suite makes
  corpus.rs          every ported recogniser over the shared 100-file STEP corpus
  evidence.rs        fillet defining faces and evidence refusals on hand-built cases
  invariance.rs      every corpus part moved and turned: each family must find the same faces
  correspondence.rs  every corpus part corresponds with itself moved, everything carried; the
                     build123d revision pairs get their expected classes
  corpus_files.rs    the corpus on disk is the one corpus.json was exported from (quiddity
                     revision, every file and its sha256)
  determinism.rs     recognition and correspondence JSON byte for byte the same twice in one
                     process (fresh hash seeds)
  wiring.rs          every family's defining faces wired, and every record field given a role
                     in the fingerprint table, over the fixtures and the corpus
  cli.rs             the binary's usage, exit codes and refusal of broken STEP files
                     (fixtures/broken/), for every subcommand
  pmi_cli.rs         `parts`, `pmi read` and `pmi check` on the AP242 fixtures (and every NIST
                     AP242 file with HAECCEITY_NIST_PMI): the JSON equals the reader's model,
                     every read part round trips through JSON with stable bytes, inch values
                     keep their text, deterministic output, refusals with the JSON path
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
  pmi_write.rs       the writer: read-back of each written case, byte preservation when adding
                     to NIST and specify-core files, one test per anti-requirement
                     (docs/step-ap242.md), corpus parts against specify-core's files, and
                     OpenCascade's reading of the result (known_pmi_write.json)
  pmi_roundtrip.rs   read → replace → read on every NIST file and specify-core input,
                     semantically equal; refusals pinned by name (known_pmi_write.json)
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
  check_pmi_occt.py  runs capture_pmi_occt.py on the writer's output (tests/fixtures/ap242/write/)
  capture_pmi_draftwright.py records draftwright's PMI extraction (tests/fixtures/ap242/draftwright/)
  make_specify_inputs.py runs specify-core on corpus parts (tests/fixtures/ap242/specify/)
  capture_revisions.py builds the revision pairs in build123d, with their expected classes
```

## Running

```
cargo build --release
./target/release/quiddity part.step > part.json            # records and fingerprints
./target/release/quiddity correspond old.json new.json      # or two STEP files
./target/release/quiddity parts part.step                   # distinct parts and the binding
./target/release/quiddity pmi read part.step [--part N]     # semantic PMI and findings as JSON
./target/release/quiddity pmi check part.step pmi.json      # a PMI document checked against the file
QUIDDITY_CORPUS_REQUIRED=1 cargo test --workspace --release
# corpus tests read ../quiddity/tests/corpus or $QUIDDITY_CORPUS; without
# QUIDDITY_CORPUS_REQUIRED=1 they pass by skipping when the corpus is missing
HAECCEITY_NIST_PMI=<NIST-PMI-STEP-Files> HAECCEITY_NIST_PMI_REQUIRED=1 \
  cargo test --release -p haecceity --test p21 --test express --test face_sources --test pmi_read
# the AP242 tests read all 17 NIST AP242 test files from that directory (NIST's
# NIST-PMI-STEP-Files.zip); without it they check the 7 committed ones; tests/pmi_cli.rs
# reads it too
```

`quiddity` exits 0 with the JSON on stdout; 1 with the error on stderr when a file cannot be
read or a PMI document is refused; 2 with the usage on stderr for bad arguments (`-h`/`--help`
prints it on stdout and exits 0).

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
PMI to) is refused, not read as empty. A STEP file is refused, never recognised as empty, when it does not parse (a cut-off
write), has no solid or shell, has shape geometry or topology that is missing or that step-io
dropped (a deleted face, a file cut at an entity and closed), or has a closed edge whose curve
the kernel cannot resolve. A face or open edge whose geometry does not resolve is kept and
recorded (`Part::unresolved_faces`, `unresolved_edges`): its solid is not valid, so its
features are not recognised, `hlr` refuses to draw the part, and the CLI warns on stderr. Python
(OpenCascade) reads a file with a deleted face as a loose shell, and resolves hyperbolas and
offset surfaces, which this kernel does not model.

The corpus must be quiddity's at the revision `tests/fixtures/corpus.json` records
(`quiddity_revision`), which CI checks out; `tests/corpus_files.rs` fails on any file added,
missing or changed. After re-exporting at a new revision, update the `ref:` in
`.github/workflows/ci.yml` to match.

The corpus tests spread their parts over every core (`tests/common/parallel.rs`) and report in
corpus order. Pins that depend on float round-off across a threshold (`faces_at_most` in
`known_correspondence.json`) are set so CI's Linux job passes: Linux is the reference platform
for them, and macOS may give a different count within the bound.

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
Z vertical). Each listed exception pins the most occurrences it may differ by. Arbitrary rotations are left to the frame normalisation Python does
before recognising (not yet ported); reflections are refused by the reader for now, which rebuilds
frames right-handed.

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
- A placement without a reference direction takes `gp_Ax2`'s default x axis.
- Face UV ranges include the control polygons of B-spline pcurves that span their edge,
  because OpenCascade boxes pcurves by their poles.
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
crack. A sphere bounded only by a vertex loop at a pole is the whole sphere.
