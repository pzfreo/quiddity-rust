# Evidence document, version 1

`quiddity evidence part.step` and Python `quiddity.evidence(step)` emit
`quiddity-rust/evidence/1`. This is the owned JSON form of the root crate's
`RecognitionEvidence`, available to Rust through `evidence_view::document(&part, framed)`.
It is additive: recognition/6 and fingerprints/3 are unchanged.

The top-level fields are:

| Field | Meaning |
| --- | --- |
| `schema_version` | `quiddity-rust/evidence/1` |
| `coordinate_space` | `file` by default; `part` with `--framed` / `framed=True` |
| `frame` | null by default; inferred origin, x/y/z axes and gauge in file coordinates when framed |
| `face_count` | Number of input faces; all face indices are zero based |
| `face_sources` | Array indexed by face: `{entity, instance}` identifies the original STEP `#entity` and its placed instance, or null for geometry without source identity |
| `features` | Accepted evidence features and section-recess refusal records |
| `candidates` | Rejected candidates and the transitive closure of their reconciliation links |
| `rejected_candidates` | Positions in `candidates` |

Each feature has `index` (within its family), `family`, `record` (the Rust
record serialized as JSON), `defining_faces`, `constituent_faces`,
`instance_faces` (ordered groups of constituent faces), `host_faces`, and
`members` (positions in `features`, for hole-pattern members). Face lists
are sorted and unique. Empty instance/host/member lists mean no such evidence
is available. A face is an integer, never an OCP or build123d object.

Each candidate has `family`, family-local `index`, `outcome`, optional `reason`,
`defining`, `constituent`, and `related` (positions in `candidates`). A null
reason denotes default acceptance. These candidate field names follow the
existing Rust evidence view.

The evidence view retains its own physical-family registry and section-recess
aggregation: recess source families are represented by `section_recesses`,
`passages` names section passages, and oriented families retain their names.
It is not a one-to-one enumeration of recognition/6 records. Evidence IDs and
order belong to this run; never join the two documents by array position or
assume the same family-local index after oriented-family merging.

Default evidence recognises directly in file coordinates, as the old caller-space
evidence API did. Framed evidence transforms the loaded geometry into an inferred
part frame before recognition; records then use that local frame. Both preserve
exactly the input face numbering. Framed requests refuse when the frame cannot
be inferred. This differs from recognition/6, whose records always use file
coordinates and whose frame is provenance only.

For edit operations, retain the exact exported bytes and a verified mapping
from `face_sources` to the live shape's faces. Check every index and invalidate
all handles when the source changes. STEP entity IDs are scoped to one export;
`instance` disambiguates repeated geometry. An entity ID alone is not a durable
revision identity. Use fingerprints and correspondence between revisions.
