# Correspondence between revisions

**Status:** design, accepted with draftwright-rust's ADR R3 (2026-10-07). It is to be
implemented once a consumer carries feature edits (draftwright's spec, or specify-core's answers).

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

- **Feature:** family and subtype, its intrinsic sizes (diameter, depth, width, length, radius,
  angle, as the family has them), its axis or normal, its position in the part's frame, its
  pattern count and pitch, and the type of its parent face.
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

- **Costs:** for each family, a cost matrix over old × new. The costs compare subtype, intrinsic
  sizes, axis, pattern count and pitch, and parent-face type, and then position (once aligned).
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
