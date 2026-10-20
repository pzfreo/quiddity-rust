"""Check Rust trim lengths against raw STEP curves, before OCCT import healing.

    OCP_PYTHON tools/perimeter_evidence.py --rust target/release/examples/trim_lengths FILE FACE...

This tool supports unplaced, millimetre STEP parts. It reads model entities without
TransferRoots: STEP #N selects each EDGE_CURVE, StepToGeom evaluates its exact 3D
curve, and GeomAPI projects the original VERTEX_POINTs onto it. SciPy integrates
speed between those feet at two tolerances, split at B-spline knots. Lines use
their stated vertices, as the trim contract does. No pcurves or recognisers are
used. Output is evidence on stdout, never a committed Python capture.
"""
import argparse
import gzip
import json
import math
from pathlib import Path
import re
import subprocess
import tempfile

from scipy.integrate import quad
from OCP.Geom import Geom_Line
from OCP.GeomAPI import GeomAPI_ProjectPointOnCurve
from OCP.gp import gp_Pnt, gp_Vec
from OCP.STEPControl import STEPControl_Reader
from OCP.StepToGeom import StepToGeom


def edge_length(edge):
    def vertex(v):
        point = v.VertexGeometry()
        return gp_Pnt(*(point.CoordinatesValue(i) for i in range(1, 4)))
    start, end = vertex(edge.EdgeStart()), vertex(edge.EdgeEnd())
    geom = edge.EdgeGeometry()
    if hasattr(geom, 'Curve3d'):
        geom = geom.Curve3d()
    curve = StepToGeom.MakeCurve_s(geom)
    if isinstance(curve, Geom_Line):
        return dict(length=start.Distance(end), change=0.0)
    def parameter(point):
        projection = GeomAPI_ProjectPointOnCurve(point, curve)
        candidates = [projection.Parameter(i) for i in range(1, projection.NbPoints()+1)]
        # OCCT's extrema solver can omit a bounded curve's endpoint entirely.
        candidates += [t for t in (curve.FirstParameter(), curve.LastParameter())
                       if math.isfinite(t)]
        return min(candidates, key=lambda t: curve.Value(t).Distance(point))
    a, b = [parameter(p) for p in (start, end)]
    if not edge.SameSense():
        a, b = b, a
    if curve.IsPeriodic():
        if b <= a:
            b += curve.Period()
    elif curve.IsClosed() and start.Distance(end) < 1e-9:
        a, b = curve.FirstParameter(), curve.LastParameter()
    if b < a:
        raise ValueError('non-periodic curve has reversed parameter interval')
    cuts = [a, b]
    if hasattr(curve, 'NbKnots'):
        cuts += [curve.Knot(i) for i in range(1, curve.NbKnots()+1) if a < curve.Knot(i) < b]
    cuts = sorted(set(cuts))
    def speed(t):
        p, d = gp_Pnt(), gp_Vec()
        curve.D1(t, p, d)
        return d.Magnitude()
    values = [math.fsum(quad(speed, lo, hi, epsabs=eps, epsrel=eps, limit=500)[0]
                        for lo, hi in zip(cuts, cuts[1:])) for eps in (1e-9, 1e-11)]
    return dict(length=values[-1], change=abs(values[-1]-values[0]), interval=[a, b])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--rust', type=Path, required=True)
    parser.add_argument('file', type=Path)
    parser.add_argument('faces', type=int, nargs='+')
    args = parser.parse_args()
    raw = args.file.read_bytes()
    if args.file.suffix == '.gz':
        raw = gzip.decompress(raw)
    # Strings and comments must not contribute spurious entity declarations.
    tokens = re.finditer(rb"'(?:[^']|'')*'|/\*.*?\*/|;\s*#(\d+)\s*=", raw, re.S)
    ids = [int(m[1]) for m in tokens if m[1]]
    reader = STEPControl_Reader()
    with tempfile.NamedTemporaryFile(suffix='.step') as temporary:
        temporary.write(raw)
        temporary.flush()
        if int(reader.ReadFile(temporary.name)) != 1:
            raise ValueError('OCCT did not read the STEP model')
    model = reader.StepModel()
    if len(ids) != model.NbEntities():
        raise ValueError('STEP text/model inventory mismatch')
    entities = {ident: model.Value(i+1) for i, ident in enumerate(ids)}
    rust = json.loads(subprocess.check_output(
        [str(args.rust.resolve()), str(args.file), *map(str, args.faces)], text=True))
    for face in rust['faces']:
        for edge in face['edges']:
            source = entities[edge['entity']]
            if source.DynamicType().Name() != 'StepShape_EdgeCurve':
                raise ValueError('source identity is not an EDGE_CURVE')
            edge['reference'] = edge_length(source)
            edge['error'] = edge['length']-edge['reference']['length']
        face['reference'] = math.fsum(e['reference']['length'] for e in face['edges'])
        face['error'] = face['perimeter']-face['reference']
    print(json.dumps(rust, indent=2, allow_nan=False))


if __name__ == '__main__':
    main()
