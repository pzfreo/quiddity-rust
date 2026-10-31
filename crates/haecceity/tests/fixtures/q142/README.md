# #142 xcaf-pmi producer regressions

`geometry.step.gz` is the supplied `g.step`, compressed deterministically without
editing its AP242 edition 1 declaration or named faces. `datum-target-decimals.json`
is the target/datum subset of the producer's `pmi.json`. `requirement-note.json`
keeps the diameter dimension, flatness tolerance and tolerance-owned note from
`tolerance_note_alone.json`; the tests also use the dimension owner from
`dimension_note.json`. Unrelated features and requirements are removed.

These fixtures intentionally retain `.0` tokens, caller ids and face-name anchors.
The tests compare complete JSON values after import/export, validate the written
schema/rules and cover every note owner, including requirements sharing support.
