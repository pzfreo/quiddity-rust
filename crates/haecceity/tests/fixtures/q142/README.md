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

Round 3: `density-integer-exponents.json` is the producer's minimal g/cm³ unit
case (integer-spelled exponents). `invalid-density.step.gz` adds only a 13-entity
material graph to the shared geometry, reproducing OCCT's gram³ × centimetre²
density. Round 4 explicitly permits reading this exact two-factor form with a
repair finding. Negative variants with other exponents or extra factors still
omit density and name the dimensional reason. Both old SI-gram and new
conversion-based gram are tested.

Round 4: `occt-density-plate.step.gz` is an unedited, deterministically compressed
copy of the supplied `cadgen_pmi_plate.step` (OCCT 8), SHA-256
`5fb6f915c1e1ae392c16a81b0b3f43c9a829c4fa50b33a7aaac14651056da6d1`.
It has both the gram³ × centimetre² unit and an untyped REAL density value.
The regression reads 2.7 g/cm³ with both findings and writes the practice form,
checking read-back identity, AP242 validity and removal of both source defects.
