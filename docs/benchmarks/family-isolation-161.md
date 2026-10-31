# Family isolation instruction counts (For #161)

Baseline: local `origin/main`, `ea72ab24a6c8b43b80bf5ad021747caac04bdb18`.
Change: implementation in `1af9a7dc` (later additions only strengthen tests/reporting).
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
| `mfcadpp/1004.step` | 566,675,135 | 566,845,691 | +0.0301% |
| `cadgenbench/threaded_connector_109.step` | 58,307,979,393 | 58,309,186,067 | +0.0021% |
| `cadgenbench/flanged_spool_132.step` | 12,029,794,266 | 12,032,538,135 | +0.0228% |

All median increases are below 0.031% of the full CLI instruction count. This
bounds observed end-to-end overhead on these inputs; it does not establish zero
instruction cost for `catch_unwind` or replace a family microbenchmark.

Raw paired instruction counts (columns remain base/head regardless of execution order):

| Part | Pair | Base | Change |
| --- | ---: | ---: | ---: |
| `mfcadpp/1004.step` | 1 | 566,675,135 | 567,020,877 |
| `mfcadpp/1004.step` | 2 | 566,534,502 | 567,435,447 |
| `mfcadpp/1004.step` | 3 | 566,742,820 | 565,513,540 |
| `mfcadpp/1004.step` | 4 | 566,653,396 | 566,845,691 |
| `mfcadpp/1004.step` | 5 | 566,678,900 | 566,271,061 |
| `cadgenbench/threaded_connector_109.step` | 1 | 58,318,433,174 | 58,313,890,616 |
| `cadgenbench/threaded_connector_109.step` | 2 | 58,312,025,547 | 58,313,736,385 |
| `cadgenbench/threaded_connector_109.step` | 3 | 58,307,979,393 | 58,309,186,067 |
| `cadgenbench/threaded_connector_109.step` | 4 | 58,292,229,532 | 58,294,521,883 |
| `cadgenbench/threaded_connector_109.step` | 5 | 58,286,573,637 | 58,296,226,580 |
| `cadgenbench/flanged_spool_132.step` | 1 | 12,031,255,183 | 12,032,538,135 |
| `cadgenbench/flanged_spool_132.step` | 2 | 12,030,688,306 | 12,031,931,703 |
| `cadgenbench/flanged_spool_132.step` | 3 | 12,029,794,266 | 12,034,004,832 |
| `cadgenbench/flanged_spool_132.step` | 4 | 12,027,732,485 | 12,035,003,596 |
| `cadgenbench/flanged_spool_132.step` | 5 | 12,027,655,595 | 12,031,487,287 |

The corpus root was the pinned `scratchpad/corpus-pin/tests/corpus` specified in the task.
Test binaries were run sequentially throughout validation, including when load exceeded 19.
The implementation has no opt-in flag and no release-build fault injection.
