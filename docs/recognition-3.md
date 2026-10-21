# Recognition document 3

`quiddity-rust/recognition/3` is the native drawing contract. Version 2 already
published unified `section_recesses`, including `SectionRecessRefusal` records
following the occurrences, and the `step_levels` and `risers` evidence families.
Version 3 retains these Python record payloads and adds unified recess patterns.

`section_recess_patterns` records have type `SectionRecessArray` or
`SectionRecessGrid`. Their parameters match Python's derived
`section_recess_patterns`: `members` are run-local occurrence indices (the
`index` field of `SectionRecess`), not source pocket indices or document IDs.
Arrays carry `pitch` and `direction`; grids carry `rows`, `cols`, `row_pitch`,
`col_pitch`, `row_direction`, `col_direction`, and `center`. Faces are the sorted
union of members' defining faces. These derived records have no independent
feature fingerprints. Refusals have no fingerprint either.

The projection is computed once with recognition and reconciliation. Its patterns
are retained rather than rederived by consumers. Rigid frame mapping carries
pattern vectors and centres with the occurrences. Readers must reject local
spatial fields they cannot interpret. Other record shapes, IDs and order stay as
in version 2. All source recess families remain published for other consumers.

The Python oracle comparisons in `tests/internal/section_recesses.rs` cover occurrences,
refusals and patterns; `tests/internal/section_recess_publication.rs` checks the records
actually handed to consumers, including the 13975/14052 local-degradation retry.

Measured helical threads use the optional [version 4 extension](recognition-4.md).
Documents without measured-thread records or refusals retain version 3 and their
existing bytes.
