# Presentation editor fixtures (#142 round 2)

`spindle.step.gz` is a deterministic gzip copy of the supplied
`q137-samples/spindle.step`, Specify's written GRM-02 spindle. Its OCL v1 licence
and attribution are retained in `LICENSE-gramil`. Tests pin the Datum A plane's
source origin `[16.8732, 0, 4.70608]`, both datum links and all 14 graphic members
of its camera-less `PMI callouts` draughting model.

`ctc01-text-filled.step.gz` is **authored regression data derived from NIST CTC-01**.
The inspected NIST archive (SHA-256
`1fb91bb8ff0fe02032b948fda0775bc74591cd0bebc0988347d32574e5884f90`)
contains 28 STEP files; none contains any `TEXT_LITERAL` entity or subtype.
Unmodified CTC-01 has zero text literals and 2,754 filled triangles from its
`COMPLEX_TRIANGULATED_SURFACE_SET` records. It cannot truthfully be advertised as
a native mixed literal/filled sample.

To provide the requested shared mixed sample, `build_fixture.py` adds exactly
one labelled, authored `TEXT_LITERAL_WITH_EXTENT`, a proper text style, callout
and annotation plane to the committed CTC-01. The global draughting model and
saved MBD_0 view gain that plane. Original geometry, semantic PMI and tessellated
graphics are untouched. New instance ids are #4377–#4391; the original highest
instance id is #4376. No NIST annotation is transcribed, inferred or relabelled.

The literal is `CTC-01 editable test text`, at model point `[10,20,30]`, X axis
`[1,0,0]`, normal `[0,-1,0]`, baseline-left alignment and extent height 3 mm.
The mixed export has 24 distinct graphic ids, one literal, and 2,754 triangles.
Aliases under several semantic keys must be deduplicated by graphic id before
counting. Tests pin both the original and derived counts, native placements,
plane membership and saved-view direction/up. They also validate newly authored
entities against the haecceity EXPRESS table.

Rebuild from the committed NIST source with:

```sh
python3 crates/haecceity/tests/fixtures/presentation/build_fixture.py
```

This directory is outside root snapshot inputs; no default goldens are changed.

`spindle.semantic.json.gz` is the exact semantic companion exported by the pre-split
CLI at f48c02b, with index anchors. The serializer-only regression compares JSON
values (including content ids and units) against it in both feature configurations.

`spindle-q137.step.gz` is a deterministic gzip copy of the maintainer's own model,
`presfix-inputs/19-spindle-semantic-links-only.step`, supplied for PR #163 round 1.
It was written 2026-10-30 at 17:06 by a #137-era build and retains its saved callouts.

`thread-callouts/` contains deterministic gzip copies of the maintainer-supplied
GRM-01 through GRM-07 geometry (same gramil licence above), the two supplied
Specify-written spindle PMI variants, and semantic companions made by accepting
defaults in Specify with the local thread-callout kernel. These are presentation
regression inputs, outside the 2,700 recognition snapshots. `shank-editor` is the
supplied editor scene's semantic companion (48 items), retained independently of
the current planner's additional support/provenance rows. The geometry and source
PMI files are unmodified. Source files remain evidence, not repaired exports.
