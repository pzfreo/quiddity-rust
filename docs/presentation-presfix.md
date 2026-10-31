# Presentation placement evidence

Measured on `cx/q-presfix`, 2026-10-31. Coordinates are model millimetres.

The old station search required clearance in seven projections around the union
of the part and every annotation. Each emitted note enlarged that union, so later
notes recursively drifted away. The replacement uses a front-plane notes column:
ink left = part max X + 2h, first ink top = part min Z - 2h, Y = part centre Y.
Further blocks stack downward with a 2h gap. Retained general notes are reserved
when appending; feature leaders do not set the notes column.

The spindle and plate use their native PMI. CTC-01 has no part settings in this
fixture, so it receives the spindle settings; cadgen retains its imported notes
and receives those settings too. This makes these measurements reproducible,
rather than dependent on Specify’s unavailable authoring state. Four CTC-01
dimensions lack nominal values and are omitted only from the measurement write.
The native XBF is outside the STEP reader’s formats; cadgen measurements use its
checked-in xcaf conversion (`fixtures/xcaf/pmi_plate.geom.step.gz` plus PMI JSON).

## Part bounds and combined written STEP notes

| Part | Part bounds min → max | Old STEP ink min → max | New STEP ink min → max |
| --- | --- | --- | --- |
| spindle | [-0.00, -4.00, -3.40] → [37.60, 4.00, 4.00] | [-55.53, 0.96, 22.04] → [-9.81, 0.96, 33.87] | [39.95, 0.00, -17.58] → [85.68, 0.00, -5.75] |
| CTC-01 | [-400.00, -225.00, -100.00] → [400.00, 225.00, 50.00] | [-1072.66, 460.71, -122.53] → [-493.69, 460.71, 3.34] | [429.80, 0.00, -255.67] → [1008.77, 0.00, -129.80] |
| plate | [-50.00, -30.00, -5.00] → [50.00, 30.00, 5.00] | [112.10, 70.63, -27.39] → [201.47, 70.63, -9.60] | [54.60, 0.00, -27.39] → [143.97, 0.00, -9.60] |
| cadgen plate | [-50.00, -30.00, -5.00] → [50.00, 30.00, 5.00] | [85.27, 49.17, -12.87] → [174.64, 49.17, 6.56] | [54.60, 0.00, -29.03] → [143.97, 0.00, -9.60] |

The regression test compares actual re-read STEP ink bounds against an independent
`Layout::notes` placement of the combined block on all four parts (1e-6 mm).
JSON retains separate semantic note graphics; the writer combines their settings.

## Generated JSON text placement origins

For `GENERAL NOTES` graphics these are the header’s alignment points, not the
ink’s left edge. The cadgen free-note entries refer to their own literal text.

| Part | Semantic collection / id prefix | Before | After |
| --- | --- | --- | --- |
| spindle | attributes:1d20665664cd9 | [98.94, -1.07, 12.73] | [46.85, 0.00, -22.97] |
| spindle | attributes:4d6e61a00afd3 | [10.90, -6.35, -18.86] | [46.85, 0.00, -17.17] |
| spindle | general:14d36ab044fcca36 | [-47.50, 0.11, -8.03] | [46.85, 0.00, -6.34] |
| spindle | surface_textures:3dc18cc | [50.86, 0.11, -13.45] | [46.85, 0.00, -11.76] |
| CTC-01 | attributes:1d20665664cd9 | [1224.11, -58.49, 186.37] | [517.07, 0.00, -347.84] |
| CTC-01 | attributes:4d6e61a00afd3 | [-1237.87, 268.38, -137.25] | [517.07, 0.00, -274.33] |
| CTC-01 | general:14d36ab044fcca36 | [-862.53, -73.39, 435.78] | [517.07, 0.00, -137.25] |
| CTC-01 | surface_textures:3dc18cc | [615.34, -73.39, 504.32] | [517.07, 0.00, -205.79] |
| plate | attributes:4d6e61a00afd3 | [-148.75, 39.20, 3.37] | [68.07, 0.00, -31.91] |
| plate | general:14d36ab044fcca36 | [68.07, -88.88, -10.75] | [68.07, 0.00, -10.75] |
| plate | surface_textures:3dc18cc | [133.90, 34.60, 2.99] | [68.07, 0.00, -21.33] |
| cadgen plate | attributes:1d20665664cd9 | [-218.09, 56.68, -2.19] | [68.07, 0.00, -58.70] |
| cadgen plate | attributes:4d6e61a00afd3 | [-436.90, -54.38, -2.63] | [68.07, 0.00, -47.35] |
| cadgen plate | general:14d36ab044fcca36 | [-284.99, -1.15, -23.63] | [68.07, 0.00, -26.19] |
| cadgen plate | notes:333c69c03fae8c8805 | [-109.87, 47.48, -3.45] | [90.09, 0.00, -11.13] |
| cadgen plate | notes:b266112fe2e8b54db9 | [-191.30, -1.15, 17.17] | [95.95, 0.00, -18.85] |
| cadgen plate | surface_textures:3dc18cc | [-378.97, 52.08, -3.01] | [68.07, 0.00, -36.77] |

Every new JSON block has an ink left edge at max X + 2h, Y at the part centre,
and nonoverlapping Z intervals with a 2h gap. Its column extends no farther in X
than one widest text block plus the gap. Opening a generated plane view should fit
both the part and that view’s graphics; fitting only part geometry can still crop
any outside annotation. Clearance is defined in the notes plane, not every camera.

## Specify follow-up

- `src/pmi_scene.rs`: add an optional boolean to the scene text-placement type and
  copy `stroked` from each haecceity placement, preserving absent/false/true rather
  than deriving it from graphic source. Keep `text_index`, text, and placement metadata.
- `api.d.ts`: expose the same `stroked?: boolean` on the matching placement interface.
- Test pass-through for generated true, file literal absent, and explicit false.
  The editor should skip the text drawing pass only when `stroked === true`.
  Views use existing direction/up/graphics fields; fit geometry plus member ink.

## Changed feature-on goldens

Only these STEP/OCP pairs in `tests/fixtures/ap242/write-presentation` change:

| Pair | Changed STEP coordinate records | Reason |
| --- | ---: | --- |
| assembly_plate_pin_intent | 6 | Notes and their plane origins move to the bounds corner for both parts. |
| editor_assembly_pin_alone | 3 | Its notes and plane origin move to the bounds corner. |
| every_kind | 7 | Its notes and plane origin move to the bounds corner. |
| pmi_gaps | 2 | Its notes and plane origin move to the bounds corner. |
| pmi_gaps_gram | 2 | The same physical placement rule in the gram-unit case. |
| spool_fits_intent | 3 | Its notes and plane origin move to the bounds corner. |
| string_post_tapped_intent | 3 | Its notes and plane origin move to the bounds corner. |
| thumbwheel_thread_knurl_intent | 3 | Its notes and plane origin move to the bounds corner. |

A decompressed entity diff verifies identical instance inventories and only
`COORDINATES_LIST`/`CARTESIAN_POINT` changes reachable from `General notes` callouts
and planes. All other STEP records are byte-identical. Fresh OCP 7.9.3.1 captures
(including the legacy Python Specify loader from `/private/tmp/claude-501/pysrc/src`)
pass; only their STEP sha256 changes. No reference verdicts change. The snapshot
provenance updates exactly these sixteen input hashes; all 2,700 snapshot payloads
remain untouched. Feature-off goldens and recognition source/output are unchanged.
Text flags and generated JSON views have no existing golden payload to refresh.
