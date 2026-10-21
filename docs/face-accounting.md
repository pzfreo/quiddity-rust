# Face accounting (recognition document 5)

Every newly emitted document, including `quiddity serve`'s `result.document`, uses
`quiddity-rust/recognition/5`. `face_accounting` is an array of exactly `face_count`
objects: array index is the original file face index. Record order, IDs, parameters,
face claims, fingerprints and evidence status are unchanged. Precedence is:

1. `{"group":"feature","record_ids":[...]}`: every published non-refusal record
   naming the face, in document order, including derived and body-description records.
2. `{"group":"refused","refusals":[{"record_id":"...","reason":"..."}]}`:
   every named refusal covering the face, in document order, retaining its reason.
3. `{"group":"body","reason":"..."}`: one of the positive evidence codes below.
4. `{"group":"unexplained"}`: no sufficient explanation. Review these faces.

Thus a refusal cannot erase a feature claim, and body evidence cannot erase either.
A feature face may belong to several records but belongs to only one accounting group.
Refusals overlapping a feature remain available in `records`.
Readers accept older documents: absent `face_accounting` deserializes to `None`,
which means **unknown accounting**, never “no unexplained faces”. New empty parts
have `Some([])`. `SCHEMA_VERSION` and `schema_version(features)` now always select 5;
`LEGACY_SCHEMA_VERSION` remains the historical version-3 constant for readers.

## Positive body rules

### `stock_box_side` (unchanged)

The face must be an outward planar side of its own solid's bounding box **in the
inferred part frame**, with a closed four-line loop visiting the four distinct box-side
corners in order. Every edge must join adjacent corners (no diagonals). The entire
rectangle spans both transverse solid extents. Interior loops (holes/pockets opening
through the stock surface) are allowed; they do not make the surrounding stock a feature.
Notched, rounded, split or partial outer loops do not qualify for this code.

The solid must have full geometric validity. All its faces must be native planes,
cylinders, cones, spheres or tori, and all boundary curves must be lines, circles or
ellipses, so sampled freeform extrema cannot supply body evidence. All three box
extents must be finite and exceed 100 times the distance tolerance. The face must
belong to that solid. Plane normal must point toward the corresponding extreme;
axis alignment tolerance is `1e-9`, and corner/extreme distance tolerance is
`1e-7 * max(1 mm, solid maximum extent)`.

### `stock_box_notched` and `stock_box_split`

Both retain the validity, native geometry, finite extents, outward principal normal,
solid membership and tolerances above. The **entire face** must lie on its outward
box support plane. In addition, one of its own boundary vertices must be a corner
of that box side (both transverse coordinates at solid extrema). Another face's
corner cannot establish evidence for an isolated island. The outline may contain
any number of line/conic edges and may have cut openings.

A potential underlying platform vetoes the new rules. Collect all interior planar
faces with the same outward normal; sort their axial levels and group consecutive
levels within the distance tolerance. For each group, merge the face bounds. If
that union reaches either solid extreme in **each** transverse axis, abstain.
This intentionally rejects some stock surfaces beside large cut floors. It also
rejects an unrecognised edge pad's cap: the lower platform reaches the remaining
perimeter. Combining split lower platforms catches a rib that divides its base.
Grouping uses geometric distances, never coordinate rounding or hash bins.

`stock_box_notched` means this is the only outward face on that support plane.
`stock_box_split` means several outward planar faces share it, as when a through
cut splits a stock side. Each accepted fragment supplies its own stock corner.
The unchanged rectangle rule is tried first, so existing evidence keeps its code.

The positive argument is a surviving vertex of the solid's rectangular envelope,
on its outward support plane, rather than an interior cut surface. Pocket floors,
cut walls and step treads inside the envelope cannot qualify. Isolated boss caps
lack that corner; edge pads and ribs on a lower exterior platform are vetoed even
with no feature records. Tests ask the geometry routine directly, independently
of recognition precedence, for a round boss, an edge pad and a rib. Extremality
alone was evaluated and rejected because it accepts the round boss cap. No label,
record availability, feature count or dataset identity selects body evidence.

This is conservative evidence for box stock, not a reconstruction of design intent.
Unusual profiles still need review. There is no turned-body rule: maximum radius
can also describe a flange or an unrecognised boss, and the stronger full-axial-span
cylinder candidate supplied no unclaimed corpus examples with two complete end
rings. Cones can themselves be drawn tapers/chamfers. Curved faces remain unexplained
unless a record or refusal accounts for them. Invalid, shell-only,
freeform-bearing, degenerate, or frame-refused input supplies no body evidence.

The rules are symmetric under frame axis permutation and reversal. It is evaluated
on the already normalized part, before records map back into file coordinates;
face indices survive that mapping. A caller-space parity result supplies no body
evidence. Motion tests cover Z30, X30 and composed Z17+X23, with translation.

## Independent label measurement

MFInstSeg's stock class is **24**, not an absent instance or an unmapped recognition
family. The pinned AAGNet [class table](https://github.com/whjdark/AAGNet/blob/e0e36b7a12a7f01a29d7be36efc22730d293a1bd/dataset/Utils/parameters.py#L26-L50)
places `stock` at index 24; its [generator](https://github.com/whjdark/AAGNet/blob/e0e36b7a12a7f01a29d7be36efc22730d293a1bd/dataset/feature_creation.py#L167-L170)
initially labels the box faces with that index. Classes 0–23 are labelled features,
including classes without a corresponding recognition family.

The fixed, hash-verified 200-model ratchet retains its existing caller-space feature
precision/recall calculations. Separately it measures the supported default framed
document. Each model and the total record `feature_as_body` (labels 0–23 assigned
body; must be zero), `stock_unexplained` (label 24 assigned unexplained; may not
increase), `stock_body`, counts by body reason, and the four group counts. Read/recognition failures count all labelled
faces as unexplained and remain hard failures in the test and baseline generator.
Labels never enter the recognition or body algorithm.

The baseline format is version 2 for the added accounting fields. Regeneration with
`tools/update_mfinstseg_baseline.py` checks unchanged identities, input hashes,
mapping and supports, non-regressing feature scores, zero hidden feature faces and
non-increasing stock-unexplained counts per model. Changes require baseline review;
raising a limit is never automatic. Dataset files remain external and read-only.


### Disjoint safety sample

For expansion of body evidence, select 2,200 models **before** looking at their
results. Sort all 62,495 STEP stems lexically, remove the fixed 200 baseline IDs,
and from the remaining 62,295 choose `remaining[i * 62295 // 2200]` for
`i = 0..2199`. No failed, invalid, frame-refused or inconvenient model is replaced.
The SHA-256 of the selected IDs, each followed by a newline, is
`0cc1624fd59d444600d738a65445f845ec000893578aee73a2c273285922c155`.
The same `mfinstseg_score` example measures every model; it exits unsuccessfully
if any labelled feature is body. Check recognition/label failures and model counts
in its output too. Keep this measurement external rather than adding 2,200-model
latency to the fixed ratchet. The recognition schema remains 5: these are additional
body reason codes in the existing accounting field, with no record/schema shape change.
