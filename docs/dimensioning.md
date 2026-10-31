# Shared dimension requirements (slice 1, #141)

`quiddity::dimensioning` decides which manufacturing dimensions a prismatic part
requires. It has no sheet, view, label, units formatting, lane, placement, or editor
scheme. Recognition and the existing CLI are unchanged. Specify and Draftwright
integration belong to their own repositories.

```rust,no_run
use quiddity::{dimensioning::{Input, Policy, plan}, read_step_file};
let part = read_step_file(std::path::Path::new("plate.step"))?;
let input = Input::from_recognition(&part, vec![], Policy::default())?;
let (required, conveyed) = plan(&input);
# Ok::<(), Box<dyn std::error::Error>>(())
```

`cargo run --release --example dimension_plan -- plate.step` prints the typed input,
required set and conveyance records as JSON. This is an example, not a new CLI path.

## Input and coordinates

`Input::from_recognition(&Part, Vec<Datum>, Policy) -> Result<Input, String>` infers
the part frame, transforms the part in memory, and runs the existing recognition
once. Face indices remain those of the caller's part. All input supports, hole axes,
pattern centres, plane coordinates and bounds use **that part frame**, in mm. Use
`input.frame.to_world(point)` to recover file coordinates. Frame refusal is an error.
The frame may permute or reverse file axes: the minimum of an inferred axis need not
be the minimum of a file axis. Supply physical datum face sets to choose a particular
end. Frame gauge ambiguities retain recognition's existing deterministic representative.

The adapter supplies typed `holes`, `patterns`, `planes`, `round_axes`, `bounds`,
`datums`, `specified` and `policy`. Round axes only corroborate small bolt circles;
no boss requirements are generated. Planar supports use recognition’s plane-level proposals in each axis (0.01 mm
station clustering, 1% projected area threshold, interior margin at most 0.6 mm or
a quarter of the body span), grouped by body, principal normal and station. Hole/recess floors and boss/pad caps belong to their features,
not the generic ladder; envelope faces and datum faces survive this exclusion.
Turned-feature shoulders are excluded from the generic ladder. Curved envelope
sides receive synthetic envelope supports; these have no face anchors.

Consumers with existing recognition can build `Input` directly, using the same
contract: unique face-derived feature ids, sorted face sets, finite coordinates,
positive sizes, valid axes `0..3`, normalized drilling directions and canonical
size-role names. `Plane.body` and `Support.body` must agree. Do not substitute
recognition ordinals for identity. `Hole.depth` is blind depth; `traversal` is a
through segment's material length, used only to count drilling across narrow slits.
`specified` contains the file's already stated requirements, with its tolerance
classes and ids; the adapter does not read or reinterpret semantic PMI itself.

## Output and PMI

`plan(&Input) -> (Vec<Required>, Vec<Conveyed>)` returns sorted, deterministic values.
`Required` has an id, `Kind::Size { feature, role, kind }` or
`Kind::Location { from, to, axis }`, nominal, count, optional `PatternRole`, tolerance,
optional reference carrying ids, and all covered feature ids. Separate bore, blind
depth, counterbore, spotface, outer-step and countersink quantities are separate size
requirements. A drawing may combine these into one callout; a callout count and an
atomic requirement count are therefore different measurements.

`Support` retains feature id, face set, body, point and whether it is a physical
plane rather than a point/axis. Requirement ids derive from faces and semantic
roles; group ids derive from sorted member ids. They are stable under recognition
list permutations, not across arbitrary renumbering of the source B-rep. Use
quiddity correspondence to transport ids across changed topology. Characteristic
numbers can attach to these ids. `Conveyed` explicitly maps retired member or
coincident requirements to surviving ids with a reason. Its `by` is flattened;
consumers need not follow chains of omitted ids. `covers` retains displaced claims.

`Required::dimension_kind(resolve)` returns haecceity's `pmi::DimensionKind` using a
consumer-supplied feature-arena resolver. Planner string ids never pretend to be
PMI arena indices. `Pattern::pmi_feature(resolve)` returns
`Feature::Group { kind: PatternOfFeatures, members }`. For counted unpatterned
sizes, consumers can build a `MultipleElements` group from `covers`. Support face
sets can be lowered to `Feature::Items` with `Anchor::Face`. Angles lower as angular
sizes with Equal selection; the nominal remains in degrees.

## Rules and provenance

Adapted from pzfreo/draftwright-rust `origin/main` **1c830977** (same copyright owner):

- `dims/holes.rs`: patterns before single holes; full evenly spaced principal-axis
  lattices use their centre and one pitch per varying axis. Lines use their first
  member and counted intervals. Irregular/rotated lattices fall back to individual
  locations. Complete identical machining specs share counted sizes. Signed drilling
  directions, blind depths and every enlargement remain spec constraints.
- `bridge/holes.rs::established_with`: at least two identical members; reject missing,
  repeated, oblique or coaxial members. Five holes establish a bolt circle; smaller
  circles need a parallel hole/boss axis within 0.5 mm of the centre. Circle diameter
  and centre state the pattern. Through traversal lengths do not split identical
  requirements. Plain coaxial through segments separated by less than one bore
  diameter count one drilling; wide gaps, blind or enlarged bores remain separate.
- `dims/pmi_policy.rs::{origins,selected_origins}`: Automatic uses the first usable
  datum ordered by label then face set per axis; otherwise the box minimum. Policy
  Minimum/Maximum/Datum overrides it. A missing or unusable datum falls back to
  minimum. Zero distance creates no dimension.
- `dims/levels.rs::ownership`: one owner per support-plane interval. Slice 1 uses
  the common axis-neutral support table: overall extents plus baseline locations
  to internal step planes in all three axes. Out-of-scope feature floors/caps are
  excluded by the adapter rather than acquiring a second ladder owner. Draftwright's
  plate/pad/through-step fixed-point grammar and equal-rise printed ladders are not
  imported; those owners need slice 2/3 feature requirements first.
- `dims/holes.rs::{merge,consolidate}` and `dims/redundancy.rs`: coincident raw spans
  merge only with equal tolerance contracts, count and pitch intervals, within
  0.0001 mm, independently of views or labels. General, Basic, Untoleranced and
  exact Explicit contracts are distinct. A specified requirement wins, then lexical
  id; all covered claims survive. Pitch and non-pitch never merge. Different bodies
  never merge coincident supports.
- `lint/redundancy.rs::path`: a bounded breadth-first station walk detects closed
  physical-plane chains per axis, body and tolerance contract. Point locations and
  pitches never supply plane edges. Already specified requirements are authoritative;
  generated determined intervals are conveyed by the existing path. With
  `policy.references`, untoleranced determined intervals remain as references.
  Toleranced requirements never become references. Unrelated equal values do not
  determine one another. Layout and optional sheet references play no role.

Defaults request general tolerance, automatic origins and no references. There is
no precision/label key: any stated precision that must prevent merging belongs in
`Tolerance::Explicit`. Inputs and recognition are never mutated.

## Tests and comparison

The module tests re-express Draftwright requirement assertions from `dims/holes.rs`
(raw ends, tolerance separation, specified priority, id tie-break), `bridge/holes.rs`
(principal axes, identical/blind/through specs, pattern acceptance and small-circle
corroboration, slit drilling counts), `dimension_rules` (grid, blind/through,
counterbore/spotface/countersink quantities, determinism), `repeated_dimensions`
(shared parallel-row pitch, distinct pitch stations, location/level ownership),
`ownership_rules` (one statement per span, toleranced dimensions never reference),
`coverage_holes` (every shoulder station), and `h5_chains`/`lint/redundancy` (physical
station paths, closed chains and optional references). Additional tests exercise
PMI lowering, body isolation, the real recognition adapter and conveyed-id validity.

Skipped deliberately: holes' oblique-view ordering/rim tips and slot/boss precedence;
bridge JSON field parsing (typed input), boss-frame parsing and rendering checks;
all three `dims/recesses.rs` tests (pocket maximum depth, edge-open pocket location,
pocket-on-box location); lint's turned-chamfer/whole-boss plane test; h5 turned
ladders, boss/slot ownership and editor omissions; repeated slot widths/boss heights;
coverage's double-D, independent seat, profile-angle and ink assertions; all view,
placement, text, precision, rendering and editor-policy assertions in the named suites.
Slots, recesses, bosses, turned ladders and dimension schemes remain later slices.

`tools/compare_dimensioning.py` runs the example and `draftwright plan` sequentially
on a plate and all entries in Draftwright's `tools/quality_parts.txt`. It emits a
JSON-lines report outside the repository: total required quantities, Draftwright
hole/level/envelope statements and expanded quantities, plus **every** unmatched
quantity and its source id. It includes unplaced wants, restores wants conveyed
by hole tables from their `covered` records, and expands compound callouts. Axis matching maps the inferred frame back onto file axes; a reversed
origin can still produce complementary offsets. Use the original plate (its SHA is
in the issue's recorded binding), not Specify's annotated output, which selects
Draftwright's authored-PMI policy. The supplied binary may predate the source;
record its version/date with comparison results. Reports are not snapshot goldens.
