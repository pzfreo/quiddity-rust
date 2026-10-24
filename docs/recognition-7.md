# Recognition 7: one family per feature kind (for #92)

Accept `quiddity-rust/recognition/7`. Schema 6's file-coordinate rules remain in
[recognition-6.md](recognition-6.md): file-axis letters stay letters; other measuring
axes use vectors and scalar distances from an explicit origin. The crate version is unchanged.

Publication merges `oriented_slots` into `slots`, `oriented_chamfers` into `chamfers`,
and `oriented_slot_patterns` into `slot_patterns`. Recognisers, their proofs, frame
voting and accepted occurrences are unchanged. No patterns are rediscovered and no
occurrences are deduplicated. Other families, including oblique through steps and blends,
remain separate.

| Source record | Published mapping |
|---|---|
| Oriented slot | `Slot`: `width_direction` → vector `width_axis`, `long_direction` → vector `long_axis`; `source.frame.run` → `depth_axis`; `source.frame.origin` → `origin`. Project source profile vertices into width/long directions for `w_center` and `lo`/`hi`; `source.run_interval` supplies `d_lo`/`d_hi`. Keep intrinsic `width`, `length`, and `body_key`. |
| Slot detail | Optional `source` retains the proved passage geometry; plain slots have null source. Unproved `end_radius` and `corner_radius` are null. No rectangular source is fabricated. |
| Oriented chamfer | `Chamfer`: `run` → vector `axis`, `turned:false`; retain `leg1_direction`, `leg2_direction`, `length`, `support_spans`, and `body_key`. Plain chamfers have null for these optional, unproved details. Support spans retain their distances along the run from `at`. |
| Oriented slot pattern | `SlotArray` or `SlotGrid`, with members mapped by the same slot conversion. Existing pitches, directions, grid angle and centre retain their schema-6 conventions; grids add explicit `row_direction` and `col_direction` vectors. |

Each family contains accepted plain records first, then accepted oriented records, in
each recogniser's existing deterministic order. IDs are `<merged-family>/<index>`.
Promoted measuring axes are normalised to unit length; existing detailed source vectors
retain their schema-6 precision. File-axis letters stay letters.
The append offset is the number of plain records. Pattern members, fingerprint IDs,
evidence status and face-accounting references use this same translation.

Fingerprints move from `quiddity-rust/fingerprint/3` to `/4` because they include the
family. Former oriented records change family and ID; oriented chamfers acquire the
common `turned:false` trait and oriented slots drop source-end proof traits. All
other intrinsic fingerprints are unchanged; the envelope version changes on every output.
Optional detailed geometry is not a new feature kind or a semantic trait.

`quiddity correspond old.json new.json` accepts saved schema-6 CLI recognition output
with v3 fingerprints and adapts the three namespaces before matching, retaining old IDs
on the old side. STEP input is recognised with the current implementation. Other old
fingerprint versions still require re-recognition. A bare recognition document without
fingerprints must be paired through its source STEP or defining faces.

Numeric IDs are run-local: do not persist a renamed family with the old numeric suffix.
Migrate saved selections and Specify `family:faces` overrides by the merged family and
sorted defining faces, or follow correspondence's old/new ID pairs. Face indices are
specific to a STEP revision; use correspondence when the source revision changes.

Rust consumers obtain the merged contract through `PublishedFeatures::families()` or
serialization. The public `Deref` into the working inventory is removed so typed field access cannot
expose an unmerged family; working inventory access remains internal. The published family registry has
45 families; raw recogniser APIs retain their internal types.
