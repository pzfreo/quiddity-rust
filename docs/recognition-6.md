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
(in millimetres). All floor pieces and both cylindrical walls are defining faces; planar faces
at the two mouth rims are consulted evidence. All must belong to one strictly valid solid.
The two complete circular floor boundaries must be concentric, their cylindrical walls
coaxial, the floor junctions concave, and the mouth junctions convex. Both walls must end
at the same open plane. Interrupted walls, treated rims, unequal depths, invalid ownership,
and competing defining claims are refused. External turned bands remain `grooves`.


The annular floor may be partitioned into native planar faces joined smoothly on the same
plane. Internal edges cancel, leaving exactly two nonbranching closed boundary cycles.
A rim may contain several circular edges, but their centres/radii must agree and their
actual angular intervals must cover a complete turn. Counting overlapping arc lengths
as a full rim is refused. The cylindrical walls must still each be complete; every floor
piece is retained in the defining claim and strict ownership proof.

## Evaluation mappings

`lip_bevelled_blind_slot` selects an existing complete `edge_open_recess` geometry:
exactly two parallel straight sides, a flat floor and one planar lip bevel flaring
outward into the open mouth. Its five profile vertices and normal planar run-end
surfaces must prove exactly one capped and one open end. The run must be strictly longer
than both nominal width and depth beyond coordinate/interval rounding uncertainty;
ambiguous run/depth roles remain excluded. `coordinate_decimals` and `interval_decimals`
are the published profile-point and run-station precision (4 and 3). The existing
section-recess recogniser proves the swept region and solid. The mapping preserves
its defining faces, including the bevel, and does not add consulted cap faces.

`compatible_polygon` is a conservative scoring predicate over the published section
coordinates. `shape` is `triangle`, `rectangle`, or `regular_hexagon`; `decimals` must
be the field's publication precision (4 for section-passage boundary points, 3 for
prismatic-pocket corners). A rectangle has four right angles; a regular hexagon has
six equal edges and sixty-degree exterior turns. The polygon must be straight and
strictly convex at that coordinate resolution. Dot/cross and edge-length tolerances
come from half-grid coordinate rounding, not a model or dataset scale. Either winding
and arbitrary section orientation are supported.

A complete alternative canonical shape is excluded. Curved profiles, clipped corners,
intersections, irregular polygons and unresolved edges remain ambiguous and retain the
old correspondence; raw wall/vertex counts do not identify their original cut shape.
Likewise an unknown hole end is retained for blind-hole scoring, while a proved through
end is excluded. Pocket flags exclude only a proved incompatible corner or curved-end
subtype. A missing flag does not prove the opposite subtype. These predicates never
read labels, model identity, topology or recognition internals beyond published records.

The round scoring correspondence also includes complete rolling-ball `blends` selected
by the published material side (`convex` or `concave`). MFInstSeg round includes both
external rounds and internal fillets, as confirmed by its labelled concave blend faces.
Their smooth supports and complete path are already proved; this also includes external
rounds below the legacy fillet radius floor.
The fillet recogniser's radius and convexity rules are unchanged. Defining-face sets
shared by a fillet and a blend still count as one prediction.

The MFInstSeg scorer evaluates the internal caller-space inventory. Its chamfer class
therefore names both `chamfers` and `oriented_chamfers`, which this document already
publishes together as `chamfers`. This correspondence changes scoring only: a complete
oriented bevel is already proved by its recogniser and its defining face remains the
claim. Identical face sets across either source count once.

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


## Semicircular-bottom blind slots

The optional `semicircular_blind_slots` family contains `SemicircularBlindSlot` records,
omitted when empty, within recognition schema 6. Its section has one semicircular bottom
and two tangent straight sides. This differs from `round_bottom_blind_slots`, whose
section has two quarter-cylinders and a flat floor; that existing family is unchanged.

`axis` points from the open run end towards the blind cap; `opening` points out of the
lateral mouth. Both are unit vectors in file coordinates. `at` is the section bounding
box's centre halfway along the run. `length`, `radius`, and `straight_height` are
millimetres: section width is twice the radius and depth is radius plus straight height.
The cylinder, both straight sides, and the planar blind cap are defining faces. The
convex planar faces at the run mouth and lateral opening are consulted evidence.

The complete cylinder boundary must have two semicircular rims and two straight run
edges; the rectangular sides must be tangent to it and rise equally towards the lateral
mouth. The blind cap has exactly that U profile and meets all three swept surfaces
concavely. Both mouth boundaries meet outward-facing planar context convexly. Every
source face must belong to one strictly valid solid. Interrupted or treated boundaries,
missing caps/ownership, and competing defining claims are refused. The cylinder axis
and unique cap establish run/depth roles without a dimensional preference.

A trough may instead be rounded: one concave cylinder along the run whose two straight edges
each meet a flank tangentially, its two ends opening convexly onto exterior planes like the
flanks'. The round is consulted, not defining; it keeps its own `blends` record, as a slot's
corner rounds do. `trough_radius` is then present. `at` and `depth` refer to the flanks'
intersection, the virtual sharp trough the round replaces, so a rounded and a sharp V of one
section publish the same `depth`, `width` and angle. The round must be tangent on the lip side
of that intersection for both flanks. A sharp trough omits `trough_radius`, so its record and
fingerprint are unchanged. On NIST CTC-01 the two V notches through the plate edge (flanks
11/13 and 15/17, R10 troughs 12 and 16) are such channels along the plate's thickness.

## Rectangular through-notches

The optional `rectangular_through_notches` family contains `RectangularThroughNotch`
records, omitted when empty. It keeps recognition schema 6 and fingerprint format 3.
A notch is a U cut through a recognised plate from its edge: a bottom wall and two parallel
side walls facing each other, all planar and perpendicular to the plate axis. `axis` is the
plate's thickness direction (its sign is a gauge), `opening` the bottom wall's outward normal,
out through the mouth, and `at` the centre of the bottom wall, midway between the sides and
through the thickness, in file coordinates. `width` separates the side walls, `depth` runs
from the bottom wall to the mouth plane and `thickness` is the plate's; all are millimetres.
`corner_radius` is the radius of the two concave corner rounds, or null for sharp corners.

The defining faces are the bottom and both side walls. Each is one loop of four lines: two
along the plate axis spanning exactly the plate's raw `lo…hi`, and two shared convexly with the
plate's own faces at **both** slab ends, as edges of those faces' outer loops. The bottom meets
each side either across a concave line or through one concave cylinder along the axis, tangent
to both and spanning the slab; the two corners are both sharp or both rounded with one radius.
Each side wall's other axial edge is the mouth: a convex edge with a planar wall facing the
bottom's way, and both mouths lie in one plane beyond the bottom. Corner rounds keep their own
`blends` records, as a rectangular slot's do; they, the mouth walls and the plate faces are
consulted. Defining and consulted faces require one strictly valid solid.

These conditions separate the notch from its neighbours without any dimension or ratio: a
closed slot's walls have no mouth and lie on inner loops of the plate faces; an edge-open
recess's walls stop at its floor rather than the far slab face; the waist between two thicker
end blocks has side walls taller than the slab (they belong to the blocks, and face
accounting gives them `stock_wall` evidence). A wall pair a full-span floored channel defines
belongs to that channel, which is how a notch through a part's whole thickness is already
published; a plate needs the body to extend beyond it along its axis. A face proposed by two
notches defines neither, and a notch is dropped with its plate when reconciliation rejects the
plate. On NIST CTC-01 the two 85 × 25 R10 notches in the +y plate edge are
`rectangular_through_notches/0` (faces 31, 33, 35) and `/1` (37, 39, 41).

## Polygonal boss caps

A polygonal boss's record also claims its free-end cap, the face its proof already
consulted, after reconciliation: a plane facing along the boss axis at the record's `base` or
`top`, lying within the side ring's transverse extent. A support face or any wider face the
ring ends on is not claimed. Feature selection and record fields are unchanged; the boss's
fingerprint `faces`, area-weighted `position` and `neighbourhood` change within format 3.
