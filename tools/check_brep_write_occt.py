"""Check the B-rep writer against OpenCascade: each input is written with `quiddity brep`, and
OpenCascade must read the written file with the input's face count and solid volume (the sum
over its solids).

Run with the read-only OpenCascade Python environment, outside the repository for --out:

    ../quiddity/.venv/bin/python tools/check_brep_write_occt.py \
        --quiddity target/release/quiddity --out /tmp/brep-occt FILE_OR_DIR...

Directories are searched for .step/.stp (optionally .gz). Prints one line per file and a
summary. Outcomes: `agree` (face and solid counts equal, volumes within --rel); `written-closer`
(counts equal, and OpenCascade's volume of the written file is nearer the kernel's volume, which
`quiddity brep` reports, than its volume of the input: OpenCascade reads the input less exactly);
`disagree` otherwise. Exits 1 on any `disagree`, or a written file OpenCascade cannot read.
"""
from __future__ import annotations

import argparse
import gzip
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

from OCP.BRepGProp import BRepGProp
from OCP.GProp import GProp_GProps
from OCP.IFSelect import IFSelect_RetDone
from OCP.STEPControl import STEPControl_Reader
from OCP.TopAbs import TopAbs_FACE, TopAbs_SOLID
from OCP.TopExp import TopExp
from OCP.TopTools import TopTools_IndexedMapOfShape


def read(path: Path):
    """The shape OpenCascade reads from *path* (gunzipped first), or None."""
    name = str(path)
    tmp = None
    if name.lower().endswith(".gz"):
        tmp = tempfile.NamedTemporaryFile(suffix=".step", delete=False)
        tmp.write(gzip.open(name).read())
        tmp.close()
        name = tmp.name
    try:
        reader = STEPControl_Reader()
        if reader.ReadFile(name) != IFSelect_RetDone:
            return None
        reader.TransferRoots()
        return reader.OneShape()
    finally:
        if tmp:
            os.unlink(tmp.name)


def measure(shape):
    faces = TopTools_IndexedMapOfShape()
    TopExp.MapShapes_s(shape, TopAbs_FACE, faces)
    solids = TopTools_IndexedMapOfShape()
    TopExp.MapShapes_s(shape, TopAbs_SOLID, solids)
    # The solids' volume: faces in no solid (a surface model) bound no volume.
    volume = None
    for i in range(1, solids.Size() + 1):
        props = GProp_GProps()
        BRepGProp.VolumeProperties_s(solids.FindKey(i), props)
        volume = (volume or 0.0) + props.Mass()
    return faces.Size(), solids.Size(), volume


def inputs(paths):
    for p in map(Path, paths):
        if p.is_dir():
            for q in sorted(p.rglob("*")):
                n = q.name.lower()
                if q.is_file() and n.endswith((".step", ".stp", ".step.gz", ".stp.gz")):
                    yield q
        else:
            yield p


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--quiddity", required=True)
    ap.add_argument("--out", required=True, type=Path)
    ap.add_argument("--rel", type=float, default=1e-6, help="volume tolerance, relative")
    ap.add_argument("paths", nargs="+")
    args = ap.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    counts = {}
    failed = False
    for i, src in enumerate(inputs(args.paths)):
        out = args.out / f"{i:05d}_{src.name.removesuffix('.gz')}"
        run = subprocess.run([args.quiddity, "brep", str(src), "-o", str(out)],
                             capture_output=True, text=True)
        if run.returncode != 0:
            outcome, detail = "not-written", run.stderr.strip().splitlines()[-1:]
        else:
            a, b = read(src), read(out)
            if a is None:
                outcome, detail = "input-unreadable", []
            elif b is None:
                outcome, detail = "written-unreadable", []
            else:
                (fa, sa, va), (fb, sb, vb) = measure(a), measure(b)
                detail = [f"faces {fa}/{fb}", f"solids {sa}/{sb}", f"volume {va}/{vb}"]
                kernel = sum(v for v in json.loads(run.stdout)["solid_volumes"] if v is not None)
                detail.append(f"kernel volume {kernel}")
                same_volume = (va is None and vb is None) or (
                    va is not None and vb is not None
                    and abs(va - vb) <= args.rel * max(abs(va), abs(vb), 1e-12))
                closer = (va is not None and vb is not None
                          and abs(vb - kernel) < abs(va - kernel))
                if fa != fb or sa != sb:
                    outcome = "disagree"
                elif same_volume:
                    outcome = "agree"
                else:
                    outcome = "written-closer" if closer else "disagree"
        counts[outcome] = counts.get(outcome, 0) + 1
        failed |= outcome in ("disagree", "written-unreadable")
        print(outcome, src, *detail, flush=True)
    print("summary", counts)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
