# Modelled thread survey and evidence

Surveyed all **100 STEP files** in `/Users/paul/repos/quiddity/tests/corpus` at
`origin/main` `08aeab7eb4bb2a47a27eb3b1d060325c2e65e954` (the files pinned by
`tests/fixtures/corpus.json`). `examples/thread_survey.rs` inventories native
supports, candidate cylinder/cone/toroid axes, and sampled helical boundaries;
`QUIDDITY_SURVEY_DETAIL=DIR` also saves trimmed surface samples and normals.
Projected edges were inspected for every B-spline-bearing CADGenBench part and
`string_post`; all other B-spline-bearing files were checked for axial windings.

## Threads found

Millimetres, physical trimmed span, all **right hand, one start**. Axis directions
below omit components below 1e-12. No nominal designation was inferred.

| Part / axis | Side | Major Ø | Minor Ø | Pitch | Start .. end | Length |
|---|---|---:|---:|---:|---|---:|
| cgb243 / Y, point (-13.131510, 0, 35.498594) | internal | 13.623798 | 12.000000 | 1.500000 | 38.242762 .. 47.368488 | 9.125726 |
| cgb243 / Z, point (-13.131510, -1.131512, 0) | internal | 61.623798 | 60.000000 | 1.500000 | -48.501406 .. -37.377133 | 11.124274 |
| cgb243 / same Z line | internal | 61.623798 | 60.000000 | 1.500000 | 40.872867 .. 49.998594 | 9.125726 |
| cgb243 / X, point (0, -1.131512, -0.001406) | external | 75.000000 | 73.376202 | 1.500000 | 55.556716 .. 66.868490 | 11.311774 |
| threaded_connector_109 / Z through origin | external | 109.999797 | 103.120000 | 6.350000 | 367.614207 .. 419.205793 | 51.591585 |

Both parts carry **B-spline flanks**, not analytic cones. cgb243 has two continuous
flank supports per thread, split helical trims at the original carrier's seams,
cylindrical crests and roots, and small planar/conical end cuts. Its aggregate
freeform inventory falls from **74 to 66**. The connector has split ruled/swept-form
B-spline flanks and crests, cylindrical roots, numerous nearly collapsed B-spline
strip remnants at the root, and planar/B-spline runouts. Its freeforms fall from
**140 to 4**; the remaining four are shoulder blends below the thread. The geometry
establishes these shapes, not a particular CAD sweep history.

No corpus flank uses a native swept or surface-of-revolution STEP support; the raw
surface-entity census found B-splines for all five threads. The recogniser samples
all supported freeform kinds, including converted extrusion/revolution supports,
and does not dispatch on a B-spline degree, filename or entity id.

## Other freeform-bearing parts / near misses

| Files (relative corpus paths) | Native B-spline faces | Finding |
|---|---:|---|
| cadgenbench_inputs/cgb202.step.gz | 576 | Rib/cylinder blends; one short almost-helical edge, no repeated screw profile |
| cadgenbench_inputs/cgb203.step | 125 | Spiral impeller vanes: radius changes, no cylindrical helical winding |
| cadgenbench_inputs/cgb207.step | 88 | Local blends; short near-helical fragments have incompatible leads |
| cadgenbench_inputs/cgb217.step.gz | 80 | Housing/bore transitions, no helical boundary |
| cadgenbench_inputs/cgb241.step | 10 | Curved shell/loft supports, no axial repetition |
| cadgenbench_inputs/cgb242.step.gz | 190 | Housing blends; isolated short near-helical fragments |
| cadgenbench_inputs/cgb245.step | 4 | Local sheet/bracket blends, no winding |
| gramel/string_post.step | 2 | Cross-drilled head transitions, no winding |
| inventory_refusal/13975.step.gz | 1 | No helical boundary |
| nist/nist_ctc_02_asme1_rc.stp | 34 | Bounded freeform transitions, no helical boundary |
| nist/nist_ctc_05_asme1_rd.stp | 9 | Same negative winding result |
| nist/nist_ftc_07_asme1_rd.stp | 20 | Same negative winding result |
| nist/nist_ftc_10_asme1_rb.stp | 1 | Same negative winding result |

The other 85 files have no native B-spline faces; the analytic-support and boundary
census found no additional modelled threads. `gramel/GRM-03_thumbwheel_drive_screw`
and `GRM-05_depth_lock_bolt` have plain carrier geometry; their AP242 specify
fixtures attach cosmetic thread/knurl semantics. Plain carriers must not become
modelled threads. No physical spring,
knurl, longitudinal spline or gear was found in this shared corpus. The existing
`golden_repeating_radial_profile.step` fixture guards the gear/spline case.

Original OCP constructions in `tests/fixtures/modelled_threads` add physical
negatives: a diamond knurl (opposed helices), round-wire spring (no shaft/profile),
and shallow helical groove (`groove_proportions` refusal). A deliberately twisting
sweep changes the pitch between turns (`inconsistent_pitch` refusal); its
freeforms remain published. A left-hand two-start positive measures Ø20 / Ø19,
pitch 1, lead 2, length 8. Regenerate with `tools/make_thread_fixtures.py` using OCP 7.9.3.1;
no third-party model or repository was copied.

## Measurement and refusal policy

Candidate lines come from coaxial analytic supports or circular boundaries, within
1e-6 mm and 1e-8 in unit direction. Unwrapped edge samples must advance monotonically
for at least a quarter turn; a component needs 1.5 turns of boundary evidence and
the resulting thread at least two pitches. Surface samples cover a 17×17 grid
**and every knot span** (quarter/mid/three-quarter stations), clipped to the face's
trim domain. This detects local changes between the uniform stations.

Straight-sided opposed flanks must agree in material side and measured slope
(within 2%). Each must make `z - lead*theta/(2*pi) - slope*radius` constant modulo
the lead. Paired, equally spaced phase classes determine starts independently on
both flank directions. Constant-radius crests/roots establish the diameters;
short bounded end cuts are runouts. Carrier length never sets thread length.

Radius fit tolerance is `max(0.001 mm, 0.001*radius)`; crest/root samples must stay
within twice that of their median. Published diameter acceptance is four times
the radius tolerance. Axial/lead residual tolerance is
`max(0.003 mm, 0.005*abs(lead))`, divided by starts for pitch. These allow measured
interpolation errors in the imported supports (the connector's crest deviates
about 0.03 mm radially); they are sampling acceptance bounds, not certified
continuous error bounds. Neither pitch nor diameters are rounded to a standard.

Changing pitch/diameters, incompatible hands, unequal starts, missing paired
flanks/bounds or body evidence failing the run's existing validity policy are refused. Isolated blends are not thread
candidates. Supported profiles have radial depth/pitch 0.2–0.9; shallow spiral
channels, round-wire springs, rounded/square profiles without the required flank
and carrier evidence remain unclaimed. Geometrically identical screw/groove
profiles cannot reveal intended use; this is geometric recognition, not recovery
of manufacturing intent. Refusals never suppress source faces.

## Compatibility and runtime

See [recognition document 4](recognition-4.md) for fields and frame semantics.
The byte comparison and paired release benchmarks below include STEP import,
recognition, fingerprints, document construction and JSON serialisation. They do
not measure draftwright rendering, which is unchanged by this repository alone.

Comparing complete CLI stdout against the saved `origin/main` release binary gave
**98/100 byte-identical outputs**. The only changes are the two threaded files.
On those, all pre-existing non-thread document records and every face fingerprint
are also exactly unchanged; only thread records, claimed freeforms, feature
fingerprints and the document version differ. Five obsolete connector
correspondence-ambiguity exceptions were removed: its former strip records now
belong to a single thread. The captured Python evidence differences are explicitly
pinned for the five new occurrences and 144 claimed B-spline faces.


On this aarch64 host with Rust 1.99 release builds, the median of three alternating
pairs per file was:

| Part | origin/main CPU s | New CPU s | Change | Individual CPU ranges (main / new) |
|---|---:|---:|---:|---|
| threaded_connector_109.step | 2.716 | 2.856 | +5.2% | 2.705–2.888 / 2.496–2.915 |
| cgb243.step.gz | 11.261 | 11.358 | +0.9% | 11.224–11.313 / 11.347–11.395 |
| cgb202.step.gz | 29.095 | 29.592 | +1.7% | 25.276–30.318 / 29.099–30.943 |
| cgb203.step | 15.047 | 15.135 | +0.6% | 14.994–15.393 / 14.963–15.700 |
| cgb242.step.gz | 12.446 | 12.694 | +2.0% | 12.379–12.455 / 12.535–12.939 |

CPU is user + system time, with CLI stdout discarded. The connector's ranges
overlap substantially, so its percentage should not be treated as a precise
regression estimate. Added recognition work is about 0.10–0.14 s on the required
threaded parts at these medians; this does not quantify the downstream savings
from conventional thread drawing. `tools/benchmark_threads.py` reproduces the
paired measurements with supplied baseline/current binaries and the corpus path.

Validation: `cargo fmt --check`, `cargo clippy --all-targets --release -- -D warnings`,
and `QUIDDITY_CORPUS=/Users/paul/repos/quiddity/tests/corpus cargo nextest run --release
--test-threads 1 --no-fail-fast` passed: **274 tests, zero skips**, including corpus,
determinism, correspondence, both recognition frames and captured evidence.
