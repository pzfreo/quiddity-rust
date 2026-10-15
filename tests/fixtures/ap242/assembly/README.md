Regenerate `assembly.step` with specify-core's venv: `/Users/paul/repos/specify-core/.venv/bin/python tests/fixtures/ap242/assembly/make_assembly.py` (build123d; the header timestamp changes on every run).

Regenerate `wrapped.step` the same way with `make_wrapped.py`; it writes the assembly name with Part 21 escapes itself, since OpenCascade writes non-ASCII labels unescaped.

Regenerate `nested.step` the same way with `make_nested.py`: the same plate and pin, the pins in a sub-assembly `pins` placed at z=5.

`editor_assembly.step` is a verbatim copy of draftwright-editor's `scripts/fixtures/assembly.step` (also specify-core's `tests/fixtures/parts/assembly.step`): an earlier run of `make_assembly.py` with both pins unturned, at x=-30 and x=+30. Do not regenerate it; copy it again if the editor's changes.
