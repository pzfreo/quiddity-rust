# Datum constituent identity

A plain child shape aspect aliases its datum feature only when its sorted,
deduplicated content equals the datum feature's whole content. A proper subset
registers its own feature from the memoised content, including when only
presentation names it. The parent retains the union of all faces. Group datums
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
checks that case keeps the tolerance on the child. Other checks cover unreferenced
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

Final validation and interleaved instruction measurements are recorded below
once the complete snapshot regeneration and release checks finish.
