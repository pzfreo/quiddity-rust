# Python quiddity 0.4.0

`quiddity` is now an abi3 wheel built from Rust with PyO3 and maturin. It supports
CPython 3.12 and newer, has no Python runtime dependencies, and requires neither
build123d nor OCP. The Rust binding crate is `crates/quiddity-py` (`publish = false`
for crates.io); its Python distribution and import name are both `quiddity`.

```python
import json
import quiddity

text = quiddity.recognise("part.step")
result = json.loads(text)
document = result["document"]  # quiddity-rust/recognition/6
for feature in document["records"]:
    print(feature["family"], feature["parameters"], feature["faces"])

with open("part.step", "rb") as stream:
    evidence = json.loads(quiddity.evidence(stream.read()))
```

| API | Result (JSON string) |
| --- | --- |
| `recognise(step: str \| bytes)` | Exactly the Rust CLI recognition envelope: family arrays, `fingerprints`, and the `/6` `document`, including stdout's trailing newline |
| `evidence(step, framed=False)` | [Evidence/1](evidence.md): family, record, defining/constituent faces, instance groups, hosts, pattern members and rejected candidates |
| `anchor(step, face: int)` | `{face, point: [x,y,z], uv: [u,v]}` from haecceity's proved trimmed-face anchor, in file coordinates |
| `fingerprints(step)` | The existing fingerprints/3 document |
| `correspond(old_step, new_step)` | The existing Rust revision correspondence document |
| `capabilities()` | Versioned supported formats, inputs, operations and PMI import availability |
| `__version__` | `"0.4.0"` (a Python string) |

Every geometry argument accepts a string path or STEP bytes. A string is always a
path, never inline STEP text. Path inputs also accept the existing `.step.gz` /
`.stp.gz` reader. Byte inputs are uncompressed STEP. File and byte recognition of
the same uncompressed input have identical source digests and JSON. Geometry
work releases the GIL. Rust input/refusal messages become `ValueError`; wrong input
types raise `TypeError`, and negative face indices raise `OverflowError`.

`from quiddity import pmi` provides `pmi.read(step, part=None)` →
`haecceity.pmi/1` JSON and `pmi.write(step, document, output, part=None)` → write
report JSON. These optional convenience operations use paths, and the wheel
includes haecceity `json-import`. Multi-part files require an explicit part.
Writes use the CLI's verified importer and atomic output replacement. Exported
JSON is bound to its source geometry; supply it to that geometry's importer.
Reading does not promise that every source item is writable: the importer retains
haecceity's explicit refusals, including dimensions without nominal values.

`quiddity ...` and `python -m quiddity ...` execute the Rust CLI dispatcher,
including `serve`, `parts`, `correspond`, `brep`, `pmi` and `pmi json` commands.
New commands include `evidence <step> [--framed]`, `fingerprints <step>` and
`capabilities`. Run `quiddity --help` for the current syntax.

## Breaking changes from Python quiddity 0.3.11

- Live build123d/OCP inputs, per-family `recognise_*` functions, and exported
  dataclasses are removed. Export to STEP and call the file-based API.
- Results are JSON strings (decode with `json.loads`), replacing dataclasses,
  `RecognitionResult`, and format-3 dictionaries. Recognition/6 is inside the
  CLI envelope's `document` field.
- Recognition coordinates are in file space, rather than a local working frame.
  Axes can be unit vectors as well as principal-axis names. A record's own
  origin/basis fields define its measurements. The top-level frame is provenance.
  Explicitly framed evidence is the exception described in [evidence/1](evidence.md).
- Recognition/6 merges oriented families into plain ones (including slots,
  chamfers and slot patterns), publishes hole patterns as records, and adds
  families such as modelled threads and turned bodies. `derived`, `association`
  and `named_dimensions` are removed. See [recognition/6](recognition-6.md).
- Fingerprints/3 and `correspond` replace correspondence receipts and their
  run-local issuance/resolution API. See [correspondence](correspondence.md).
- Evidence and anchors identify faces by indices, replacing Face objects.
  OCP import, inspection and mapping helpers are removed.
- The CLI is the Rust CLI, with different subcommands and JSON output.
- Python 3.12 is the minimum supported version.

## Migrating build123d-mcp

Export the in-session shape once to STEP bytes (or a temporary STEP path), retain
those bytes for the run, then call `recognise` for inventory and `evidence` for
edit handles. `evidence(..., framed=False)` corresponds to the old caller-space
evidence; `framed=True` corresponds to part-relative evidence. Evidence preserves
`bottom`, `cbore`, `spotface`, `csink`, `diameter`, `axis`, `location`, and `depth`
in hole records. Preserve null stack fields and handle axis vectors.

Build targets from `evidence["features"]`: retain `family`, `index`, `record`,
`defining_faces` and `constituent_faces`, and resolve indices through a verified
STEP-to-live-face mapping. Keep pattern aggregates read-only as before. Replace
the old association report with the recognition document's `face_accounting`;
feature, body, refused and unexplained are distinct groups. Do not treat
unproved/refusal records as permission to edit. Retain the consumer's existing
valid-solid, plain-through-hole and stale-source checks, then re-export and
re-recognise after each edit. Use `correspond` for revision matching.

The face-order trial and reproducible procedure are in
[the trial report](python-face-order.md). Do not assume an exporter version's
observed ordering is a durable contract. The trial verified a complete geometric bijection using unique anchor membership,
area and centroid agreement. Apply those checks to each export before issuing
edit handles. Names can also be joined through `face_sources[face].entity` when
the writer supplies a complete name map, but the fillet-heavy trial retained
only 23 of 27 names. Never fill missing names by assumption or resolve an
ambiguous geometric match by nearest centroid.

## Migrating b123d-decompiler (document 3 → recognition/6)

| Python format-3 field | New field or action |
| --- | --- |
| `frame` | `result["document"]["frame"]`; provenance only; records already in file space |
| `features` | `result["document"]["records"]` |
| `features[].index` | Parse the family-local suffix of `records[].id` (`family/index`) |
| `features[].family` | `records[].family` (apply oriented-family merge migration) |
| `features[].record` | `records[].parameters` |
| `features[].constituent_faces` | Use the matching evidence feature after verifying family/record/defining faces; `records[].faces` are defining faces only |
| `derived.turned_profiles` | No direct field. Use `turned_bodies`, `turned_steps`, `cylindrical_profiles` and groove records to rebuild the consumer's machining profile |
| `association` | `document.face_accounting` (one group per face) and evidence faces; the old association graph has no direct replacement |
| `named_dimensions` / `dependents` | Reconstruct in the consumer if needed |

Remove the old `import_step_geometry` and `build_recognition_document` imports.
The consumer can import STEP itself for reconstruction, but must verify that
its imported face map matches the exact file the recogniser read. Stop applying
`to_local` to record coordinates. If the emitter requires a local working part,
transform both the part and every relevant record using the provenance frame,
including points, direction vectors and record-owned bases. Framed evidence can
help with legacy reconstruction, but its family registry differs from `/6`.

## Development and release gate

```sh
maturin build --release -m crates/quiddity-py/Cargo.toml
python -m pip install target/wheels/quiddity-0.4.0-*.whl
python -m pytest tests/python -q
# Optional: export QUIDDITY_CORPUS=/path/to/tests/corpus
```

`.github/workflows/python.yml` builds abi3 wheels for Linux x86_64/aarch64,
macOS arm64/x86_64 and Windows x64, plus an sdist. Each wheel runs the smoke
suite on CPython 3.12; Linux x86_64 also tests a newer CPython. A separate Linux
job installs the sdist and runs the same smoke suite. The corpus test
skips without `QUIDDITY_CORPUS` and fails if required corpus inputs are missing.
Publishing is possible only through `workflow_dispatch` with `publish=true` on
`main`, after all builds/tests pass, using the `pypi` environment and PyPI trusted
publishing. Pushes and tags never publish. The maintainer must configure the
trusted publisher/environment before using that gate.
