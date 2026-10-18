# Derived PMI geometry

Maintainer decision, 2026-10-18. `quiddity-pmi` format **2**, reader
`haecceity-pmi-read/7`, adds `PartPmi.derived_geometry` and the optional
`features[i].derived.geometry` reference. Version 1 documents must be regenerated from
STEP; the strict decoder rejects them. Other semantic PMI fields keep their meanings.
The shared CLI format-version constant also advances `quiddity-parts` and write reports to 2.

## Derivation

For a plain `DERIVED_SHAPE_ASPECT`, follow `SHAPE_ASPECT_DERIVING_RELATIONSHIP`
from its `relating_shape_aspect` to its `related_shape_aspect`, through chains and feature
groups, to distinct part faces. Cycles, missing references, edges, supplemental-only bases,
unsupported surfaces and refused intermediate derivations cannot establish geometry.
Specialized Table 3 subtypes retain their previous interpretation.

- One or more cylindrical, conical, toroidal or `SURFACE_OF_REVOLUTION` faces sharing one
  axis produce an **axis**. The cylinder radius is available only if every deriving face
  belongs to the same cylinder (coincident axes and equal radii within tolerance).
- Exactly two parallel planar faces produce their **median plane**, halfway between the
  supporting planes. Face orientation and normal sign do not change the result.
- Other combinations retain their semantic derivation with no geometry reference and a
  finding explaining the refusal. A single planar face does not establish a median plane.

Coordinates are computed from each deriving face's representation in its stated length
unit and converted to millimetres in the part's local coordinate system. These computed
values are decimal strings, rather than exact source measurement tokens. A supplied
supplemental point never selects the computed axis point or the median plane's position.
Analytic trimmed-surface wrappers are followed; freeform surfaces are not fitted.

Linear comparisons use the maximum of **1e-7 mm** and the positive length uncertainties
stated in the relevant representation contexts. Parallelism compares the magnitude of the
cross product of normalized directions against **1e-9** (sign-independent sine tolerance).

Explicit items identified through the aspect's usages must belong to their
`CONSTRUCTIVE_GEOMETRY_REPRESENTATION`. Their own context supplies their length unit.
All explicit items must pass:

- Cartesian point: lies on the derived axis or median plane.
- Axis placement (1D or 3D): origin lies on the result; axis agrees with the derived axis
  direction or median-plane normal. An omitted axis defaults to Z per STEP.
- Line: coincident with an axis, or contained in a median plane.
- Plane: coincident with a median plane; incompatible with an axis.

Unsupported or malformed explicit items refuse the geometry. A disagreement produces a
`conflict` finding identifying the aspect, usage, explicit item and deriving faces, with
the measured distance and tolerance or the direction disagreement. Other failures produce
`not-modelled` findings (missing derivation relationships remain a nonconformance).
Explicit geometry without deriving faces never establishes a derived feature.

## Record format and consumer migration

Within a part's `pmi` object, a derived feature may now be:

```json
{"derived":{"kind":"derived_shape_aspect","from":[0],"geometry":0}}
```

`from` still indexes `pmi.features`. `geometry` indexes the new `pmi.derived_geometry`
list, **not** the existing supplemental `pmi.geometry` list. A record is:

```json
{
  "kind": "axis",
  "deriving_faces": [3],
  "point": [
    {"value":"-3.2","unit":"mm"},
    {"value":"0","unit":"mm"},
    {"value":"0","unit":"mm"}
  ],
  "direction": ["1","0","0"],
  "radius": {"value":"2","unit":"mm"},
  "supplemental": [0]
}
```

| Field | Meaning |
| --- | --- |
| `kind` | `axis` or `median_plane` |
| `deriving_faces` | Sorted, unique face indices, starting at zero, in `step::read_part_definitions` numbering; map through `PartDefinition.faces` for STEP instance ids |
| `point` | Three length values: a point on the infinite line or median plane |
| `direction` | Three decimal strings: unit axis direction or plane normal; either sign describes the same geometry |
| `radius` | Optional cylinder radius; absent for a median plane, cones, tori, general revolution surfaces, or several cylinders of different radii |
| `supplemental` | Optional indices into `pmi.geometry`; explicit items that passed the cross-check, held by value with their own units |

As elsewhere, optional fields and empty lists are omitted. Missing `derived.geometry`
means geometry is unresolved: use the findings, and do not turn the deriving faces into a
surrogate target. The reader still retains the semantic dimension, nominal, qualifier and
tolerance data. This follows the prior reader's policy of retaining derivation relationships
while reporting geometry it cannot resolve.

**draftwright-rust must** accept document version 2 and reader binding `/7`, follow
`dimensions[i].kind.size.feature`, `dimensions[i].kind.location.from` / `to`, or
`tolerances[i].target.feature` into `features`, then follow `derived.geometry`. A tolerance
on a dimension or relation uses the existing target links first. Use `point` and `direction`
as an infinite line or plane according to `kind`; use the two resolved records for
axis–axis, axis–plane and plane–plane location dimensions. Use `radius` for cylindrical
size targets, and `deriving_faces` to bind drawing support to the part's faces. Diameter
without a cylinder radius stays unresolved and has a finding. Keep displaying the stated
nominal and `dimensions[i].qualifier` (including `"maximum"`), rather than substituting a
computed measurement. Apply the ordinary part/occurrence placement when projecting these
local coordinates. `supplemental` is cross-check evidence, not the drawable target.

The STEP writer continues to emit deriving relationships, and reuses or emits the
cross-checked supplemental items with their usages. Computed geometry is regenerated from
the source faces on read-back; semantic verification includes it.
