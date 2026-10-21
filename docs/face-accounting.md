# Face accounting (recognition document 5)

Every newly emitted document, including `quiddity serve`'s `result.document`, uses
`quiddity-rust/recognition/5`. `face_accounting` is an array of exactly `face_count`
objects: array index is the original file face index. Record order, IDs, parameters,
face claims, fingerprints and evidence status are unchanged. Precedence is:

1. `{"group":"feature","record_ids":[...]}`: every published non-refusal record
   naming the face, in document order, including derived and body-description records.
2. `{"group":"refused","refusals":[{"record_id":"...","reason":"..."}]}`:
   every named refusal covering the face, in document order, retaining its reason.
3. `{"group":"body","reason":"stock_box_side"}`: positive evidence below.
4. `{"group":"unexplained"}`: no sufficient explanation. Review these faces.

Thus a refusal cannot erase a feature claim, and body evidence cannot erase either.
A feature face may belong to several records but belongs to only one accounting group.
Refusals overlapping a feature remain available in `records`.
Readers accept older documents: absent `face_accounting` deserializes to `None`,
which means **unknown accounting**, never “no unexplained faces”. New empty parts
have `Some([])`. `SCHEMA_VERSION` and `schema_version(features)` now always select 5;
`LEGACY_SCHEMA_VERSION` remains the historical version-3 constant for readers.

## Positive body rule: `stock_box_side`

The face must be an outward planar side of its own solid's bounding box **in the
inferred part frame**, with a closed four-line loop visiting the four distinct box-side
corners in order. Every edge must join adjacent corners (no diagonals). The entire
rectangle spans both transverse solid extents. Interior loops (holes/pockets opening
through the stock surface) are allowed; they do not make the surrounding stock a feature.
Notched, rounded, split or partial outer loops do not qualify.

The solid must have full geometric validity. All its faces must be native planes,
cylinders, cones, spheres or tori, and all boundary curves must be lines, circles or
ellipses, so sampled freeform extrema cannot supply body evidence. All three box
extents must be finite and exceed 100 times the distance tolerance. The face must
belong to that solid. Plane normal must point toward the corresponding extreme;
axis alignment tolerance is `1e-9`, and corner/extreme distance tolerance is
`1e-7 * max(1 mm, solid maximum extent)`.

This deliberately stricter rule excludes partial boss caps, step treads, internal
walls, pocket floors, chamfers and curved profiles. Mere extremality, convexity,
an unclaimed face, or a large area is insufficient evidence. There is currently no
turned-body rule: even an unclaimed outer cylinder remains unexplained. Invalid,
shell-only, freeform-bearing, degenerate, or frame-refused input supplies no body
evidence. Unsupported cases remain unexplained, unless claimed or refused.

The rule is symmetric under frame axis permutation and reversal. It is evaluated
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
increase), and the four group counts. Read/recognition failures count all labelled
faces as unexplained and remain hard failures in the test and baseline generator.
Labels never enter the recognition or body algorithm.

The baseline format is version 2 for the added accounting fields. Regeneration with
`tools/update_mfinstseg_baseline.py` checks unchanged identities, input hashes,
mapping and supports, non-regressing feature scores, zero hidden feature faces and
non-increasing stock-unexplained counts per model. Changes require baseline review;
raising a limit is never automatic. Dataset files remain external and read-only.
