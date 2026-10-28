# Fusion input

Install with `cargo install quiddity --features f3d`, or enable `f3d` on the
`haecceity` or `quiddity` dependency. Both library and CLI defaults leave it off.
The optional cadmpeg dependencies are pinned to `=0.6.7` (Apache-2.0, pure Rust).

`haecceity::f3d::read_f3d_file(&Path)` and `read_f3d(&[u8])` return
`Result<Part, Box<dyn std::error::Error>>`. `to_step(&[u8])` supplies the same
conversion as bytes for clients needing STEP part definitions and stable face anchors.

The route is cadmpeg-codec-f3d → selected CadIr → cadmpeg-codec-step in memory →
haecceity's existing STEP reader. This reuses the tested geometry path and cadmpeg's
body transforms and occurrence placements. Body transforms are restated as child
occurrences before export: cadmpeg's body-local `MAPPED_ITEM` representation is not
resolved by the existing STEP scene. This preserves the same rigid motion without
changing STEP import. Non-rigid body placements refuse. Fusion model lengths decode to millimetres;
STEP export declares millimetres. The generated unit cube is 10 × 10 × 10 mm,
six faces and 1000 mm³.

Only `kind == Solid` bodies with `visible != Some(false)` are selected. Their
geometry reference closure is retained, including pcurves and procedural supports;
sketches, wire/sheet bodies, free geometry, history and design intent are omitted.
Product definitions and occurrences retain their placements and only the selected
body references. If all solids are hidden, import refuses and names the hidden count.
Empty and wire/sheet-only designs refuse. Blocking cadmpeg decode/export losses refuse;
non-blocking losses are accepted. The usual STEP completeness checks still apply.

The CLI dispatches `.f3d` case-insensitively for recognition, `correspond`, `parts`,
`brep` and `serve` (the protocol's existing `step` field can name a Fusion file).
Conversion produces the usual warning on stderr; the service carries it in `warnings`.
Recognition's `source_sha256`, and `parts`/PMI bindings, hash the original Fusion bytes.
Face anchors refer to the converted geometry, so keep the reader/dependency versions
fixed while preparing PMI. Builds without the feature explain how to enable it.

Fusion input is **geometry only: no PMI is imported**. `pmi read` returns the empty
PMI document bound to the Fusion source; edit it, optionally run `pmi check`, then use
`quiddity brep part.f3d --pmi intent.json -o part.step` for new AP242 geometry and PMI.
The existing one-part requirement for `brep --pmi` applies. `pmi write` refuses Fusion
input because it edits original STEP text. Output is STEP, never an edited Fusion archive.

Before integration, cadmpeg CLI 0.6.6 matched Autodesk STEP face counts for 294/297
Fusion Gallery parts, and quiddity recognition for 286/297. On 21 private sample designs,
20 decoded without blocking losses; five single-solid parts already recognised through
unfiltered CLI export (Toe: 33 faces, 10 records, no unproved; Shell `45df37c8`: 74 faces,
46 records). The other 15 were obstructed by exported sketch wires or hidden solids
(Hip Servo: 14 solids, 11 hidden, plus 660 wire bodies). Foot failed upstream with two
blocking losses. These samples are local validation inputs and are not distributed.
Only cadmpeg's Apache-2.0 encoder-generated cube is committed, with attribution beside it.

Local validation with 0.6.7 imports all 20 decodable designs (970 faces, 537 records;
three records remain unproved: two on Hip Ball `197f6865`, one on Knee Ball `594773a4`).
All five previously working real designs have identical recognition to the 0.6.6 CLI
route, excluding source hashes. Toe's millimetre bounds also agree exactly with that route:
`[-49.39692620785909, 0, -10]` to `[0, 10, 3.0781812899310186]`.
Shell `2844649c` is a different design: two visible solids, one hidden, 175 faces.
