# Derived PMI evidence, 2026-10-18

Baseline: `0cfe8356614bc16d1b8a4e7c7ae944c25710026c` (`v0.1.0-alpha.6`, reader
`haecceity-pmi-read/6`). The implementation uses PMI document format 2 and reader `/7`.
See [the rules, record format and draftwright migration](pmi-derived-geometry.md).

## GRM-03

The original supplied STEP file is committed as a deterministic gzip fixture, unmodified;
its source hash and test provenance are in the [fixture README](../tests/fixtures/ap242/derived/README.md).
Both readers retain **10/10 semantic dimensions**, including Maximum on #914. Drawable
geometry refusals are **10 → 10**. Applying the agreed rules does not make this particular
input drawable: its diameter points disagree with the derived axes and its length
endpoints each derive from only one planar face. The new findings name those reasons
rather than reporting that supplemental geometry is not held.

Linear tolerance in this file is `1e-7 mm`. The original input's exact failures are:

| Dimension | Derived aspect(s) | Reason geometry is refused |
| --- | --- | --- |
| #841, diameter 4 | #835 | Explicit point #836 is 2 mm from the derived axis of face #573 |
| #877, diameter 6 | #871 | Explicit point #872 is 3 mm from the derived axis of face #479 |
| #914, diameter 10, Maximum | #907 | Explicit point #908 is 5 mm from the derived axis of face #193 |
| #958, diameter 5 | #952 | Explicit point #953 is 2.5 mm from the derived axis of face #138 |
| #994, diameter 1.6 | #988 | Explicit point #989 is 0.8 mm from the derived axis of face #663 |
| #1040, length 3.2 | #1030 / #1034 | One planar deriving face each (#628 / #538); median plane requires exactly two |
| #1086, length 0.5 | #1076 / #1080 | One planar deriving face each (#538 / #391); median plane requires exactly two |
| #1132, length 2 | #1122 / #1126 | One planar deriving face each (#391 / #17); median plane requires exactly two |
| #1174, length 3 | #1164 / #1168 | One planar deriving face each (#17 / #248); median plane requires exactly two |
| #1216, length 20 | #1206 / #1210 | One planar deriving face each (#248 / #534); median plane requires exactly two |

The diameter points are on cylinder surfaces. They cannot be treated as axis points under
the stipulated cross-check. Taking a single source plane as a derived plane would also
broaden the stipulated median-plane rule. Neither interpretation is silently applied.

`pmi_derived` asserts every original refusal and retains all nominal values and qualifiers.
Synthetic edits of the same fixture prove the successful paths: remove an explicit usage
or place its point on the source cylinder's axis, or derive a median plane from two
parallel faces. Axes, cylinder radii, median planes, deriving-face bindings, derivation
chains, axis–axis / axis–plane / plane–plane dimension targets and tolerance targets all
resolve. Tests also cover explicit point, line, axis placement and plane agreement and
disagreement, mixed units, context uncertainty, explicit-only refusal, cycles and writer
round-tripping. Pure unit tests cover coaxial / non-coaxial cylinders, parallel /
non-parallel planes and mixed face kinds.

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

- `cargo fmt --all --check`: passed.
- `cargo clippy --workspace --all-targets --release -- -D warnings`: passed.
- `cargo test --release --workspace`: passed; 504 tests passed,
  zero failed, 3 ignored across 62 test targets (including doc-test targets).
- The derived-geometry coverage includes 3 pure derivation unit tests, 12 kernel
  integration tests and the JSON record/reference validation test. The final workspace
  run also passed the 17-file NIST semantic-output audit and the reader-output version pin.

Ignored tests: `face_moment_evidence`, `dump` and `export_written_files` (the repository's
opt-in development aids). The requested command completed with exit status 0.
