# Derived PMI evidence, 2026-10-18

Baseline: `0cfe8356614bc16d1b8a4e7c7ae944c25710026c` (`v0.1.0-alpha.6`, reader
`haecceity-pmi-read/6`). The implementation uses PMI document format 2 and reader `/7`.
See [the rules, record format and draftwright migration](pmi-derived-geometry.md).
The revised surface-point and single-plane rules start from `c67ffa44`; the record format
and reader binding remain unchanged.

## GRM-03

The original supplied STEP file is committed as a deterministic gzip fixture, unmodified;
its source hash and test provenance are in the [fixture README](../tests/fixtures/ap242/derived/README.md).
Both readers retain **10/10 semantic dimensions**, including Maximum on #914. Drawable
geometry refusals are now **10 → 0** under the revised analytic rules: explicit points
on deriving surfaces agree with their axes, and a single planar deriving face supplies
its own plane. No STEP fixture bytes, nominal tokens, qualifiers or NIST baseline entries
were changed.

Linear tolerance in this file is `1e-7 mm`; the surface check adds only a floating-point
roundoff allowance below `2e-13 mm` at these coordinates. The regression independently
measures every resolved target and compares it with its stated nominal:

| Dimension | Derived aspect(s) | Geometry checked against nominal |
| --- | --- | --- |
| #841, diameter 4 | #835 | Face #573 axis radius 2 mm; explicit point #836 is one radius from the axis |
| #877, diameter 6 | #871 | Face #479 axis radius 3 mm; explicit point #872 is one radius from the axis |
| #914, diameter 10, Maximum | #907 | Face #193 axis radius 5 mm; explicit point #908 is one radius from the axis |
| #958, diameter 5 | #952 | Face #138 axis radius 2.5 mm; explicit point #953 is one radius from the axis |
| #994, diameter 1.6 | #988 | Face #663 axis radius 0.8 mm; explicit point #989 is one radius from the axis |
| #1040, length 3.2 | #1030 / #1034 | Single planes from faces #628 / #538; normal separation 3.2 mm |
| #1086, length 0.5 | #1076 / #1080 | Single planes from faces #538 / #391; normal separation 0.5 mm |
| #1132, length 2 | #1122 / #1126 | Single planes from faces #391 / #17; normal separation 2 mm |
| #1174, length 3 | #1164 / #1168 | Single planes from faces #17 / #248; normal separation 3 mm |
| #1216, length 20 | #1206 / #1210 | Single planes from faces #248 / #534; normal separation 20 mm |

The ten length-endpoint aspects reuse `median_plane` records, each with one deriving face
and that face's own plane; the five diameter aspects retain their face-derived axis,
radius and cross-checked supplemental point. The regression asserts all ten exact nominal
tokens and units, all qualifiers, all target links, plane parallelism, radius/diameter
agreement, plane separation, and zero derived-geometry or plane-distance refusals.

Synthetic tests retain the earlier successful and refusal paths: removing an explicit
usage, putting its point on the axis, two-face median planes, deriving-face bindings,
derivation chains, axis–axis / axis–plane / plane–plane targets, tolerance targets,
explicit point/line/placement/plane agreement, mixed units, context uncertainty,
explicit-only refusal, cycles and writer round-tripping. A point off both the axis and
the deriving cylinder surface still produces the existing conflict finding. Two valid
single-plane aspects with non-parallel normals retain their plane records but refuse the
linear distance with a dimension finding. Cone tests check changing radius with station
and angle-unit conversion; torus tests check inner/outer radii and the top of the tube.
Surface-tolerance boundary tests accept in-tolerance points and reject arbitrary offsets.
Pure tests still refuse non-coaxial cylinders, non-parallel deriving planes, mixed face
kinds, and more than two planar deriving faces. JSON validation accepts both single-face
planes and two-face medians and rejects three-face plane records.

## NIST before and after

The requested directory `/Users/paul/repos/quiddity/tests/corpus/nist` contains **10 AP203
geometry-only exports**, not semantic AP242 files. None contains a derived aspect,
dimension or geometric tolerance. All ten were read before and after; their complete
semantic output and findings are unchanged. Refused dimensions / geometric tolerances:
**0 / 0 → 0 / 0**.

The seven committed NIST AP242 fixtures were additionally audited:

| NIST AP242 file | Dimensions read, before → after | Geometric tolerances read, before → after | Refused roots, before → after |
| --- | --- | --- | --- |
| CTC-01 e1 | 12 → 12 | 6 → 6 | 0 → 0 |
| CTC-02 e2 | 7 → 7 | 22 → 22 | 0 → 0 |
| FTC-07 e2 | 17 → 17 | 27 → 27 | 0 → 0 |
| FTC-10 e2 | 26 → 26 | 40 → 40 | 0 → 0 |
| STC-06 e3 | 17 → 17 | 25 → 25 | 0 → 0 |
| STC-09 e4 | 20 → 20 | 27 → 27 | 0 → 0 |
| STC-10 e2 | 27 → 27 | 37 → 37 | 0 → 0 |
| **Total** | **126 → 126** | **184 → 184** | **0 → 0** |

None of these seven files contains a derived aspect either. “Refused roots” means stated
dimension or geometric-tolerance instances minus read items. Findings about other
constructs, or defects read through, are not counted as refused dimension/tolerance roots.
For completeness, finding counts also remain unchanged. The supplied ten AP203 files have
5 `not-modelled` and 10 `nonconformance` findings before and after. The seven semantic
AP242 fixtures have 23 `not-modelled`, 62 `nonconformance`, 3 `unresolved-reference` and
3 `unconsumed` findings before and after. These findings cover other constructs and
read-through defects; they do not represent additional refused dimension/tolerance roots.

The audit also checks exact semantic-output hashes for every field and every finding,
excluding only document format/binding versions; it therefore protects previously resolved
items beyond these counts. The before hashes and counts are committed in
[`nist-baseline.json`](../tests/fixtures/ap242/derived/nist-baseline.json).

## Checks

The release checks use `CARGO_BUILD_JOBS=8`, `RUST_TEST_THREADS=8` and
`QUIDDITY_CORPUS=/Users/paul/repos/quiddity/tests/corpus`, so the external ten NIST files
are included in the audit.

- `cargo fmt --check`: passed.
- `cargo clippy --all-targets --release -- -D warnings`: passed; the additional
  `--workspace` run also passed.
- `cargo test --release`: passed; 262 tests passed, zero failed.
- `cargo test --release -p haecceity`: passed; 246 tests passed, zero failed, 3 ignored.
- The targeted `derived_geometry_does_not_regress_any_nist_pmi` audit passed against
  all 17 unchanged baseline entries, including exact semantic-output and finding hashes.
- The derived-geometry coverage includes 3 pure derivation unit tests, 16 kernel
  integration tests and the JSON record/reference validation test.
- `cargo test --release --workspace`: passed; 508 tests passed, zero failed, 3 ignored
  across 62 targets (including doc-test targets). This final run also passed the NIST
  semantic-output audit and the reader-output version pin.

Ignored tests: `face_moment_evidence`, `dump` and `export_written_files` (the repository's
opt-in development aids). Every verification command completed with exit status 0.
