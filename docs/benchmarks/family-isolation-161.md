# Family isolation instruction counts (For #161)

Baseline: local `origin/main`, `ea72ab24a6c8b43b80bf5ad021747caac04bdb18`.
Change: final implementation in `8c98a125` (including immediate post-pass recovery).
Both executables were built with `cargo build --release --bin quiddity`, default features,
the pinned Rust 1.99 toolchain, on the same Apple Silicon machine.

The measurement is the full CLI: STEP import, recognition, fingerprints, document
assembly and JSON serialization, with stdout discarded. Both binaries were warmed.
Five pairs per corpus part alternate base/head, head/base, base/head, head/base,
base/head. A small injected dylib destructor reads macOS
`proc_pid_rusage(getpid(), RUSAGE_INFO_V4, ...)` / `ri_instructions`.
No elapsed-time or CPU-time measurements were used. Successful stdout was also
compared byte-for-byte for all three parts before counting.

| Corpus part | origin/main median instructions | Change median instructions | Delta |
| --- | ---: | ---: | ---: |
| `mfcadpp/1004.step` | 566,854,390 | 565,551,266 | -0.2299% |
| `cadgenbench/threaded_connector_109.step` | 58,291,764,400 | 58,289,463,892 | -0.0039% |
| `cadgenbench/flanged_spool_132.step` | 12,023,200,531 | 12,026,389,044 | +0.0265% |

The largest observed median increase is below 0.027% of the full CLI instruction
count. The small negative changes are not evidence of a speedup. These
measurements bound observed end-to-end overhead on these inputs; they do not establish
zero instruction cost for `catch_unwind` or replace a family microbenchmark.

Raw paired instruction counts (columns remain base/head regardless of execution order):

| Part | Pair | Base | Change |
| --- | ---: | ---: | ---: |
| `mfcadpp/1004.step` | 1 | 566,854,390 | 565,228,701 |
| `mfcadpp/1004.step` | 2 | 566,649,783 | 565,388,808 |
| `mfcadpp/1004.step` | 3 | 566,797,802 | 566,005,789 |
| `mfcadpp/1004.step` | 4 | 566,969,478 | 566,083,360 |
| `mfcadpp/1004.step` | 5 | 566,950,868 | 565,551,266 |
| `cadgenbench/threaded_connector_109.step` | 1 | 58,292,764,353 | 58,290,036,019 |
| `cadgenbench/threaded_connector_109.step` | 2 | 58,293,891,299 | 58,290,676,409 |
| `cadgenbench/threaded_connector_109.step` | 3 | 58,291,764,400 | 58,289,463,892 |
| `cadgenbench/threaded_connector_109.step` | 4 | 58,289,148,112 | 58,288,974,629 |
| `cadgenbench/threaded_connector_109.step` | 5 | 58,291,222,464 | 58,283,874,791 |
| `cadgenbench/flanged_spool_132.step` | 1 | 12,022,995,224 | 12,025,189,688 |
| `cadgenbench/flanged_spool_132.step` | 2 | 12,021,332,569 | 12,026,779,714 |
| `cadgenbench/flanged_spool_132.step` | 3 | 12,023,200,531 | 12,026,389,044 |
| `cadgenbench/flanged_spool_132.step` | 4 | 12,026,553,864 | 12,028,330,362 |
| `cadgenbench/flanged_spool_132.step` | 5 | 12,023,870,630 | 12,025,970,822 |

The corpus root was the pinned `scratchpad/corpus-pin/tests/corpus` specified in the task.
Test binaries were run sequentially throughout validation, including when load exceeded 19.
The implementation has no opt-in flag and no release-build fault injection.


## Final validation

The final implementation (`8c98a125`) passed:

- `cargo fmt --all --check`.
- `cargo clippy --workspace --all-targets --release -- -D warnings`.
- Ten isolation unit tests in release mode with `json-import`, including every discovery
  boundary, the 16-face #805 island pocket and independent pads, countersink blockers,
  actual post-pass mutation rollback, rebuilding before the next post-pass, payloads,
  deterministic ordering, shared CLI/wheel/server metadata and scoped hook behavior.
- Root `cli` (19), `pmi_cli` (16) and `brep_write` (6) tests, with `json-import` and the
  pinned corpus. Four optional full B-rep sweeps retain their standard ignored annotations.
- A freshly rebuilt abi3 Python 3.11+ wheel: all 59 Python tests passed on CPython 3.12,
  with `QUIDDITY_CORPUS_REQUIRED=1`, the pinned corpus and native CLI comparisons enabled.
- `cargo run -p snapshot-tool --release -- --check`, with the specified pinned corpus,
  `QUIDDITY_CORPUS_REQUIRED=1`, and a clean committed source tree: all 2,700 compressed
  snapshots reproduced byte for byte. No snapshots or provenance were regenerated.

No required checks remain outstanding. All commits are local on `cx/q-isolate`;
nothing was pushed or posted.
