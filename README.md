# quiddity-rust

A pure-Rust port of [quiddity](https://github.com/pzfreo/quiddity): deterministic,
geometry-only feature recognition for STEP B-Rep. No OpenCascade: STEP is read with
[`step-io`](https://crates.io/crates/step-io) and everything the Python implementation asks of
OpenCascade is reimplemented in `src/kernel`.

**Status: prototype.** Ported so far, each checked against Python call by call:

| Family | Python entry point | Captured test calls | Corpus |
|---|---|---|---|
| Fillets | `recognise_fillets` | 108/109 match (1 known divergence) | see below |
| Holes | `recognise_holes` | 211/214 match (3 known divergences) | see below |
| Chamfers | `recognise_chamfers` | 107/107 | see below |
| Countersinks | `recognise_countersinks` | 77/77 | see below |
| Angled steps | `recognise_angled_steps` | 105/105 | see below |
| Flats | `recognise_flats` | 88/88 | see below |
| Paired ramp steps | `recognise_paired_ramp_steps` | 109/109 | see below |
| Bosses | `recognise_bosses` | 133/133 | see below |
| Hole patterns | `recognise_hole_patterns` | 470/470 | see below |
| Oriented chamfers | `recognise_oriented_chamfers` | 15/15 | see below |
| Face levels | `recognise_face_levels` | 36/36 | — |
| Risers | `recognise_risers` | 66/66 | — |
| Circular face patterns | `recognise_circular_face_patterns` | 5/5 | see below |
| Thin-wall bodies | `recognise_thin_wall_bodies` | 10/14 (4 known divergences) | see below |
| Interior voids | `recognise_interior_voids` | 6/6 | see below |
| Through steps | `recognise_through_steps` | 141/141 | see below |
| Oblique through steps | `recognise_oblique_through_steps` | 10/10 | see below |
| Circular blind steps | `recognise_circular_blind_steps` | 36/36 | see below |
| Turned steps | `recognise_turned_steps` | 286/287 (1 known divergence) | see below |
| Round-bottom blind slots | `recognise_round_bottom_blind_slots` | 25/27 (2 known divergences) | see below |
| Rectangular blind slots | `recognise_rectangular_blind_slots` | 21/23 (2 known divergences) | see below |
| Gusset ribs | `recognise_gusset_ribs` | 26/26 | see below |
| Gusset rib patterns | `recognise_gusset_rib_patterns` | 18/18 | see below |
| Grooves | `recognise_grooves` | 71/72 (1 known divergence) | see below |
| Plates | `recognise_plates` | 175/178 (3 known divergences) | see below |

Known divergences are listed, with reasons, in `tests/fixtures/captured/known_divergences.json`
(captured calls) and `tests/fixtures/known_divergences.json` (corpus).

The kernel also answers the questions the unported families ask of OpenCascade's booleans,
checked against every one Python asks over the corpus: the volume a probe shares with a solid
(`kernel/volume.rs`, `tests/probes.rs`) and whether faces cover a face (`kernel/cover.rs`,
`tests/patches.rs`). Faces stored as B-splines that are exactly planes, cylinders, cones or
spheres are recovered as such (`kernel/recover.rs`), as Python's `_effective_surfaces` does.

For draftwright-rust, the kernel draws a part's views without OpenCascade's hidden-line
algorithm or booleans (`kernel/hlr.rs`): every visible and hidden edge and silhouette, and a
section view's kept half and cut outline. `crates/haecceity/tests/drawings.rs` checks them
against OpenCascade's drawings of the corpus: 85 parts in four views and 98 sections. The cut
outlines agree on 95 of 99 parts, where the other four are OpenCascade boolean failures. The
views that differ by more than 1e-3 mm are listed in `tests/fixtures/known_drawings.json`.
Most of them agree within 0.03 mm, where OpenCascade's hidden-line output approximates
projected curves.

## Layout

```
crates/haecceity/    the geometry kernel (what OpenCascade is to the Python code), its own
  src/               crate so draftwright-rust can share it; quiddity re-exports it as
                     `quiddity::kernel`
    step.rs          STEP → Part: assemblies flattened with placements, units, voids,
                     reachable surface models, vertex loops, pcurve control polygons
    brep.rs          Part: faces, loops, edges, solids in OpenCascade's traversal order;
                     cached neighbours, validity, face bounds, extents along any direction
    geom.rs          vectors, frames, Bounds, analytic surfaces and curves, Python rounding
    nurbs.rs         rational B-spline curves/surfaces: evaluation, derivatives, inversion,
                     ray intersection
    sampling.rs      edges as polylines (adaptive, 0.2 µm chord tolerance)
    uv.rs            faces in parameter space: unwrapped loops, singular points, OpenCascade-
                     compatible UV ranges (BRepTools::UVBounds), point containment
    cloud.rs         faces as point clouds (mesh-like density on the exact geometry), k-d tree
                     nearest neighbours, exact point-to-face distance
    cover.rs         whether faces cover a face (covered_patch), in the face's parameter space
    classify.rs      point-in-solid (whole part or one solid) by the parity of rays.rs
                     crossings, with on-boundary detection (BRepClass3d)
    rays.rs          every hit of a ray on a solid's trimmed faces, through a box hierarchy
                     (IntCurvesFace_ShapeIntersector); cracks the file leaves between faces
    poly.rs          real polynomial roots by isolation (exact ray-torus hits)
    recover.rs       planes, cylinders, cones and spheres stored as B-splines, fitted and
                     certified by their largest deviation (ShapeAnalysis_CanonicalRecognition)
    sweep.rs         a planar face swept into a prism solid, the probe shape the recognisers
                     build with Solid.extrude (BRepPrimAPI_MakePrism)
    volume.rs        the volume a probe (box, solid, or a planar region swept along an axis)
                     shares with a solid, without a boolean: exact ray lengths integrated
                     between the corners of the solid ∩ probe arrangement
    mass.rs          exact solid volume and area: Green's theorem along the exact edges,
                     Gauss-Legendre quadrature (BRepGProp)
    hlr.rs           hidden-line projection and section views (HLRBRep, draftwright's
                     section A–A): edges and traced silhouettes cut where their projections
                     cross, each piece's visibility by one ray; section contours traced
                     across the faces
    py.rs            Python's numeric semantics: fsum, compensated sum, hypot, %, rounding,
                     tuple ordering — wherever results must agree to the bit
src/
  features/          the recognisers, one module per family, plus what they share
    context.rs       Context: one run over one part; box, classifier, cylinder inventory
                     computed once, on first use
    evidence.rs      Occurrence<R> (record + defining faces) and the valid-solid check
    body.rs          body keys: a solid's box, volume and area, unique within the part
    cylinders.rs     the cylinder inventory, runs, segments, coaxial keys
    planes.rs        nearest axis-aligned neighbour planes; a face's effective plane (native,
                     or a B-spline certified as one)
    bevel.rs         the single-face bevel read and the convex-corner probe
    probes.rs        the five fixed interior probes the whole-body families sample faces at
    turned.rs        what turned-stock treatments share: coaxial external cylinders, cone rims,
                     the turned-profile key steps and grooves publish
    stacks.rs        coaxial segments read at their ends (open / flat / drill point)
    regions.rs       logical plane/cylinder face regions, their boundary wire and its runs,
                     the stock face a recess opens through, and the empty-sweep probe
    fillets.rs  chamfers.rs  holes.rs  countersinks.rs  hole_patterns.rs  bosses.rs  angled_steps.rs  flats.rs
    paired_ramp_steps.rs  oriented_chamfers.rs  levels.rs  circular_face_patterns.rs
    thin_walls.rs  interior_voids.rs  through_steps.rs
    oblique_through_steps.rs  circular_blind_steps.rs
    turned_steps.rs  round_bottom_slots.rs  rectangular_blind_slots.rs  gussets.rs
    grooves.rs  plates.rs
    volume_probe.rs  axis-aligned prism probes (`prism_is_empty`) over kernel/volume.rs
  correspondence/    revision matching (docs/correspondence.md): fingerprint.rs (features and
                     faces), align.rs (rigid alignment), assign.rs (Hungarian with an
                     unmatched option), faces.rs (seeded propagation), mod.rs (`correspond`)
  bin/quiddity.rs    `quiddity part.step` → JSON with fingerprints;
                     `quiddity correspond old new` → the correspondence as JSON
tests/
  captured.rs        replays every recogniser call the Python test suite makes
  corpus.rs          every ported recogniser over the shared 100-file STEP corpus
  evidence.rs        fillet defining faces and evidence refusals on hand-built cases
  invariance.rs      every corpus part moved and turned: each family must find the same faces
  correspondence.rs  every corpus part corresponds with itself moved, everything carried; the
                     build123d revision pairs get their expected classes
  fixtures/          STEP parts and Python's recorded answers, shared by both crates
crates/haecceity/tests/
  patches.rs         every covered_patch question Python asks over the corpus, answered by
                     cover.rs (tools/capture_patches.py records them)
  probes.rs          every volume probe Python's recognisers ask over the corpus, answered by
                     volume.rs (tools/capture_probes.py records them)
  kernel.rs          kernel behaviour on real parts
  drawings.rs        projections and section views against OpenCascade's drawings
                     (tools/capture_hlr.py, tools/capture_section.py record them)
crates/haecceity/examples/
  hlr_compare.rs     the drawings comparison, every score printed, with listings of the
                     curves either side draws differently
tools/
  capture_probes.py  records every volume probe Python asks over the corpus
  capture_patches.py records every covered_patch question Python asks over the corpus
  capture_hlr.py     records OpenCascade's hidden-line projection of every corpus part
  capture_section.py records draftwright's section view of every corpus part
  capture_plugin.py  pytest plugin that records the Python suite's recogniser calls
  export_corpus.py   records Python's answers over the corpus
  export_fixtures.py hand-built fillet evidence cases
  capture_revisions.py builds the revision pairs in build123d, with their expected classes
```

## Running

```
cargo build --release
./target/release/quiddity part.step > part.json            # records and fingerprints
./target/release/quiddity correspond old.json new.json      # or two STEP files
cargo test --workspace --release   # corpus tests need ../quiddity/tests/corpus or QUIDDITY_CORPUS
```

## How a family is ported

1. **Capture Python's behaviour.** From the quiddity checkout, run its tests for the family
   with the capture plugin (add the function to `QUIDDITY_CAPTURE`):

   ```
   PYTHONPATH=../quiddity-rust/tools \
   QUIDDITY_CAPTURE=recognise_fillets,recognise_holes,recognise_hole_patterns,recognise_countersinks,<new> \
   QUIDDITY_CAPTURE_OUT=../quiddity-rust/tests/fixtures/captured \
   .venv/bin/python -m pytest -p capture_plugin -p no:cacheprovider <test files>
   ```

   Every call whose arguments are a part plus plain options (or records) is stored with the
   part as gzipped STEP and Python's answer on the re-imported STEP. Add the family to
   `tools/export_corpus.py` and rerun it.
2. **Port the module** into `src/features/<family>.rs`: a public `recognise_<family>(part,
   opts)` returning records that serialise to the same JSON as the Python dataclass, and a
   `discover(ctx, …) -> Vec<Occurrence<Record>>` that takes the shared `Context`. Reuse
   `Context` for anything a second family will also need (put it there rather than recomputing).
3. **Wire it in**: the module in `src/features/mod.rs` (and, for a family with occurrences,
   a `Features` field filled in `recognise`), the entry point re-exported from `src/lib.rs`,
   match arms in `tests/common/mod.rs` (`recognise`, and `defining` for an evidence path), a
   `#[test]` in `tests/captured.rs`, and a row in the table above.
4. **Explain every difference** that remains in the relevant `known_divergences.json`; the
   tests fail on any unexplained difference and on any listed one that has disappeared.

Float comparison: values must agree to one part in a million (sub-micron at part scale);
Python-rounded fields agree exactly. Below that the kernels' parameter-range arithmetic
differs in its last digits.

## Beyond parity

Matching Python is a stepping stone; the goal is the right answer. Every listed difference
(`known_divergences.json` in both fixture directories, and `known_invariance.json`) carries a
`verdict`: `rust-correct` (the port has the true geometry, and the reason gives the evidence),
`rust-wrong` (a port limitation), `equivalent` (one geometry, two representations),
`undetermined`, or `not-applicable`. The tests refuse an entry without one.

`tests/invariance.rs` checks what Python cannot vouch for: every corpus part is re-read moved by
a translation and right-angle rotations (`read_step_placed`), and each family must find the same
features on the same faces. Arbitrary rotations are left to the frame normalisation Python does
before recognising (not yet ported); reflections are refused by the reader for now, which rebuilds
frames right-handed.

## What parity means

Python's *specified* numeric behaviour is matched exactly: rounding grids, summation order,
tie-breaking, thresholds (`kernel/py.rs`). OpenCascade's *accidents* are not reproduced: where
it approximates (pcurves it builds on import, a fixed Gauss order on B-spline faces), heals a
file, or decides a degenerate case by round-off, the port computes the true geometry and the
difference is recorded with its reason. Some older emulation remains where removing it would
cost many records (face ranges boxed by B-spline pcurve poles, which hole depths follow).

Circular face patterns compare sampled surfaces, as Python compares OpenCascade meshes. The port
samples the exact geometry as densely as such a mesh, so congruent copies sample congruently and
an exact repeat measures 0, as in Python; on parts whose copies are parameterised differently
`fit_error` is each side's own sampling noise. Whether a face near the axis turns onto itself
(and so is not part of the pattern) is decided exactly: Python's mesh test answers by whether
its point counts happen to divide by the pattern count.

The corpus test also checks kernel answers directly against OpenCascade: every face's
`BRepTools::UVBounds`, the arc between every pair of neighbours, and every solid's volume and
area (to 1e-9; the files that differ all have B-spline geometry, and each records its worst
difference as a power of ten).

## Notes on matching OpenCascade

OpenCascade silently heals files on import, and Python's answers depend on it. Things the
reader reproduces:

- `FACE_BOUND` orientation flags are not trusted: OpenCascade writes `.F.` on some toroidal
  faces whose edges are already listed in the face's sense and fixes the result on reading.
  Face containment therefore uses orientation-free crossing parity; orientation only breaks
  genuine ties (which way round a sphere's pole a boundary goes, and which side of its two
  loops a seamless torus band lies — files written by OpenCascade always have the seam).
- A placement without a reference direction takes `gp_Ax2`'s default x axis.
- Face UV ranges include the control polygons of B-spline pcurves that span their edge,
  because OpenCascade boxes pcurves by their poles.
- Closed edges (full circles) are exempt from the orientability check: their recorded
  direction is not evidence.

Known kernel limitations: OpenCascade's healing adds missing seam edges to periodic faces (and splits closed edges they cross), which the reader does not; sphere patches that pass through a pole have approximate interior
bounding boxes; closed surfaces of revolution/extrusion in NURBS form are treated as
non-periodic.

Rays meet a face where they cross it inside its trim, or within the band by which one of its
edges strays from its surface: B-spline faces exported as approximations of their neighbours
leave cracks up to a few tenths of a millimetre wide, which OpenCascade covers with the edge
tolerances it sets on import. A hit found only in that band gives way to the face across the
crack. A sphere bounded only by a vertex loop at a pole is the whole sphere.
