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
existing foreign PMI/presentation remains untouched on append. Disable default features for
semantic-only dimension/tolerance writing and the legacy datum lettering. The feature is on
by default so Specify's normal writer output carries visible PMI. Public-domain Hershey lettering and explicit
ISO symbols keep files small. Datum leaders start inside their faces and leave normally,
in planes containing the face normal. Text height is 0.5 mm + 1.8% of the largest extent;
frames use short feature-scaled offsets with a deterministic stagger. Multi-face aspects
are composite, and location references use non-datum aspects in both feature configurations. Straight dimensions use two
measured witnesses, extension lines and arrowed lines on shortest-first ladders. See the [AP242 writer structure](../../docs/step-ap242.md#graphical-callouts-137).

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
