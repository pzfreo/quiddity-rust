# Graphic PMI presentation accounting

`haecceity::pmi::Accounting::presentation` counts graphic PMI instances, rather than every instance of the EXPRESS presentation family:

- `annotation_occurrence` and all schema subtypes, including annotation planes, placeholders and tessellated occurrences;
- `draughting_callout` and all schema subtypes;
- draughting models and presentation representations/views carrying those annotations, and their `model_geometric_view` records.

Each Part 21 instance counts once, including complex instances with several matching leaves. Models and views qualify through their representation items, mapped representations, styled-item wrappers and compound items. Cycles terminate. Shared representation contexts, geometry styles, colours, curve fonts, layers and invisibility do not qualify a model or view and are not counted themselves. Mapping and style wrappers are traversal links, not extra graphic PMI instances.

The broad EXPRESS presentation family remains unchanged: document removal needs that classification to preserve or remove styles safely. Only the PMI reader's accounting changes.

The reader identifier advances from `haecceity-pmi-read/7` to `/8`, following the convention that a change to `pmi::read` output changes the reader binding. The JSON form remains version 2. Package versions and CHANGELOGs are release work.

The ten NIST corpus files identify themselves as geometry-only. Positive graphic-PMI coverage uses the seven existing NIST AP242 fixtures, including CTC and FTC models. `crates/haecceity/tests/pmi_presentation.rs` checks both groups, as well as synthetic inheritance, content links and geometry-only styling.

## NIST corpus counts

Counts below compare the previous family-based rule with graphic PMI accounting. Paths are relative to `/Users/paul/repos/quiddity/tests/corpus/nist`.

| File | Before | After |
| --- | ---: | ---: |
| `nist_ctc_01_asme1_rd.stp` | 392 | 0 |
| `nist_ctc_02_asme1_rc.stp` | 35 | 0 |
| `nist_ctc_03_asme1_rc.stp` | 398 | 0 |
| `nist_ctc_04_asme1_rd.stp` | 106 | 0 |
| `nist_ctc_05_asme1_rd.stp` | 39 | 0 |
| `nist_ftc_06_asme1_rd.stp` | 416 | 0 |
| `nist_ftc_07_asme1_rd.stp` | 9 | 0 |
| `nist_ftc_08_asme1_rc.stp` | 18 | 0 |
| `nist_ftc_09_asme1_rd.stp` | 149 | 0 |
| `nist_ftc_10_asme1_rb.stp` | 33 | 0 |

## Existing NIST AP242 fixtures

Paths below are relative to `tests/fixtures/ap242/nist`. These retain their annotations while excluding styles and annotation scaffolding from the count.

| File | Before | After |
| --- | ---: | ---: |
| `nist_ctc_01_asme1_ap242-e1.stp.gz` | 843 | 71 |
| `nist_ctc_02_asme1_ap242-e2.stp.gz` | 3313 | 191 |
| `nist_ftc_07_asme1_ap242-e2.stp.gz` | 514 | 168 |
| `nist_ftc_10_asme1_ap242-e2.stp.gz` | 851 | 279 |
| `nist_stc_06_asme1_ap242-e3.stp.gz` | 1114 | 257 |
| `nist_stc_09_asme1_ap242-e4.stp.gz` | 559 | 179 |
| `nist_stc_10_asme1_ap242-e2.stp.gz` | 902 | 283 |
