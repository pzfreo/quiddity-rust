# Recognition 6: file-coordinate measurements (for #92)

Readers in draftwright and Specify should accept `quiddity-rust/recognition/6`.
Every published record is usable without the top-level `frame`, which remains recognition
provenance. There is no `local` key, including on nested records. Schema 6 publishes one
family per feature kind: oriented slots, chamfers and slot patterns join their plain
families. Recognition predicates, proofs, frame voting and accepted counts are unchanged.
The crate version and fingerprint format `quiddity-rust/fingerprint/3` are unchanged;
merged records' fingerprint values change within v3, as described below.

An axis-valued field can be a file-axis letter (`"x"`, `"y"`, `"z"`) or a unit vector
`[x,y,z]` in file coordinates. This applies to `axis`, `long_axis`, `width_axis`,
`depth_axis`, `other_axis`, `thickness_axis`, nested `profile.axis`, support-plane axes
and mirror-plane axes. Existing vector-valued directions retain their contracts.
Only records that previously had frame-relative fields acquire new representations.
File-axis measuring letters and coordinate formats remain unchanged. Merged families
expose the optional detail fields below. No `_letter` hints are added.
Publication retains #70's coordinate-floor snap; a snapped letter names the positive file
axis, with coordinates, interval ends and end flags adjusted to that sense.

For a converted record, `origin` is a file-coordinate point: the recognition origin carried
into file coordinates. Scalar coordinates remain signed distances from this point along
their named measuring vectors. Reconstruct a point by adding each coordinate times its
vector to `origin`. Several directions share this one origin. Directions alone (such as a
fillet's `axis`, whose position is already `at`) do not require an origin.

| Measurement | Published measuring directions |
|---|---|
| Slot/pocket/channel `w_center`, `lo`/`hi`, `d_lo`/`d_hi` | `width_axis`, `long_axis`, new `depth_axis` |
| Riser `positions`, `other_positions`, `z_lo`/`z_hi` | `axis`, `other_axis`, new `height_axis` |
| Level `z`, `x_span`, `y_span` | new `height_axis`, `x_axis`, `y_axis` |
| Turning/prism stations, profile `span`, run intervals | the record's `axis` |
| Plate `u`/`v`, two-dimensional sections and flat `axis_line` | new ordered `section_axes` (the original two transverse measuring axes) |
| Pad `x0`/`x1`, `y0`/`y1`, `z0`/`z1` | new ordered `basis_axes` |
| Unequal ramp `half_widths` | new `width_axis`, low side first |
| Gusset support coordinates, mirror-plane coordinate | axis in the corresponding support/mirror pair |

Riser `vertical` means parallel to the published `height_axis`, not necessarily file Z.
`lo_at_envelope` and `hi_at_envelope` describe the low and high ends along `height_axis`;
they follow those endpoints when sense reverses. `body_levels` remains ascending along that
same published height direction. `open_sign`, `depth_sign`, pad direction
and gusset support-side signs refer to their published directions. Empty position arrays
remain empty. Nested levels and pattern members use the same decoding rules independently.
An unconverted record without `origin` retains the existing absolute file-coordinate rules.
Flat `axis_line` gives the two coordinates along `section_axes`. Start at
`q = origin + axis_line[0] * section_axes[0] + axis_line[1] * section_axes[1]`, then move
along `axis` until `dot(q - origin, axis_direction) = 0`. This also handles a stock cylinder
slightly inclined to its principal measuring axis. The resulting point lies on the stock axis.
Flat `stock_span` already uses `axis_direction` from the file origin and retains that rule.

A converted `body_key` becomes `{origin, axes, bounds, volume, area}`. `axes` is an ordered
three-direction basis; `bounds` is three `[lo,hi]` distance intervals from this origin.
A converted turned-profile `body_bounds` uses `{origin, axes, bounds}`. These preserve the
oriented box, not its larger file-aligned envelope. Unconverted box arrays retain their
old layout. `profile.axis_origin` is a full file-coordinate point after conversion.

Already explicit passage/recess frames remain self-contained. If independently rounded
reanchoring would invalidate a previously proved recess, carry its original relative geometry
and complete measuring frame without a second rounding; do not discard it or require provenance.

## Merged families and IDs

Publication merges `oriented_slots` into `slots`, `oriented_chamfers` into `chamfers`,
and `oriented_slot_patterns` into `slot_patterns`. Recognisers, their proofs, frame
voting and accepted occurrences are unchanged. No patterns are rediscovered and no
occurrences are deduplicated. Other families, including oblique through steps and blends,
remain separate.

| Source record | Published mapping |
|---|---|
| Oriented slot | `Slot`: `width_direction` → vector `width_axis`, `long_direction` → vector `long_axis`; `source.frame.run` → `depth_axis`; `source.frame.origin` → `origin`. Project source profile vertices into width/long directions for `w_center` and `lo`/`hi`; `source.run_interval` supplies `d_lo`/`d_hi`. Keep intrinsic `width`, `length`, and `body_key`. |
| Slot detail | Optional `source` retains the proved passage geometry; readers accept it absent or null. Plain slots currently publish null source. Unproved `end_radius` and `corner_radius` are null. No rectangular source is fabricated. |
| Oriented chamfer | `Chamfer`: `run` → vector `axis`, `turned:false`; retain `leg1_direction`, `leg2_direction`, `length`, `support_spans`, and `body_key`. Plain chamfers have null for these optional, unproved details. Support spans retain their distances along the run from `at`. |
| Oriented slot pattern | `SlotArray` or `SlotGrid`, with members mapped by the same slot conversion. Grids add explicit `row_direction` and `col_direction` vectors; the precision rule below applies to plain slot arrays and grids. |

Each family contains accepted plain records first, then accepted oriented records, in
each recogniser's existing deterministic order. IDs are `<merged-family>/<index>`.
Promoted measuring axes are normalised to unit length; existing detailed source vectors
retain their schema-6 precision. File-axis letters stay letters.
The append offset is the number of plain records. Pattern members, fingerprint IDs,
evidence status and face-accounting references use this same translation.

## Fingerprints and saved references

Fingerprints remain `quiddity-rust/fingerprint/3`. Their values change for merged
records because they include family and ID; oriented chamfers acquire the common
`turned:false` trait and oriented slots drop source-end proof traits. Optional detailed
geometry is not a new feature kind or a semantic trait. The slot-pattern precision
correction below also uses measured axes and pitches in intrinsic fingerprints.

For references saved from 0.1.1's schema-5 `oriented_*` records, use the merged family
and sorted defining faces, or run `quiddity correspond old.json new.json` on the saved
CLI recognition output and the new recognition (or STEP). Both use v3 fingerprints;
correspondence maps the former family semantics while retaining the old IDs on the
old side. Three slots, their pattern and the captured 0.1.1 oriented chamfer are pinned
by the legacy correspondence regression. Other fingerprint formats require
re-recognition. A bare recognition document without fingerprints must be paired
through its source STEP or defining faces.

Numeric IDs are run-local: do not persist a renamed family with its old numeric
suffix. Move saved selections and Specify `family:faces` overrides by defining faces
or correspondence's old/new ID pairs. Face indices are specific to a STEP revision;
use correspondence when the source revision changes.

Rust consumers obtain the merged contract through `PublishedFeatures::families()` or
serialization. The public `Deref` into the working inventory is removed so typed
field access cannot expose an unmerged family; working inventory access remains
internal. The published registry has 45 families; raw recogniser APIs retain their
internal types. Intrinsic fingerprints use exact-motion working measurements, with
axes resolved at extraction; no provenance-frame repair is needed by correspondence.

## Slot precision correction (for #92)

The plain recess recogniser retains its legacy two-decimal positions, for stable
recognition and file-axis output. `Slot::published` in `recess_records.rs` applies
that grid after reductions have measured the raw spans; channels apply the same
position grid in `recess_core.rs`. These rounded values are unsuitable as distances
in a turned measuring basis. Converted slots now publish their retained raw
`w_center`, `lo`, `hi`, `d_lo` and `d_hi`; these distances are not independently
rounded to a fictitious file grid. The measuring basis still uses #70's
coordinate-floor snap. File-axis extents keep their existing publication rule.
Pockets and channels are outside this correction.

Slot patterns retain exactly their recognised members and grid cells. Publication
remeasures arrays and grids from the raw member centres, before any position
rounding, and snaps explicit directions to a file axis only within the existing
coordinate-floor movement bound. Array sense runs from the endpoint with the
lexicographically greater defining-face list to the other endpoint; reverse the
member list together when necessary. This makes sense stable under rigid moves
that preserve face identity. Grid pitches and centre come from those same raw
centres. File-axis pattern geometry is also remeasured, so a copy that happens to
land on file axes does not retain a rounding-induced skew. Feature IDs, proofs,
inventory, schema 6, fingerprint version 3 and the crate version are unchanged.


## Annular face grooves

The optional `annular_grooves` family contains `AnnularGroove` records and is omitted when
empty. This additive family keeps `quiddity-rust/recognition/6`; existing families retain
all their records. It describes a blind annular face recess of rectangular radial section,
such as a face-seal housing. It makes no nominal O-ring size or intended-function claim.

Each record publishes `axis` (a unit vector from the floor towards the mouth),
`floor_center` (a point in file coordinates), `inner_radius`, `outer_radius`, and `depth`
(in millimetres). The floor and both cylindrical walls are defining faces; planar faces
at the two mouth rims are consulted evidence. All must belong to one strictly valid solid.
The two complete circular floor boundaries must be concentric, their cylindrical walls
coaxial, the floor junctions concave, and the mouth junctions convex. Both walls must end
at the same open plane. Interrupted walls, treated rims, unequal depths, invalid ownership,
and competing defining claims are refused. External turned bands remain `grooves`.


## Evaluation mappings

`tests/fixtures/mfinstseg_mapping.json` schema 2 permits per-family `selectors`. Each
selector names a JSON-pointer `field` in the published record, its comparison, and its
geometric `evidence`. All predicates on a family must match before its defining-face set
is passed to the scorer. Record indices stay paired with their defining faces, and labels
never select predictions. The circular-through-slot mapping selects the existing
`section_recesses` classification `feature_kind: channel`, `section_shape: circular`:
these are open cylindrical troughs with both run ends proved open. This changes evaluation
correspondence only, not the recognition schema or the geometry engine.


## V-section through channels

The optional `v_channels` family contains `VChannel` records, omitted when empty.
It keeps recognition schema 6 and does not change any other family's records.
`axis` is the unit run vector (its sign is a presentation gauge), `opening` is the outward
unit normal to the lip plane, and `at` is the trough edge's midpoint in file coordinates.
`length`, `width`, and `depth` are millimetres; `included_angle_degrees` is the angle
between the two sides, without any preferred angle or dimensional ratio.

The defining faces are two complete rectangular planes meeting concavely at the trough.
Their opposite lips must be coplanar and their ends aligned. Each lip and each end must
meet an outward-facing exterior plane convexly. The apex must lie between the lips when
projected onto the mouth; interrupted walls, missing ownership, and competing wall claims
are refused. All defining and consulted faces require one strictly valid solid. A pair of
walls already contained in a proved closed section passage is part of that complete
passage, not a separate open channel. This exclusion leaves passage recognition intact.
