Specify-generated #137 placement regression parts (Apache-2.0, Paul Fremantle).
spindle: resume-alpha6/shaft; plate: cross-engine parts/plate;
corners-flatness: parts/four_corner_holes_are_quiddity_rectangle_with_members,
with steel and datum.A.flatness=0.02 answered. Written through Specify at the
#137 round-1 Hershey/local-placement implementation. Tests replace these files' PMI
again and independently bound the resulting geometry against the part extents.

Fixture preparation removes Specify's DESCRIPTION_ATTRIBUTE/ID_ATTRIBUTE provenance stamps
on semantic items: the kernel intentionally refuses to remove these foreign associations
when replacing PMI. Re-test output files retain all original stamps. Geometry, PMI and
callout graphics are unchanged in these regression inputs.
