# quiddity-rust

A pure-Rust port of [quiddity](https://github.com/pzfreo/quiddity): deterministic,
geometry-only feature recognition for STEP B-Rep. No OpenCascade: STEP is read with
[`step-io`](https://crates.io/crates/step-io) and everything the Python implementation asks of
OpenCascade is reimplemented in `src/kernel`.

**Status: prototype.** Ported so far, each checked against Python call by call:

| Family | Python entry point | Captured test calls | Corpus |
|---|---|---|---|
| Fillets | `recognise_fillets` | 108/109 match (1 known divergence) | see below |
| Holes | `recognise_holes` | 205/214 match (9 known divergences) | see below |
| Chamfers | `recognise_chamfers` | 107/107 | see below |
| Countersinks | `recognise_countersinks` | 77/77 | see below |
| Hole patterns | `recognise_hole_patterns` | 470/470 | see below |

Known divergences are listed, with reasons, in `tests/fixtures/captured/known_divergences.json`
(captured calls) and `tests/fixtures/known_divergences.json` (corpus). The main gap is
**B-spline cylinder recovery**: Python recognises holes on cylinders stored as B-spline
surfaces through `_effective_surfaces.py`, which is not ported.

## Layout

```
src/
  kernel/            the geometry engine (what OpenCascade is to the Python code)
    step.rs          STEP → Part: assemblies flattened with placements, units, voids,
                     reachable surface models, vertex loops, pcurve control polygons
    brep.rs          Part: faces, loops, edges, solids in OpenCascade's traversal order;
                     cached neighbours, validity, face bounds
    geom.rs          vectors, frames, Bounds, analytic surfaces and curves, Python rounding
    nurbs.rs         rational B-spline curves/surfaces: evaluation, derivatives, inversion,
                     ray intersection
    sampling.rs      edges as polylines (adaptive, 0.2 µm chord tolerance)
    uv.rs            faces in parameter space: unwrapped loops, singular points, OpenCascade-
                     compatible UV ranges (BRepTools::UVBounds), point containment
    classify.rs      point-in-solid by ray parity, with on-boundary detection (BRepClass3d)
    py.rs            Python's numeric semantics: fsum, compensated sum, hypot, %, rounding,
                     tuple ordering — wherever results must agree to the bit
  features/          the recognisers, one module per family, plus what they share
    context.rs       Context: one run over one part; box, classifier, cylinder inventory
                     computed once, on first use
    evidence.rs      Occurrence<R> (record + defining faces) and the valid-solid check
    cylinders.rs     the cylinder inventory, runs, segments, coaxial keys
    planes.rs        nearest axis-aligned neighbour planes
    bevel.rs         the single-face bevel read and the convex-corner probe
    turned.rs        what turned-stock treatments share: coaxial external cylinders, cone rims
    fillets.rs  chamfers.rs  holes.rs  countersinks.rs  hole_patterns.rs
  bin/quiddity.rs    `quiddity part.step` → JSON
tests/
  captured.rs        replays every recogniser call the Python test suite makes
  corpus.rs          every ported recogniser over the shared 100-file STEP corpus
  evidence.rs        fillet defining faces and evidence refusals on hand-built cases
  kernel.rs          kernel behaviour on real parts
tools/
  capture_plugin.py  pytest plugin that records the Python suite's recogniser calls
  export_corpus.py   records Python's answers over the corpus
  export_fixtures.py hand-built fillet evidence cases
```

## Running

```
cargo build --release
./target/release/quiddity part.step
cargo test --release           # corpus test needs ../quiddity/tests/corpus or QUIDDITY_CORPUS
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
3. **Wire it into the tests**: one match arm in `tests/common/mod.rs::recognise`, one `#[test]`
   in `tests/captured.rs`.
4. **Explain every difference** that remains in the relevant `known_divergences.json`; the
   tests fail on any unexplained difference and on any listed one that has disappeared.

Float comparison: values must agree to one part in a million (sub-micron at part scale);
Python-rounded fields agree exactly. Below that the kernels' parameter-range arithmetic
differs in its last digits.

## Notes on matching OpenCascade

OpenCascade silently heals files on import, and Python's answers depend on it. Things the
reader reproduces:

- `FACE_BOUND` orientation flags are not trusted: OpenCascade writes `.F.` on some toroidal
  faces whose edges are already listed in the face's sense and fixes the result on reading.
  Face containment therefore uses orientation-free crossing parity; orientation only breaks
  genuine ties (which way round a sphere's pole a boundary goes).
- A placement without a reference direction takes `gp_Ax2`'s default x axis.
- Face UV ranges include the control polygons of B-spline pcurves that span their edge,
  because OpenCascade boxes pcurves by their poles.
- Closed edges (full circles) are exempt from the orientability check: their recorded
  direction is not evidence.

Known kernel limitations: OpenCascade's healing adds missing seam edges to periodic faces (and splits closed edges they cross), which the reader does not; sphere patches that pass through a pole have approximate interior
bounding boxes; closed surfaces of revolution/extrusion in NURBS form are treated as
non-periodic; torus ray intersection is sampled rather than solved in closed form.
