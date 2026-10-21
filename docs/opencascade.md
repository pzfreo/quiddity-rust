## Notes on matching OpenCascade

OpenCascade silently heals files on import, and Python's answers depend on it. Things the
reader reproduces:

- `FACE_BOUND` orientation flags are not trusted: OpenCascade writes `.F.` on some toroidal
  faces whose edges are already listed in the face's sense and fixes the result on reading.
  Face containment therefore uses orientation-free crossing parity; orientation only breaks
  genuine ties (which way round a sphere's pole a boundary goes, and which side of its two
  loops a seamless torus band lies — files written by OpenCascade always have the seam).
- A void shell used reversed (`ORIENTED_CLOSED_SHELL` `.F.`) reverses each of its faces with
  its loops, as OpenCascade's explorer does, not just the faces' normals.
- A placement without a reference direction takes `gp_Ax2`'s default x axis.
- Face UV ranges include the control polygons of B-spline pcurves that span their edge,
  because OpenCascade boxes pcurves by their poles; they also hold the edges themselves (a
  file's pcurve may stray from its edge), except where a pcurve holds a parameter constant.
- Closed edges (full circles) are exempt from the orientability check: their recorded
  direction is not evidence.

Known kernel limitations: OpenCascade's healing adds missing seam edges to periodic faces (and splits closed edges they cross), which the reader does not; sphere patches that pass through a pole have approximate interior
bounding boxes; closed surfaces of revolution/extrusion in NURBS form are treated as
non-periodic, and a face on such a surface whose own seam is not the surface's (its boundary
crosses the surface's seam mid-edge, cgb217 face 29) has no area, so its solid has no mass. A face swept into a probe solid (`crates/haecceity/src/sweep.rs`, Python's
`Solid.extrude`) may be bounded only by lines and by circles and arcs about the sweep: the
edge-open recess floor proof declines a floor with any other edge (rust-wrong; no captured call
or corpus part has one).

Rays meet a face where they cross it inside its trim, or within the band by which one of its
edges strays from its surface: B-spline faces exported as approximations of their neighbours
leave cracks up to a few tenths of a millimetre wide, which OpenCascade covers with the edge
tolerances it sets on import. A hit found only in that band gives way to the face across the
crack. Where such an edge joins two analytic faces and overshoots (a B-spline edge for the
intersection of two cylinders, displaced into the opening it bounds), a hit within a few bands
of it that lies past the other face's surface is not on the face, whatever its trim says. A
line that only touches a face at its edge (a tangent blend) bounds no material there. A sphere
bounded only by a vertex loop at a pole is the whole sphere.

A B-spline crossing is tested against the trim at the parameters the intersection solved for,
not by inverting the surface again at the point: on cgb243's combs the inversion settled on a
face boundary 1.3 to 1.5 mm away and the trim test dropped crossings well inside faces 223, 225,
238 and 239, leaving 8 interior-void grid lines with odd counts (and 7 comb crossings of faces
233 and 234 missed on another, as OpenCascade misses them). Where a crossing is counted, the band
of an edge also reaches as far as the edge strays, at that point, from the face across it: the
crack is that wide there whichever face the edge leaves (cgb217's cylinders 51 and 358 beside
B-spline faces 43 and 47, crossings 2.8 and 6.0 µm outside their trims that parity needs). A hit
asked about alone (visibility) keeps to the face's own band, having no face across to yield to.
The overshoot reach (`OVERSHOOT_BANDS`, 4) is empirical: the corpus needs at least 1.8 and
breaks at 70 (evidence at the constant).
