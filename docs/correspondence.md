# Correspondence between revisions

**Status:** design accepted with draftwright-rust's ADR R3 (2026-10-07); implemented in
`src/correspondence/` (fingerprint version `quiddity-rust/fingerprint/1`). See
[Implementation](#implementation) for what was built, the thresholds and how they were tuned, and
what is not done.

## The problem

A STEP file is revised. A hole moves, a boss grows, a slot is added, or the whole part is
re-modelled in another position. The decisions made against the old revision (a drawing's
callouts, a part's fits and datums) should carry over to the new one. There is no construction
history to follow, only two files.

Face indices and quiddity's occurrence refs are stable within one file only. So recognition
results gain revision-stable fingerprints, and a pure function, `correspond`, matches two
results.

## Fingerprints

Every recognised feature and every face gets a fingerprint in the result. It is deterministic,
serialised, and versioned (`fingerprint_version`).

- **Feature:** three parts.
  - *Semantics:* family and subtype, its intrinsic sizes (diameter, depth, width, length,
    radius, angle, as the family has them), and its pattern count and pitch.
  - *Placement:* its axis or normal, and its position in the part's frame.
  - *Neighbourhood:* its subgraph of the attributed adjacency graph (Joshi & Chang 1988). This
    covers the feature's own faces, and the faces it meets with how it meets them: surface type,
    whether each shared edge is convex, concave or tangent, and the meeting face's role (a
    planar face it enters, the floor it ends on, a boss it sits on). It is recorded as a
    canonical, order-independent signature.
- **Face:** surface type and parameters (radius, half-angle, axis), its centroid, area and
  outward normal, and its adjacency signature (the types of its neighbours, and whether each
  edge is convex or concave).

A consumer stores the fingerprint beside each decision, so the old STEP file isn't needed to
rebind.

## `correspond(old, new) → Correspondence`

### 1. Align

- **Find a rigid motion.** From features and planar faces whose fingerprints are distinctive in
  both revisions (unique holes, unique large faces), solve for a rigid motion: Kabsch over
  candidate pairs, with RANSAC.
- **Accept it** when enough fingerprints agree under it.
- **Otherwise** use the identity and mark the result `unaligned`, which down-weights positions in
  the costs below.

### 2. Match the features

- **Costs:** for each family, a cost matrix over old × new. Each cost sums three weighted
  terms:
  - *semantics:* subtype, intrinsic sizes, pattern count and pitch;
  - *neighbourhood:* the distance between the two AAG signatures;
  - *placement:* axis, and position (once aligned).

  The terms do different jobs:
  - **Neighbourhood** is unaffected by rigid motion, and holds when a feature moves or resizes.
    It carries the match where position is weak, in an unaligned revision or for a moved
    feature. It also tells apart features that are alike in size and place but sit in different
    surroundings, such as a hole through a flange and a blind hole into a boss.
  - **Placement** separates what the neighbourhood can't: truly symmetric repeats, such as the
    instances of a hole pattern, whose neighbourhoods are identical.
  - When placement is down-weighted (an unaligned result) and such repeats remain within the
    margin, they are reported as ambiguous, never guessed.

  The weights, like the thresholds, are tuned on revision pairs.
- **Assignment:** one to one, by the Hungarian method, with an explicit "unmatched" column.
- **Classification:**
  - **carried:** low cost, and the winner clearly ahead of the runner-up;
  - **adapted:** the same feature with changed parameters, reported field by field;
  - **ambiguous:** the margin is small, typically pattern instances and symmetric features. The
    candidates are reported; none is guessed;
  - **orphaned:** an old feature with no match;
  - **new:** a new feature with no match.

### 3. Match the faces

Faces matter for datums and GD&T targets.

- **Seed** from the faces owned by matched features, and from faces that coincide or overlap
  after alignment (the same surface, overlapping by at least 80%).
- **Propagate** by adjacency signature over the attributed adjacency graph, preferring a face of
  the same surface type when breaking ties.
- **Classify** with the same five classes as features.

### Output

The Correspondence holds:

- the alignment, and whether it was found;
- per old feature and face: its class, its match or candidates, and what changed;
- the new features and faces;
- a list of questions for the ambiguous entries.

It is exposed in the library, in the CLI (`quiddity correspond old.json new.json`) and in the
JSON-lines and MCP surface beside recognition.

## Background

- **History-based persistent naming needs the construction history.** Examples are Capoyleas,
  Chen & Hoffmann (CAD 1996), Kripac (CAD 1997), Marcheix & Pierra's survey (SMA 2002), OCCT's
  TNaming and FreeCAD 1.0's element maps.
- **Onshape's re-import matcher is history-free.** It is described by Baran & Schulz,
  US 11,288,411: exact seeds, then overlap, then adjacency-signature propagation, with the rest
  left unmatched.
- **Jones et al., "B-rep Matching for Collaborating Across CAD Systems"** (SIGGRAPH 2023) learn
  the scoring on top of that matcher. They report a residue that stays genuinely ambiguous.
- **Bidarra et al. (2005)** argue for anchoring on features rather than boundary faces.
- **Joshi & Chang (CAD 1988)** introduced the attributed adjacency graph used here for both
  features and faces.
- **Vandenbrande et al. (US 8,576,224)** align two B-reps by matching planar faces with the
  Hungarian method.

## Tests

- **Invariance:** the corpus's invariance variants (moved, rotated, mirrored) must correspond with
  everything carried.
- **Revision pairs:** a capture tool builds revision pairs in build123d (move a hole, resize a
  boss, add or remove a feature, pattern changes, a whole-part motion). Each pair has its
  expected classes, and differences get verdicts as in the other families.
- **Thresholds:** the cost cut-off, the margin and the inlier fraction are tuned on those pairs.
  No published values transfer.

## Implementation

`src/correspondence/` (2026-10-07). `quiddity part.step` now writes each family's records as
before plus a top-level `fingerprints`; `quiddity correspond old new` takes two such results (or
two STEP files) and writes the `Correspondence`. In the library: `correspondence::recognise`,
`fingerprint`, `correspond` and `correspond_with` (explicit `Thresholds`).

### Fingerprints (`fingerprint.rs`, version `quiddity-rust/fingerprint/1`)

- **Features** are read generically from each family's serialised records, so a newly ported
  family is fingerprinted once it records its defining faces (`Features::defining`, filled by
  `features::recognise`):
  - *traits* (the subtype): string, boolean and null fields, and the presence of nested records
    (a counterbore); axis letters are placement, not traits. Pattern families add their shape
    (the record's field names, since the patterns are untagged variants);
  - *sizes*: a fixed list of size names (diameter, depth, radius, legs, pitch, count, …). A
    pattern's `angle` is its orientation in the frame and is left out; a pair of sizes ordered by
    the frame (a ramp's half-widths) is sorted;
  - *placement*: the first axis-like field (a vector or an axis letter) as a line, and the
    area-weighted centroid of the defining faces. Derived patterns take their members' faces.
  - *neighbourhood*: in draftwright's bridge format, with the arc labels below (which depart
    from the bridge's on nearly tangent edges).
- **Faces:** the effective surface type (a B-spline exactly a plane, cylinder… counts as one),
  its intrinsic parameters, axis or outward normal and support point, area, centroid and mean
  outward normal (`Part::face_moments`, the boundary quadrature of `face_mass`), neighbours with
  their arcs, and the sorted adjacency signature. Two departures from the kernel, both for
  invariance: a pair of faces whose shared edges all have normals within 1e-3 (1 − cos, about
  2.6°) of parallel is recorded `smooth` (all within 1e-3 of opposite, `unknown`), a departure
  from the kernel's labels and so from the bridge's (a genuine crease shallower than 2.6° reads
  as smooth), because the kernel's convex/concave call there follows round-off
  and changed with a mere translation on nist_ftc_07, mfcadpp 11512 and sm-hanger; and a face
  whose quadrature gives no area (nist_ftc_10 face 174, stream F's defect) has an unknown area
  and its edge samples' mean as centroid.
- `scale` is the square root of the part's area, which motion leaves unchanged.

### Correspond

- **Align** (`align.rs`): candidate pairs are features and faces whose motion-free fingerprints
  agree, with at most four candidates either way; hypotheses come from every pair and triple of
  pairs (or 3000 of each, sampled deterministically), solved by Horn's quaternion method (Kabsch
  without reflections); the most inliers win, then the smallest residual, then the motion nearest
  the identity. When every agreeing anchor lies on one line (a turned part), the alignment is
  *axisymmetric*: positions are compared by their distance along and from that line and
  directions by their angle to it. The turn about the line is then fixed from the off-axis
  repeats (features and faces alike with up to 64 candidates): each pair votes for the turn
  taking the old item onto the new one, and the turn most old items agree with wins. A part with
  k-fold symmetry about its axis has k such turns (`fold`); the one giving the motion nearest the
  identity is taken. Only a part with no off-axis repeats stays axisymmetric. `symmetric` records
  that another motion fits as well.
- **Features** (`mod.rs`, `assign.rs`): per family, cost = semantics (trait mismatch and mean
  relative size difference) + neighbourhood (multiset distance) + placement (half axis, half
  position). Aligned, a displacement costs its logarithm in tolerances (so millimetres separate
  repeats); unaligned, it is linear and weighted 0.1. The Hungarian method runs on the (old + new)
  square with an unmatched option at half the cut-off. A match is ambiguous when forcing another
  candidate (or none) costs less than the margin more; the duals' reduced costs bound that, and
  only the candidates under the bound are solved again. Matches are carried when nothing changed
  within the tolerances, adapted otherwise with the changes field by field. Unaligned, there is
  no common frame, so placement is left out of the changes: carried then means unchanged in
  everything but placement (the thickened plate's top face, 5 mm higher, is carried).
- **Faces** (`faces.rs`): seeds are faces on one surface after alignment that overlap (estimated:
  areas within 80% and centroids within 20% of the smaller face's size; the fingerprints carry no
  boundary to intersect), then the faces of carried and adapted features assigned within each
  pair. Propagation pairs an unmatched face next to a matched one with an unmatched face next to
  its partner across an edge of the same kind, costed by the face cost plus the share of its
  matched neighbours whose partners the candidate does not touch; a pair is taken when each is
  the other's best by the margin. What remains is assigned per surface type under the feature
  rules (up to 400 faces a type), then propagated again.

### Thresholds and how they were tuned

`Thresholds` (`src/correspondence/mod.rs`) documents each value. In short:

| Threshold | Value | Basis |
|---|---|---|
| `same_rel` | 1e-6 | round-off: moved analytic fingerprints agree to ~1e-9 (except the kernel's sphere-pole areas, below) |
| `freeform_rel` | 1e-3 | B-spline quadrature moves with placement by up to ~2e-4 |
| `position_tol`, `angle_tol` | 1e-5 × scale, 1e-5 rad | round-off, far below a design change |
| `anchor_candidates` | 4 | keeps two- and four-fold symmetric faces as anchors |
| `turn_candidates` | 64 | a turned part's off-axis repeats (24 alike holes on the spool's two flanges) vote for the turn |
| `min_inliers`, `min_inlier_fraction` | 3, 0.5 | aligned revision pairs keep ≥ 70% |
| `placement_reach`, `unaligned_placement` | 0.25 × scale, 0.1 | a whole-reach move costs 1 |
| `cutoff` | 1.0 | matched revision costs are 0.06–0.5 |
| `margin` | 0.05 | unaligned swaps: 20 mm apart 0.064 (kept), 5–10 mm 0.016–0.032 (ambiguous) |

### Results

- **Invariance** (`tests/correspondence.rs`): 100 corpus parts × 6 motions, 10 716 features and
  64 014 faces. Every feature and face is carried to itself except in 19 listed (file, motion)
  cases in `tests/fixtures/known_correspondence.json`, each with a verdict and the exact features
  and face count that differ (any other difference in a listed case still fails):
  - `flanged_spool_132` under three of the turns (rust-correct): the part is six-fold symmetric
    about its axis, the turn is fixed from the off-axis repeats with six turns fitting equally
    (`fold` 6), and the identity-nearest one is a 30° turn rather than the motion's own, so the
    42 off-axis features and 72 faces are carried consistently onto their symmetric copies;
  - cgb202 (rust-wrong): sphere corner patches with a great-circle edge through the sphere's
    pole get a kernel area that changes several-fold with placement (face 240: 9.03 mm² unmoved,
    as OpenCascade, 61.75 translated), so they are orphaned, ambiguous or, under rot_zx_moved,
    matched to the wrong face and reported adapted; and B-spline quadrature as below;
  - cgb207, cgb242 (rust-wrong): B-spline face areas that change with placement by up to 0.7%;
  - threaded_connector_109 (rust-wrong): a surface recovered as a plane in one placement only
    (recover.rs);
  - nist_ctc_05 and cgb217 (rust-wrong): hole depths on a rounding half-way point.

  Families listed in `known_invariance.json` are left out where listed.
- **Revision pairs** (`tools/capture_revisions.py`, `tests/fixtures/revisions/`, 10 pairs): a hole
  moved (adapted, `position`), the boss resized (adapted, `sizes.diameter`), a hole added (new)
  and removed (orphaned), the boss removed (orphaned), the whole part turned and moved (all
  carried), the part moved with a hole moved (adapted), a row of holes grown from four to six (the
  pattern adapted with `sizes.count`, two holes new), a fillet radius changed (adapted), and the
  plate thickened (unaligned; distinct holes and a 20 mm row adapted with `sizes.depth`, a 5 mm
  row ambiguous). Every other feature is checked carried, each carried feature's faces (patterns
  aside) carried onto its partner's, and every face of the whole-part motion carried. All checks
  pass, with no listed differences.

### Not done

- The JSON-lines and MCP surfaces: quiddity-rust has neither yet; the library and CLI carry it.
- Mirrored revisions: the alignment is proper only (as the reader refuses reflections), so a
  mirrored part is not aligned.
- Arbitrary rotations of a part: recognition itself is not yet invariant to them (the frame
  normalisation is not ported), so the pairs and the invariance test use right-angle turns.
- The overlap test is estimated from fingerprints rather than computed from the faces'
  boundaries.
- The cut-off has not been stressed by a revision where a removed and an added feature of one
  family compete; such a pair may be reported adapted (moved and resized) rather than orphaned and
  new.
