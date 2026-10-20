"""Compare Rust signatures with live OCP TopExp order; writes no reference captures.

Usage: OCP_PYTHON tools/check_dataset_face_order.py STEPS_DIR signatures.jsonl
Generate signatures with examples/face_order.rs. All faces must match in index order
at 1e-6 relative area and 1e-6 model-scale centroid tolerance. No nearest-neighbour
fallback: an order mismatch fails, requiring an explicit verified mapping.
"""
import json
import sys
from pathlib import Path
from OCP.STEPControl import STEPControl_Reader
from OCP.TopExp import TopExp_Explorer
from OCP.TopAbs import TopAbs_FACE
from OCP.TopoDS import TopoDS
from OCP.BRepAdaptor import BRepAdaptor_Surface
from OCP.BRepGProp import BRepGProp
from OCP.GProp import GProp_GProps


def check(steps, signatures):
    models = faces = 0
    failures = []
    max_area = max_centroid = 0.0
    kinds = ['Plane', 'Cylinder', 'Cone', 'Sphere', 'Torus', 'Freeform', 'Freeform', 'Freeform', 'Freeform', 'Other', 'Other']
    for line in Path(signatures).read_text().splitlines():
        model = json.loads(line)
        reader = STEPControl_Reader()
        assert int(reader.ReadFile(str(Path(steps) / (model['id'] + '.step')))) == 1
        reader.TransferRoots()
        explorer = TopExp_Explorer(reader.OneShape(), TopAbs_FACE)
        actual = []
        while explorer.More():
            face = TopoDS.Face_s(explorer.Current())
            prop = GProp_GProps()
            BRepGProp.SurfaceProperties_s(face, prop, 1e-9)
            c = prop.CentreOfMass()
            actual.append(dict(kind=kinds[int(BRepAdaptor_Surface(face).GetType())], area=prop.Mass(), centroid=[c.X(), c.Y(), c.Z()]))
            explorer.Next()
        labels_path = Path(steps).parent / 'labels' / (model['id'] + '.json')
        if labels_path.exists():
            labels = json.loads(labels_path.read_text())
            assert len(labels) == 1 and labels[0][0] == model['id']
            seg = labels[0][1]['seg']
            assert set(seg) == {str(i) for i in range(len(actual))}, model['id']
        models += 1
        if len(actual) != len(model['faces']):
            failures.append([model['id'], 'count', len(actual), len(model['faces'])])
            continue
        for i, (a, b) in enumerate(zip(actual, model['faces'])):
            faces += 1
            da = abs(a['area'] - b['area']) / max(abs(a['area']), 1e-9)
            dc = max(abs(x-y) / max(1.0, abs(x)) for x,y in zip(a['centroid'], b['centroid']))
            max_area = max(max_area, da)
            max_centroid = max(max_centroid, dc)
            candidates = [j for j, other in enumerate(actual)
                          if other['kind'] == b['kind']
                          and abs(other['area'] - b['area']) <= 1e-6 * max(abs(other['area']), 1e-9)
                          and all(abs(x-y) <= 1e-6 * max(1.0, abs(x))
                                  for x,y in zip(other['centroid'], b['centroid']))]
            if candidates != [i] or a['kind'] != b['kind'] or da > 1e-6 or dc > 1e-6:
                failures.append([model['id'], i, a, b, da, dc])
    print(json.dumps(dict(models=models, faces=faces, max_relative_area=max_area, max_scaled_centroid=max_centroid, failures=failures)))
    return bool(failures)


if __name__ == '__main__':
    sys.exit(check(*sys.argv[1:]))
