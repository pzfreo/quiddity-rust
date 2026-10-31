# haecceity

An exact B-rep geometry kernel in pure Rust, with no OpenCascade or other native dependency.
It is the kernel under [quiddity](https://crates.io/crates/quiddity), which recognises
manufacturing features. Use haecceity directly when you need the geometry itself.

- **STEP:** reads AP203, AP214 and AP242 files (plain or gzipped) into a B-rep `Part`:
  solids, faces, edges and their exact surfaces and curves. A malformed or unsupported file is
  refused with a typed error, never a panic.
- **Geometry:** rays and point classification, face parameter space, volumes, areas,
  centroids and face moments, shared volumes and face coverage, analytic surfaces recovered
  from freeform ones, meshing and hidden-line views.
- **PMI:** reads and writes semantic AP242 PMI (dimensions, geometric tolerances, datums and
  notes) and reports where a file departs from the recommended practices.
- **Writing:** writes a B-rep and its PMI to a new AP242 file.

```toml
[dependencies]
haecceity = "0.2"
```

The default `presentation` feature exposes `haecceity::presentation`: dimension/tolerance
notation, feature control frames, datum frames, and local 2D text/symbol strokes shared with
drawing clients. Every newly written dimension and tolerance gets a graphical callout;
face textures get ISO 1302 symbols and written part settings share a general-notes block.
Property links retain their semantic requirements, and notes clear all other callouts;
existing foreign PMI/presentation remains untouched on append. Disable default features for
semantic-only dimension/tolerance writing and the legacy datum lettering. The feature is on
by default so Specify's normal writer output carries visible PMI. Public-domain Hershey lettering and explicit
ISO symbols keep files small. Datum leaders start inside their faces and leave normally,
in planes containing the face normal. Text height is 0.5 mm + 1.8% of the largest extent;
frames use short feature-scaled offsets with a deterministic stagger. Multi-face aspects
are composite, and location references use non-datum aspects in both feature configurations. Straight dimensions use two
measured witnesses, extension lines and arrowed lines on shortest-first ladders. See the [AP242 writer structure](../../docs/step-ap242.md#graphical-callouts-137).

Drawing clients can use these public functions without constructing `PartPmi`:

```rust
use haecceity::presentation::{self, gdt, notes, tolerance};
let plex_width = |text: &str, height: f64| text.chars().count() as f64 * height * 0.6;
let frame = gdt::control_frame("position", "0.05", Some("diameter"), Some("M"),
    &["A".into(), "B-C".into()], &plex_width);
assert_eq!(frame.cells, ["⌖", "⌀0.05Ⓜ", "A", "B-C"]);
let label = presentation::label_with_width("⌀10 H7", &plex_width);
assert_eq!(gdt::symbol("profile_surface"), "⌓");
assert_eq!(tolerance::HOLE_CLASSES, ["H", "G", "F"]);
```

`gdt::control_frame(name: &str, tolerance: &str, zone: Option<&str>,
material: Option<&str>, datums: &[String], width: &dyn Fn(&str, f64) -> f64) -> Callout`
accepts already formatted cells. The width function measures ink at the supplied capital
height in the same drawing units as the returned geometry. `gdt::symbol(name: &str) ->
&'static str` uses model names (`profile_line`, `profile_surface`, `circular_runout`,
`total_runout`, etc.; see its rustdoc). `gdt::characteristic(name, centre, &mut Callout)`
adds just the characteristic geometry. `label_with_width(value, width)` and
`notes::surface_texture_with_width(&SurfaceTexture, width)` size labels and ISO 1302
symbols with the same convention. `notes::removal_marks(MaterialRemoval, short_height)`
returns strokes and arcs about the symbol tip. Existing `label`, `surface_texture`,
`dimension` and `tolerance` use `text_width(value, height)` (Hershey ink width).
`Callout::letters` is the font-drawn layer; `runs` also includes geometrically drawn symbols.

`tolerance::{HOLE_CLASSES, SHAFT_CLASSES, GRADES}` are ordered arrays for discovery and
refusal messages; `MAX_NOMINAL_MM` is the inclusive 250 mm table limit. These describe the
supported subset, not every ISO 286 class.

Dimension strings use U+2300 `⌀` (Unicode's diameter sign), not U+00F8 `ø` (a letter).
ISO 129-1:2018 §§7.1–7.2 specifies the diameter symbol before the value; ISO 1101:2017
§8.2.2.1.1 uses it for circular/cylindrical tolerance zones. Neither standard prescribes
Unicode code points; this API chooses the semantically correct character. A single-line
asymmetric band is `+upper/lower`, including an unsigned zero (`+0.09/0`). This is the
explicit single-line form in ISO 129-1:2004 §6.2, Figure 29. ISO 129-1:2018 §6.2,
Figures 31–34 specifies stacked deviations (upper above lower) or an ISO code; §4.1.7
requires signs except for zero and decimal alignment when stacked. Drawing clients targeting
the 2018 presentation should stack the two deviations. `limits_term` already returns upper,
newline, lower; slash-separated dimension text is the unambiguous plain-text form, not a
claim that the 2018 edition specifies inline deviations. Sources:
[ISO 129-1:2018](https://www.iso.org/standard/64007.html),
[ISO 129-1:2004 §6.2](https://s538bda2d5ed2d98d.jimcontent.com/download/version/1588561433/module/9041319182/name/ISO%2520129_2004.pdf),
[ISO 1101:2017](https://www.iso.org/standard/66777.html).
Draftwright should replace `ø`/`Sø` with `⌀`/`S⌀` in dimension labels and retain the slash
for inline asymmetric deviations. Geometry may still use an existing font's slashed-ring glyph.

Enable the optional `f3d` feature for `haecceity::f3d::read_f3d_file`: visible Autodesk
Fusion solid geometry in millimetres, including placements, with no imported PMI.

```rust
let part = haecceity::step::read_step_file(std::path::Path::new("part.step"))?;
for solid in 0..part.solids.len() {
    if let Some((volume, area)) = part.solid_mass(solid) {
        println!("solid {solid}: volume {volume:.3}, area {area:.3}");
    }
}
```

*Haecceity* is "thisness": the particular thing itself, as *quiddity* is "whatness".

Licensed under Apache-2.0. Source and issues: <https://github.com/pzfreo/quiddity-rust>.


Enable the non-default `json` Cargo feature for `haecceity::json`: semantic
`haecceity.pmi/1`, the `haecceity.presentation/1` companion, and packaged schema
constants. See [the reference](docs/pmi-json.md) and `schema/` in the crate.
`json::identify`, `to_value`/`from_value` and `write` keep source identifiers and
resolve named/support anchors against caller-supplied geometry. The ordinary
writer and feature-off builds retain their output.
