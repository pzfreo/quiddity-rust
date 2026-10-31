# build123d face-order trial

Trial on 2026-10-31, macOS arm64, CPython 3.12.13, build123d 0.13.0,
`cadquery-ocp-novtk` 8.0.1.1.0, and the local quiddity 0.4.0 abi3 wheel.
The environment and exported STEP files were created in scratch space.
The [machine-readable result](python-face-order.json) records each index map.

| Shape | Faces | Standard `export_step` order versus `shape.faces()` | Named-face map |
| --- | ---: | --- | --- |
| Box minus a through cylinder | 7 | Exact match | Complete |
| Two coaxial cylinders fused, minus an axial bore | 6 | Exact match | Complete |
| Box with every edge filleted, minus a through cylinder | 27 | Exact match | 23/27; original faces 0, 7, 12, 24 had no transfer-map entity |

The standard-export test writes STEP to `BytesIO`, calls `recognise` and
`evidence`, checks equal face counts, then obtains each exported face's proved
anchor with `anchor`. It matches that point against every original trimmed
build123d face using OCCT point-to-face distance (1e-6 mm). Every anchor had
exactly one match, the matches formed a complete bijection, and every matched
face also agreed in area (1e-5 relative/absolute floor) and mass centroid
(1e-5 mm). Every index mapped to itself on all three shapes.

The name experiment uses `STEPControl_Writer`, looks up each original face with
`STEPConstruct.FindEntity_s`, names its transferred `ADVANCED_FACE`, and joins
those names through evidence's `face_sources[].entity`. Every name that was
available mapped to the expected original face and passed an anchor membership
check. The fillet-heavy model did **not** yield a complete name map. An additional
XCAF subshape-name experiment also retained only 23 of its 27 names. Face names
alone are therefore not a reliable universal mapping for this exporter/kernel
combination. The incomplete map is reported explicitly, never filled by order.

For build123d-mcp, these trials support using face order as a candidate mapping,
with a complete geometric verification on each export. Retain the original
`faces()` list and exact exported bytes, require equal counts and a bijection,
check unique anchor membership plus area/centroid agreement, and refuse edit
handles if any match is missing, ambiguous or inconsistent. A complete verified
name map is another option when available. Do not assume names survive export,
or choose the nearest centroid when geometric matches are ambiguous. These
three models do not establish a face-order guarantee for arbitrary models,
assemblies, future exporter versions, or topology-changing export repairs.

Reproduce with a throwaway venv containing build123d and the built wheel:

```sh
python tools/python_face_order_trial.py --output /tmp/face-order.json
```

The script constructs all three models itself; it is an optional migration
experiment and adds no build123d/OCP dependency to the wheel or smoke suite.
