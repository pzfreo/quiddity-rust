"""For #134: derived from gramil 9171ef6 (gramil contributors), production shaft, only end_chamfer 0 -> 1.5.

Originally supplied for pzfreo/specify-core-rust#67 (provenance).

Run with gramil installed (Python 3.12, build123d 0.10.0):
    python gramil_shaft.py OUTDIR
Uses gramil's build_shaft and its own STEP export path (build123d.export_step).
The derived geometry and this recipe are OCL v1; see LICENSE-gramil.
"""
import gzip
from pathlib import Path
import sys

from build123d import export_step
from gramil.parameters import PurflingCutterParams
from gramil.parts.shaft import build_shaft

out = Path(sys.argv[1])
for which, chamfer, extra in (("old", 0.0, {}), ("new", 1.5, {}),
                              ("shorter", 1.5, {"length": 35.6}),
                              ("flat_moved", 1.5, {"flat_depth": 0.8})):
    p = PurflingCutterParams()
    p = p.model_copy(update={
        "process": p.process.model_copy(update={"prototype": False}),
        "shaft": p.shaft.model_copy(update={"end_chamfer": chamfer, **extra}),
    })
    path = out / f"gramil_shaft.{which}.step"
    export_step(build_shaft(p), str(path))
    path.with_suffix(".step.gz").write_bytes(gzip.compress(path.read_bytes(), mtime=0))
    path.unlink()
