# The B-rep STEP writer

**Status:** implemented (2026-10-21), `haecceity::brep_write`.

haecceity reads STEP into a [`Part`]; `brep_write::write_step` writes a `Part` back out as a new
AP242 file, through step-io 0.3's `StepBuilder` (skeleton, products, units, solids, assembly
placements) and its authoring layer (topology and geometry the builder does not take: edge
`same_sense`, `SURFACE_CURVE`/`SEAM_CURVE` with pcurves, `VERTEX_LOOP` bounds, conics given by
their own frames, swept surfaces as the file stated them).

## When to use it

Maintainer decisions (2026-10-21):

- A file we were given keeps its bytes. PMI edits to it go through `pmi::write` (and
  `quiddity pmi write`), which changes nothing else in the file.
- The B-rep writer is for output that is ours: a transformed or repaired part, or a part we
  build output for.
- No new geometry modelling: it writes the B-rep haecceity holds; it adds no booleans, fillets
  or construction.

## API

```rust
use haecceity::brep_write::{WriteOptions, write_step};

let part = haecceity::read_step(&bytes)?;
let (step, report) = write_step(&part, &WriteOptions { name: "bracket", pmi: None })?;
```

`WriteOptions::pmi` takes the part's semantic PMI (`pmi::PartPmi`, anchored to the part's faces
and edges by index, as `pmi::read` gives it for a part read with `step::read_part`). The command
line writes a file's part with `quiddity brep <file.step> -o <out.step>` and prints the report,
with the kernel's volume of each solid.

## What is written

- One product, in millimetres and radians. The length uncertainty is the source file's (kept by
  the reader, in millimetres), else 1e-7 mm.
- Every solid as a `MANIFOLD_SOLID_BREP`, or a `BREP_WITH_VOIDS` when its faces form several
  shells (the first face's shell is the outer one; the others are voids, written as authored).
  Faces in no solid go in one `OPEN_SHELL` of a `SHELL_BASED_SURFACE_MODEL` per product.
- Faces, edges and vertices in the part's order: each face's loops in the direction that makes
  the reader number edges as the part does (a loop the file used reversed is written reversed,
  `FACE_BOUND.orientation` false), face `same_sense` from its orientation, every edge with its
  `same_sense`, vertex loops at cone apexes and poles.
- Surfaces: planes, cylinders, cones, spheres and tori as themselves; B-spline (and Bézier)
  surfaces as their exact rational or plain B-spline; surfaces of revolution and linear
  extrusion as the file stated them (profile, axis, extrusion vector), so they read back as the
  same NURBS and keep their kind.
- Curves: lines, circles, ellipses, parabolas and hyperbolas as themselves; every other curve as
  its exact (rational) B-spline.
- Pcurves: the file's own, as it stated them, where their parameters are the written surface's
  (an analytic face from a millimetre/radian file; a B-spline, Bézier or swept face written with
  the file's parameterisation); otherwise the pcurves the part holds in full (2D lines, and
  B-splines with their knots).
- Exactness. The reader keeps, beside the geometry it evaluates, what writing it back exactly
  needs: frame and direction ratios as the file gave them (the reader normalises them), B-spline
  pcurves' knots and weights, swept surfaces' definitions, every face's pcurves, the uncertainty,
  and each placed instance's native geometry with its placement levels as the file stated them.
  A placed instance (an assembly's part) is written in its own frame in a component placed by
  the same levels, so composing the placement on reading gives the same bits. A part moved after
  reading (`Part::transformed`) is written where it lies; directions are then written as the
  nearest ratios that normalise back to the held ones.
- Deterministic: no time stamp; the same part and options give the same bytes.

## PMI

`WriteOptions::pmi` is written by `pmi::write` (mode add) into the new file, anchored to the
written faces and edges by index; that writer validates what it adds against the schema and its
rules. Not carried, each named in `PmiCarried::refused` with its reason:

- hole definitions (`PartPmi::holes`), which `pmi::write` neither writes nor refuses;
- every item `pmi::write` refuses (for example a dimension without a nominal value, or a
  validation property whose unit, a `derived_unit` or kilogram, the new file does not have), and
  every item that refers to one (`pmi::without`);
- when `pmi::write` fails otherwise (its rule check of what it would add), every item, naming
  that failure. On GRM-03 (a derived median plane sharing faces with another feature) it adds
  two `geometric_item_specific_usage`s for one face and representation, which
  `item_identified_representation_usage.UR1` rejects.

What is written reads back as `pmi::write::standard_form` of `PmiCarried::written`.

## What it refuses

`WriteError::Unsupported`, naming every face and edge, for geometry it cannot write faithfully:
a face whose surface did not resolve, an edge whose curve did not resolve (the part holds its
chord), a loop with neither edges nor a vertex. The report also states what reading the file back
cannot give again: freeform surfaces of another kind written as B-splines
(`surface_kinds_changed`: a swept surface from a file not in millimetres, an offset surface),
pcurves not written, whether face and edge order are preserved, and whether placed instances
were written where the part holds them (`placements_flattened`).

## The round-trip guarantee and its evidence

For a part read from a file, writing it and reading the result gives the same part: the same
faces, edges and solids in the same order, the same geometry bit for bit (so the same areas and
volumes), the same geometry after the same rigid motion, the same recognition records and
document (`quiddity <file.step>` output, its `source_sha256` aside), and no EXPRESS schema or
rule violation in the written file. `tests/brep_write.rs` checks this for every fixture part and,
with `QUIDDITY_CORPUS`, every corpus part, plus PMI over the AP242 fixtures.

2026-10-21, on step-io 0.3 (`cx/step-io-03`):

| Set | Files | Same | Refused | Unreadable input |
| --- | ---: | ---: | ---: | ---: |
| Corpus | 106 | 106 | 0 | 0 |
| Fixtures (top level and subdirectories) | 254 | 241 | 2 | 11 |
| Captured fixtures | 2585 | 2585 | 0 | 0 |

The two refusals are the broken fixtures whose geometry did not resolve
(`unresolved_open_curve.step`: edge 1; `unresolved_surface.step`: face 1). PMI: 45 parts of the
AP242 fixtures and corpus NIST files read back identical; 6 read back as written less items
named as refused (the three kinds above).

OpenCascade (`tools/check_brep_write_occt.py`, the read-only OCP environment): every written
file reads with the input's face and solid counts; volumes agree to 1e-6 except
`revolution_cone.step.gz`, where OpenCascade's volume of the written file equals the kernel's
(37.699111843) and its volume of the input does not (37.697639).

[`Part`]: ../crates/haecceity/src/brep.rs
