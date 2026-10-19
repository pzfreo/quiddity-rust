# Recognition document 4

`quiddity-rust/recognition/4` adds measured `modelled_threads` (`ModelledThread`)
and `modelled_thread_refusals` (`ModelledThreadRefusal`). All version 3 families
retain their payloads and order. Documents requiring neither new record type
continue to use `quiddity-rust/recognition/3`; their complete outputs remain byte
identical. `SCHEMA_VERSION` names the latest supported contract;
`schema_version(features)` chooses the minimum required version.

A thread publishes:

- `axis.point` (the point on the line nearest the file origin) and unit
  `axis.direction` (dominant component positive, ties preferring Z).
- `side`: `internal` or `external`; `major_diameter`, `minor_diameter`, `pitch`;
  `hand`: `right` or `left`; `starts`: the measured number of starts.
- `start`, `end`: signed distances from `axis.point` along `axis.direction`;
  `length = end - start`. These bound the physical flanks and runouts, including
  partial end turns, rather than a nominal full-depth engagement length.
- `flank_faces`, `crest_faces`, `root_faces`, `runout_faces`: disjoint, sorted face
  indices. The document's `faces` is their sorted union. A cylindrical carrier can
  extend outside the thread's axial span: the STEP face is indivisible. Consumers
  must use both the span and face claims when suppressing modelled helix edges.
- `tolerances.diameter`, `.pitch`, `.axial`: geometric acceptance bounds in mm,
  including STEP interpolation error, **not** a metrological uncertainty certificate.

All lengths are millimetres. There is no ISO/ASME designation, nominal size,
thread class, or construction-history inference. Designation matching belongs to
the drawing consumer. Pitch is lead divided by starts, never the measured lead
mislabelled as pitch.

Thread records have ordinary feature fingerprints and defining/constituent face
evidence. Their claimed B-spline faces leave the aggregate `freeform_surfaces`
family; the standalone native-support inventory remains available unchanged.
Refusals carry an axis, sorted candidate `faces`, and a machine-readable `reason`.
They claim nothing, have no fingerprints, and leave the source freeforms intact.
The standalone `analyse_modelled_threads` API returns both occurrences and refusals.

Rigid mapping moves the complete line and its physical endpoints, re-anchors the
line, and reverses/swaps the axial distances when canonicalising its direction.
Hand, starts, dimensions, tolerances and face indices remain intrinsic. The new
records have no spatial fields left local to a working frame.

See [the corpus survey and recognition limits](modelled-threads.md).
