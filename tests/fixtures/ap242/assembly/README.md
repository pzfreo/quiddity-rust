Regenerate `assembly.step` with specify-core's venv: `/Users/paul/repos/specify-core/.venv/bin/python tests/fixtures/ap242/assembly/make_assembly.py` (build123d; the header timestamp changes on every run).

Regenerate `wrapped.step` the same way with `make_wrapped.py`; it writes the assembly name with Part 21 escapes itself, since OpenCascade writes non-ASCII labels unescaped.
