# Recognition 6: file-coordinate measurements (for #92)

Readers in draftwright and Specify should accept `quiddity-rust/recognition/6`.
Every published record is usable without the top-level `frame`, which remains recognition
provenance. There is no `local` key, including on nested records. Family names, record IDs,
recognition predicates and frame voting are unchanged; the three `oriented_*` families remain.
The crate version is unchanged. Fingerprints remain `quiddity-rust/fingerprint/3`: their
exact intrinsic form and values are unchanged, including formerly frame-relative records.

An axis-valued field can be a file-axis letter (`"x"`, `"y"`, `"z"`) or a unit vector
`[x,y,z]` in file coordinates. This applies to `axis`, `long_axis`, `width_axis`,
`depth_axis`, `other_axis`, `thickness_axis`, nested `profile.axis`, support-plane axes
and mirror-plane axes. Existing vector-valued directions retain their contracts.
Only records that previously had frame-relative fields acquire new representations.
A file-axis record retains its existing fields and values. No `_letter` hints are added.
Publication retains #70's coordinate-floor snap; a snapped letter names the positive file
axis, with coordinates, interval ends and end flags adjusted to that sense.

For a converted record, `origin` is a file-coordinate point: the recognition origin carried
into file coordinates. Scalar coordinates remain signed distances from this point along
their named measuring vectors. Reconstruct a point by adding each coordinate times its
vector to `origin`. Several directions share this one origin. Directions alone (such as a
fillet's `axis`, whose position is already `at`) do not require an origin.

| Measurement | Published measuring directions |
|---|---|
| Slot/pocket/channel `w_center`, `lo`/`hi`, `d_lo`/`d_hi` | `width_axis`, `long_axis`, new `depth_axis` |
| Riser `positions`, `other_positions`, `z_lo`/`z_hi` | `axis`, `other_axis`, new `height_axis` |
| Level `z`, `x_span`, `y_span` | new `height_axis`, `x_axis`, `y_axis` |
| Turning/prism stations, profile `span`, run intervals | the record's `axis` |
| Plate `u`/`v`, two-dimensional sections and flat `axis_line` | new ordered `section_axes` (the original two transverse measuring axes) |
| Pad `x0`/`x1`, `y0`/`y1`, `z0`/`z1` | new ordered `basis_axes` |
| Unequal ramp `half_widths` | new `width_axis`, low side first |
| Gusset support coordinates, mirror-plane coordinate | axis in the corresponding support/mirror pair |

Riser `vertical` means parallel to the published `height_axis`, not necessarily file Z.
`lo_at_envelope` and `hi_at_envelope` describe the low and high ends along `height_axis`;
they follow those endpoints when sense reverses. `open_sign`, `depth_sign`, pad direction
and gusset support-side signs refer to their published directions. Empty position arrays
remain empty. Nested levels and pattern members use the same decoding rules independently.
An unconverted record without `origin` retains the existing absolute file-coordinate rules.
Flat `stock_span` already uses `axis_direction` from the file origin and retains that rule.

A converted `body_key` becomes `{origin, axes, bounds, volume, area}`. `axes` is an ordered
three-direction basis; `bounds` is three `[lo,hi]` distance intervals from this origin.
A converted turned-profile `body_bounds` uses `{origin, axes, bounds}`. These preserve the
oriented box, not its larger file-aligned envelope. Unconverted box arrays retain their
old layout. `profile.axis_origin` is a full file-coordinate point after conversion.

Already explicit passage/recess frames remain self-contained. If independently rounded
reanchoring would invalidate a previously proved recess, carry its original relative geometry
and complete measuring frame without a second rounding; do not discard it or require provenance.

Rust's `Recognition.features` and `Mapped` use `PublishedFeatures`. Serialize it or call
`families()` for the schema-6 payload. Its typed `Deref` view is retained for working kernel
algorithms, not as a decoder for the published contract. Intrinsic fingerprint extraction
continues to use the exact-motion working measurements, with axes resolved at extraction;
there is no correspondence-side repair through the provenance frame.
