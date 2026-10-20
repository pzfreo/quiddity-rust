"""Independent OCCT references for issue #53, without writing or exporting geometry.

Run with a Python environment providing OCP, for example:
    ~/cld/quiddity/.venv/bin/python tools/check_revolution_fixtures.py

The truck files are original exports, including invalid orientation flags and the
pub hole file's duplicate IDs. Do not substitute OCCT's healed import for those
source bytes. The hand-written fixtures use exact circular quadrant edges and
bounded linear meridians; their analytic answers are the primary references.
"""

import gzip
import json
import math
import re
import tempfile
from collections import Counter
from pathlib import Path

import OCP
from OCP.BRepAdaptor import BRepAdaptor_Surface
from OCP.BRepGProp import BRepGProp
from OCP.GProp import GProp_GProps
from OCP.STEPControl import STEPControl_Reader
from OCP.TopAbs import TopAbs_FACE
from OCP.TopExp import TopExp_Explorer
from OCP.TopoDS import TopoDS

FIXTURES = Path(__file__).resolve().parent.parent / "tests" / "fixtures"
ANALYTIC = {
    "revolution_shaft": (15000 * math.pi, 2850 * math.pi),
    "revolution_cone": (12 * math.pi, 24 * math.pi),
    "truck_master_shaft_only": (15000 * math.pi, 2850 * math.pi),
    "truck_pub_shaft_only": (15000 * math.pi, 2850 * math.pi),
}

print(json.dumps({"OCP": OCP.__version__}))
for name in [
    "revolution_shaft", "revolution_cone",
    "truck_master_shaft_only", "truck_master_shaft_hole",
    "truck_pub_shaft_only", "truck_pub_shaft_hole",
]:
    directory = FIXTURES / "broken" if name == "truck_pub_shaft_hole" else FIXTURES
    data = gzip.decompress((directory / (name + ".step.gz")).read_bytes())
    entities = Counter(re.findall(rb"^#(\d+)\s*=", data, re.MULTILINE))
    duplicates = sorted(int(k) for k, v in entities.items() if v > 1)
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "input.step"
        path.write_bytes(data)
        reader = STEPControl_Reader()
        reader.ReadFile(str(path))
        reader.TransferRoots()
        shape = reader.OneShape()
        props = GProp_GProps()
        BRepGProp.VolumeProperties_s(shape, props, 1e-10)
        volume = props.Mass()
        BRepGProp.SurfaceProperties_s(shape, props, 1e-10)
        area = props.Mass()
        orientations = Counter()
        faces = TopExp_Explorer(shape, TopAbs_FACE)
        while faces.More():
            face = TopoDS.Face_s(faces.Current())
            surface = BRepAdaptor_Surface(face)
            orientations[f"{surface.GetType().name}/{face.Orientation().name}"] += 1
            faces.Next()
    row = {
        "fixture": name, "volume": volume, "area": area,
        "imported_orientations": dict(orientations),
        "duplicate_entity_ids": duplicates,
    }
    if name in ANALYTIC:
        row["analytic_volume"], row["analytic_area"] = ANALYTIC[name]
    print(json.dumps(row, sort_keys=True))
