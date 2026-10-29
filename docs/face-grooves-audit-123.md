# Face-groove audit for #123

Baseline: unmodified main `42649a2`. Candidate: intrinsic `face_grooves` implementation
on `cx/facegrooves`. The baseline release CLI was built before source edits and retained
outside the repository. No STEP reproduction or geometry copied from it is committed.

The audit ran both release CLIs over **2,963** distinct available STEP paths: all 100
pinned corpus files (Python revision `6d544e060cd0eeb4442d2196e5df757b5a15ea55`), every
STEP/STP fixture recursively, including compressed captures, and the local 608ZZ.
**2,952** were readable; **11** reader refusals were identical. Every old document
record (including parameters, faces, record type, ID and evidence status) and every old
feature fingerprint compared exactly, without numerical tolerances. All six newly
claimed faces were `unexplained` on main, with no feature or body claim.

There were **zero exceptions**. The complete list of new occurrences is:

| Part | Faces | Floor | Rim outer/inner diameter (mm) | Depth (mm) | Outer/inner wall angles |
| --- | --- | --- | --- | --- | --- |
| Local `bearing-608zz.step`, positive end | 2, 4, 6 | 4 | 18.5 / 13 | 0.25 | 44.999971914041566° / 44.999971914041566° |
| Local `bearing-608zz.step`, negative end | 9, 11, 13 | 11 | 18.5 / 13 | 0.25 | 44.999971914041566° / 44.999971914041566° |

The floor diameters are approximately 18.00000049 / 13.49999951 mm. The rim is at
z = ±3.5 mm, floor at z = ±3.25 mm. Thus the nominal 18/13 description mixes different
levels; this family consistently measures both diameters at the rim. Angles retain the
STEP's measured semi-angle, approximately 45°, rather than rounding to a nominal value.

The pre-recording audit gate passed. Existing `annular_grooves` records are retained;
they already describe square-walled face recesses on main. Existing claims also take
precedence over treated grooves, including generic blend and residual-profile claims.
Body classifications may share faces. Coaxial grooves on prismatic hosts are eligible.
Tangent toroidal corner rounds and short conical treatments join the defining faces when
the complete floor/wall/open-plane proof holds. The rule and record contract are in
`recognition-6.md`.

The audit can be repeated with `tools/audit_face_grooves.py`, exporting the required
`QUIDDITY_CORPUS` and `QUIDDITY_CORPUS_REQUIRED=1`:

```sh
python3 tools/audit_face_grooves.py --capture /tmp/quiddity-main-42649a2 --output /tmp/main.json.gz
python3 tools/audit_face_grooves.py --compare target/release/quiddity --baseline /tmp/main.json.gz --output /tmp/audit.json
```

Recognition captures and audit JSON remain outside the repository. Because no captured
fixture or corpus part acquires a new record, its inventory floors need no numerical
changes. Procedural tests pin the new occurrences, exact face lists, rim dimensions,
rotated publication, square walls, corner treatments, prismatic hosts, split floors and
boundaries, counterbore exclusion and preservation of existing claims. Wiring and
optional-family assertions explicitly name #123.
