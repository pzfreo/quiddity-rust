The `gramil_shaft.*` STEP files and recipe are Paul Fremantle's gramil production
shaft at `9171ef6` (https://github.com/pzfreo/gramil), supplied for #134. The only
old/new change is `shaft.end_chamfer: 0 -> 1.5`. `shorter` additionally sets length
to 35.6; `flat_moved` additionally sets flat depth to 0.8. The geometry and recipe
are OCL v1, with the full licence in [LICENSE-gramil](LICENSE-gramil).

`q134_plate.*` are original Apache-2.0 controls: a 100 × 60 × 4 base joined to a
100 × 8 × 30 wall, then the base thinned to 2 or removed entirely. The recipe is
[make_edge_treatment_fixtures.py](../../../tools/make_edge_treatment_fixtures.py).
`expected.json` checks the feature classes, changed dimensions and surviving
trimmed face partners. All other revision pairs retain their existing recipe,
`tools/make_revision_fixtures.py`.
