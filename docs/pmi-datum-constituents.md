# Datum constituent identity

A plain child shape aspect aliases its datum feature only when its sorted,
deduplicated content equals the datum feature's whole content. A proper subset
registers its own feature from the memoised content when a requirement or
presentation names it. Unnamed support aspects remain absorbed, preserving
existing read/write round trips. The parent retains the union of all faces. Group datums
and all-around groups retain their existing handling. Legacy reader binding: /14.

## Structural regression

`pmi_import_repairs::multi_constituent_datum_keeps_three_size_features` adds
synthetic PMI to the existing small bolt fixture: three plain face aspects,
three independent Ø19 sizes, one composed datum feature. The supplied
`samples/bearing-lm10uu.step` is FreeCAD geometry without Specify's PMI, so a
trimmed reproduction of that writer output was unavailable. The synthetic test
uses datum Z because the base geometry fixture already carries datums A and B.
It asserts distinct size features, each exact face, and the complete parent union.
The baseline fails by giving a land all three union faces.

The original single-child regression now gives the parent and child the same
face set (including a duplicate direct usage, testing deduplication). Its previous
extra parent face actually made it a subset case; a separate regression now
checks that case keeps the tolerance on the child. Other checks cover presentation-only
constituents, child-before-parent resolution without duplicate registration,
all-around groups and shared per-member group carriers.

## STC10: all seven folds remain valid

Input: `tests/fixtures/ap242/nist/nist_stc_10_asme1_ap242-e2.stp.gz`.
Each parent below has exactly one composing face aspect, no direct face usages,
and one other SAR establishing its theoretical `DATUM`. That latter relationship
is not face composition. The child's GISU identifies the sole face of the parent.

| Datum | Parent | Child | Composition SAR | GISU | Whole face set |
| --- | ---: | ---: | ---: | ---: | ---: |
| A | #113 | #2857 | #588 | #2736 | {#6993} |
| B | #115 | #2858 | #600 | #2737 | {#7230} |
| C | #114 | #2856 | #598 | #2735 | {#7009} |
| E | #119 | #2905 | #707 | #2779 | {#7071} |
| F | #118 | #2903 | #691 | #2777 | {#7070} |
| G | #120 | #2915 | #709 | #2789 | {#7048} |
| H | #121 | #2923 | #759 | #2797 | {#7022} |

Thus **seven single-constituent folds are kept; zero multi-constituent folds
split**. STC10 remains at 166 features (the pre-#168 count was 173). The structural
regression asserts the count, each child/parent provenance alias, datum reference,
and exact sole face. No count expectation was relaxed.

## Validation and snapshots

Regenerated with the required command:

```sh
QUIDDITY_CORPUS=/Users/paul/repos/quiddity/tests/corpus QUIDDITY_CORPUS_REQUIRED=1 cargo run -p snapshot-tool --release --
```

Regenerated all 2,700 compressed snapshots from `47c387ba`. Exactly two tracked
files differ from `origin/main`:

- `tests/fixtures/rust-snapshot/pmi.json.gz`: 28 reader bindings (PMI and parts
  for each of 14 fixtures) advance /13 → /14. After normalizing only those strings,
  the entire decoded snapshot is equal to main: no feature, dimension, datum,
  tolerance, face, finding or other value changes on the existing inputs.
- `tests/fixtures/rust-snapshot/provenance.json`: records the new PMI snapshot
  digest and source commit. All input hashes and all 2,699 other snapshot hashes
  are unchanged.

FTC07 remains at 91 features, FTC10 at 146, STC09 at 125, and STC10 at 166.
The new synthetic regression supplies the missing case where requirements name
proper subsets of a multi-constituent datum.

`se_and_control_samples` passes when explicitly run with `--ignored`: the SE
sample retains 7 dimensions, 3 tolerances, 2 datums; callout #258 still targets
datum C's feature (FeatureId(1), incorporating child #222 and face #2502).
Complete `pmi read` JSON matches the baseline apart from the reader binding.
The control CTC01 fixture retains 12 dimensions, 6 tolerances, 3 datums.

`cargo fmt --all -- --check` and
`cargo clippy --workspace --all-targets -- -D warnings` pass. Full release tests
with the required corpus environment are recorded after completion.

## Retired instructions

Measured release `quiddity pmi read` on the largest corpus STEP by decoded bytes,
`cadgenbench_inputs/cgb202.step.gz` (17,927,439 bytes;
100 corpus STEP inputs surveyed). The six runs alternate pairs in order
main/head, head/main, main/head. The baseline binary was built from `origin/main`
(`bb0cd6d0`) before edits; the new binary contains the final reader fix
(`dac39603`). Darwin `proc_pid_rusage` reads
`RUSAGE_INFO_V4.ri_instructions` after child exit and before reaping; this avoids
`time -l`'s sandbox-blocked clock sysctl. No time-to-instructions estimate is used.

| Run pair | Main instructions | New instructions |
| --- | ---: | ---: |
| 1 | 40,258,403,096 | 40,237,363,479 |
| 2 (new first) | 40,228,407,709 | 40,236,611,983 |
| 3 | 40,232,519,750 | 40,233,028,535 |
| Median | 40,232,519,750 | 40,236,611,983 |

Median change: **+0.0102%**. Output is identical except the
expected reader binding. [Raw counters, binary/input hashes and platform](pmi-datum-constituents.instructions.json)
retain provenance and run order. This is a whole CLI read measurement, including
STEP decoding and serialization; this geometry input has no semantic PMI.

## Unnamed support aspects

A named constituent resolves to its own feature whenever its content is a proper
subset. Unnamed support aspects remain absorbed in the union, as before. This
matters when the existing writer factors a direct multi-face datum usage into
plain child aspects: read/write verification must not gain unused features.
The original CTC01 round-trip comparisons and the unreferenced-datum writer test
remain unchanged and strict; no writer change or difference filter is needed.
