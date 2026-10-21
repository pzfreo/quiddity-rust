# quiddity-rust

Pure Rust manufacturing-feature recognition from STEP B-rep models, with fingerprints,
revision correspondence, and semantic AP242 PMI read/write. STEP is read through
`step-io`; the geometry kernel is `crates/haecceity`, re-exported as `quiddity::kernel`.
Recognition does not require Python or OpenCascade at runtime.

Status: **0.1.0-alpha.9**. See [CHANGELOG.md](CHANGELOG.md) and the
[alpha gates](docs/alpha1.md).

## Rust-first

The regression record is the **Rust snapshot** in `tests/fixtures/rust-snapshot/`.
Python is no longer consulted for expected results, fixes or regeneration. Python-written
STEP/PMI files, legacy notes, old fingerprints and recorded helper arguments remain test
inputs. Historical verdicts and their evidence are preserved in
[the parity archive](docs/history/python-parity.md).

Independent geometry, label, invariance, inventory-floor and NIST checks remain separate
requirements. A snapshot records behavior; it does not establish geometric correctness.
Do not remove output to improve metrics, lower a label/inventory floor, loosen a tolerance,
or regenerate expectations merely to make a failure pass. Investigate against the independent
references, then review an intentional behavior change and its decompressed snapshot diff.

```rust
let part = quiddity::read_step_file(std::path::Path::new("part.step"))?;
let recognised = quiddity::correspondence::recognise(&part);
let holes = &recognised.features.holes;
```

This is the supported recognition route: infer the part frame, transform geometry in memory,
and map records back to file coordinates. Some parameters remain local on oblique parts;
read them through the result's frame. Caller-space recognition, discovery, inventories and
contexts are private implementation details. Internal unit tests can exercise them without a
Cargo feature. There is no `python-parity` feature.

Recognition includes holes and patterns, bosses, steps, slots, pockets, channels, blends,
fillets, chamfers, plates, pads, sheet-metal bodies, thin walls, voids, profiled bores,
polygonal stock, section recesses and native measured screw threads. Refused or unproved
claims remain visible through the document and diagnostics; see the
[trust contract](docs/recognition-trust.md), [thread geometry](docs/modelled-threads.md),
[correspondence](docs/correspondence.md), and intrinsic tie policy (ADR 0011, documented in source).

## Regression snapshots

Commit source and input changes before generating, so provenance identifies a reproducible
Rust commit. The initial baseline is anchored in the alpha.9 release and a local baseline
source tag. `provenance.json` records the actual source commit, tags, input hashes, and every
compressed snapshot hash. The tool never creates a release, pushes a tag, or posts a report.

```sh
export QUIDDITY_CORPUS=/path/to/pinned/corpus
export CARGO_BUILD_JOBS=6 RUST_TEST_THREADS=6
python3 tools/rust_snapshot.py
python3 tools/rust_snapshot.py --diff BASE_REF
python3 tools/rust_snapshot.py --check
cargo fmt --all --check
cargo clippy --workspace --all-targets --release -- -D warnings
QUIDDITY_CORPUS_REQUIRED=1 cargo test --workspace --release
```

`--diff` decompresses the expectations for review; use `--file corpus.json.gz` to narrow it.
Review record counts, defining/constituent faces, refusals, dimensions, order, frame and PMI
changes, together with the source change and independent tests. Commit regenerated gzip files
and provenance in the same change. `--check` regenerates into a temporary directory and
requires byte-identical compressed output on HEAD. Normal tests only read expectations.

Every recorded recognition call is replayed with its original options and record arguments.
The corpus retains every family/option/evidence run and now also snapshots the public
recognition document and fingerprints. Other replays cover fillet manifests, frames,
effective surfaces, outer profiles, sections, patches, passages, geometry proofs,
section-recess records/documents/publication, face levels/risers, reconciliation,
recognition evidence, degradation, volume probes and semantic PMI. Inputs that a former capture
could not export remain documented as unavailable, rather than being invented.

Comparison bands are unchanged: record numbers use 1e-6 absolute/relative; manifest fillet
records compare exactly; frames use 1e-9 for axes and 1e-9 of part size for origins; PMI uses
1e-9; probes retain exact empty, 1e-9 full, and 1e-6 fraction comparisons. Section proposal
ordering still allows only the existing round-off ties. Strings, refusal outcomes, structure,
and array lengths must agree. Compressed files have a fixed gzip timestamp.

## Independent references by family

**F** is the separately maintained historical Rust inventory floor, with positive coverage
for every listed family (`tests/inventory_floor.rs`, `tests/fixtures/inventory_floor.json`).
Its loss allowances retain their analytic/OpenCascade evidence. **L** is the independent
MFInstSeg/MFCAD++ class mapping and MFInstSeg ratchet (`tests/mfinstseg.rs`,
`tests/fixtures/mfinstseg_mapping.json`). Label mappings are coarse and do not validate every
subtype. The corpus motion suite (`tests/invariance.rs`) additionally checks the recognition
and correspondence families it exercises; motion alone cannot detect a missing feature.
No new independent references are claimed for legacy value-only helpers.

| Family | Independent references |
|---|---|
| `angled_steps` | F; L |
| `blends` | F |
| `bosses` | F |
| `chamfers` | F; L; analytic rolled planes (`src/features/chamfers.rs`) |
| `channels` | F; L |
| `circular_blind_steps` | F; L; analytic supporting planes (`src/features/circular_blind_steps.rs`) |
| `circular_face_patterns` | F |
| `countersinks` | F |
| `double_d_bores` | F; analytic wall claims + motion (`tests/evidence.rs`) |
| `edge_open_circular_pockets` | F; analytic floor/hole claims (`tests/evidence.rs`) |
| `edge_open_prismatic_recesses` | F; analytic wall/floor claims (`tests/evidence.rs`) |
| `fillets` | F; L; analytic rolled supports (`src/features/fillets.rs`) |
| `flats` | F |
| `freeform_surfaces` | F |
| `grooves` | F |
| `gusset_rib_patterns` | F |
| `gusset_ribs` | F; analytic cap/slant claims (`tests/evidence.rs`) |
| `hole_patterns` | F |
| `holes` | F; L; analytic counterbores + motion (`tests/tie_breaks.rs`) |
| `interior_voids` | F; analytic bounded-air fixtures (`src/features/interior_voids.rs`) |
| `modelled_threads` | F; analytic thread fixtures (`tests/modelled_threads.rs`) |
| `oblique_through_steps` | F; L |
| `oriented_chamfers` | F |
| `oriented_slot_patterns` | F |
| `oriented_slots` | F; L |
| `pads` | F; analytic pad fixtures (`tests/pads.rs`) |
| `paired_ramp_steps` | F; L |
| `plates` | F |
| `pocket_patterns` | F |
| `pockets` | F; L |
| `polygonal_bosses` | F |
| `polygonal_stock` | F |
| `prismatic_pockets` | F; L; analytic prism fixtures (`tests/prismatic_pockets.rs`) |
| `rectangular_blind_slots` | F; L |
| `repeating_radial_profiles` | F |
| `risers` | F |
| `round_bottom_blind_slots` | F; L |
| `section_passages` | F; L |
| `section_recess_patterns` | F; member-face unions + motion (`tests/section_recess_publication.rs`, `tests/section_recess.rs`) |
| `section_recesses` | F; geometry publication and motion (`tests/section_recess_publication.rs`, `tests/section_recesses.rs`) |
| `sheet_metal_bodies` | F |
| `slot_patterns` | F |
| `slots` | F |
| `step_levels` | F |
| `thin_wall_bodies` | F |
| `through_steps` | F; L |
| `turned_steps` | F |
| face levels (legacy `recognise_face_levels`) | **snapshot only** |
| passages (legacy roster) | **snapshot only** |
| planar outer profiles | analytic directed supports, source-edge correspondence, motion (`tests/outer_profiles.rs`) |
| effective surfaces | material orientation, dependency consistency, motion (`tests/effective_surfaces.rs`) |
| sections / cylindrical / end-surface geometry | motion (`tests/section_geometry.rs`, `tests/section_recess_geometry.rs`, `tests/section_recess_helpers.rs`); value-only helpers: **snapshot only** |
| frames | motion equivariance, inverse point transforms, caller/working-face identity (`tests/frames.rs`) |
| correspondence | handwritten analytic revision intent (`tests/fixtures/revisions/expected.json`), motion (`tests/correspondence.rs`, `tests/invariance.rs`) |
| mass / face integrals | OpenCascade-measured corpus, area and moment ledgers; analytic surfaces; live OCCT (`tests/corpus.rs`, `crates/haecceity/tests/face_{areas,moments,integrals}.rs`, `tools/check_face_integrals.py`) |
| PMI read/write | NIST expectations, stated design intent, OCCT XCAF, analytic writer fixtures, semantic round trips (`crates/haecceity/tests/pmi_{read,write,roundtrip}.rs`, `tests/pmi_cli.rs`) |

OpenCascade and analytic ledgers now live in `tests/fixtures/references/`; their executable
checks and tolerances remain. Direct OCCT captures under `ap242/occt` and `ap242/written/occt`
are independent measurements, not Python recognition expectations. The read-only live
cross-checks (`tools/check_face_integrals.py`, `tools/check_pmi_occt.py`) remain available.

## Labelled accuracy

`examples/mfinstseg_score.rs` compiles the same private Rust engine as the library and
retains the original caller-space evaluation semantics and committed class mapping. No feature flag is required:

```sh
cargo run --release --example mfinstseg_score -- DATASET IDS.txt OUT.json 6
QUIDDITY_MFINSTSEG=/path/to/MFInstSeg cargo test --release --lib regression_tests::mfinstseg
```

MFInstSeg has `steps/` and `labels/`; for MFCAD++ pass its STEP directory and append
`--mfcadpp`. Only defining faces count. Identical face sets are deduplicated; instance
matching is one-to-one at IoU >= 0.5; empty denominators score zero. Incomplete instance
labels retain semantic metrics. Input failures remain misses. Dataset files stay external;
use `tools/update_mfinstseg_baseline.py` only for non-regressing improvements.

## Running

```sh
cargo build --release
cargo run --release -- part.step
cargo run --release -- correspond old.step new.step
cargo run --release -- parts part.step
cargo run --release -- pmi read part.step
cargo run --release -- pmi check part.step pmi.json
cargo run --release -- pmi write part.step pmi.json out.step
cargo run --release -- serve
```

The CLI emits JSON. Exit 0 means success, 1 means unreadable input or a refused operation,
and 2 means invalid usage. `serve` exchanges one JSON request/response per line:

```json
{"id":1,"op":"recognise","step":"part.step"}
```

PMI JSON binds decimal values and face/edge anchors to the exact STEP text and reader version.
Writes support add, replace and remove, preserve unrelated text, validate schema/practice,
and verify semantics by reading back before replacing the destination. See
[src/pmi_json.rs](src/pmi_json.rs) and [AP242 practice](docs/step-ap242.md).

The 100-file corpus is pinned by path and SHA-256 in `tests/fixtures/corpus.json`. CI checks
out only those input files at its recorded input-repository revision; it does not run Python
recognition or capture steps. Export `QUIDDITY_CORPUS` locally. CI requires it; developer runs
without a corpus may skip corpus work. The full release suite must include it. Additional
NIST AP242 files can be checked by exporting `HAECCEITY_NIST_PMI` and
`HAECCEITY_NIST_PMI_REQUIRED=1`; committed NIST fixtures always run.

## Layout

- `src/`: recognition, frames, feature records, fingerprints, correspondence and CLI.
- `crates/haecceity/`: STEP/Part 21, geometry, validity, integrals, drawings and semantic PMI.
- `tests/`: internal geometry/reference and snapshot replays, plus public CLI tests.
- `tests/fixtures/captured/`: preserved legacy test inputs and query arguments.
- `tests/fixtures/references/`: independent measured ledgers and motion exceptions.
- `tests/fixtures/rust-snapshot/`: Rust-owned expectations and provenance.
- `tools/rust_snapshot.py`: the single snapshot regeneration/check/review tool.
- `docs/history/python-parity.md`: archived verdicts, evidence and source commits.

Kernel limitations and historical findings remain documented in the source and reference
ledgers. Approximate boundaries, malformed solids, unsupported surfaces and ambiguous
geometry can yield explicit refusals or unproved claims. A successful parse is not a guarantee
that every family is recognised. Refer to the trust contract before consuming such claims.
