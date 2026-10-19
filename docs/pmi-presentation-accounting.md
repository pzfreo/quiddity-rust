# Graphic PMI presentation accounting

`haecceity::pmi::Accounting::presentation` counts graphic PMI instances, rather than every instance of the EXPRESS presentation family:

- `annotation_occurrence` and all schema subtypes, including annotation planes, placeholders and tessellated occurrences;
- `draughting_callout` and all schema subtypes;
- draughting models and presentation representations/views carrying those annotations, and their `model_geometric_view` records.

Each Part 21 instance counts once, including complex instances with several matching leaves. Models and views qualify through their representation items, mapped representations, styled-item wrappers and compound items. Cycles terminate. Shared representation contexts, geometry styles, colours, curve fonts, layers and invisibility do not qualify a model or view and are not counted themselves. Mapping and style wrappers are traversal links, not extra graphic PMI instances.

The broad EXPRESS presentation family remains unchanged: document removal needs that classification to preserve or remove styles safely. Only the PMI reader's accounting changes.

The reader identifier advances from `haecceity-pmi-read/7` to `/8`, following the convention that a change to `pmi::read` output changes the reader binding. The JSON form remains version 2.

The ten NIST corpus files identify themselves as geometry-only. Positive graphic-PMI coverage uses the seven existing NIST AP242 fixtures, including CTC and FTC models. `crates/haecceity/tests/pmi_presentation.rs` checks both groups, as well as synthetic inheritance, content links and geometry-only styling.
