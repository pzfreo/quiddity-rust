"""Independent areas of corpus faces over the region their 3D edge curves bound: the evidence
behind the verdicts in ``tests/fixtures/known_face_areas.json``.

    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/face_area_evidence.py faces.json
    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/face_area_evidence.py --show FILE FACE...
    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/face_area_evidence.py --moments faces.json

``faces.json`` lists ``[file, face]`` pairs (face indices in OpenCascade's traversal, as in
``tests/fixtures/face_areas.json.gz``); their areas are merged into
``tests/fixtures/face_area_evidence.json.gz``. Nothing here shares code or method with the
port's ``mass.rs`` beyond the definition being computed: each loop's 3D edge curves are sampled
over their own parameter ranges (``BRepTools_WireExplorer`` order, each edge in its use's
sense, consecutive edges joined straight) and the samples placed on the surface (analytically
on planes, cylinders, cones, spheres and tori; by ``GeomAPI_ProjectPointOnSurf`` on B-spline
surfaces), giving a polygon in the surface's parameters. Its area integral is taken two ways:

- ``green``: Green's theorem along the polygon (Simpson on each side), the inner integral of the
  area density in closed form on analytic surfaces, and on B-spline surfaces tabulated on a
  600 x 600 Gauss grid and interpolated by a bicubic spline;
- ``slices`` (B-spline faces, on request with ``--slices``): the density integrated directly,
  in slices across u broken at every polygon vertex (a 3-point Gauss rule per slice, the
  crossings of the slice paired even-odd), and along each slice by an 8-point Gauss rule per
  knot span of the surface; no tabulation, no Green's theorem.

- ``moments`` (on request with ``--moments``, for ``tools/known_face_moments.py``): the area
  and centroid, by ``tools/kernel_evidence.py``'s Green's theorem along the edges' foot points
  (high-order panels on the edge curves, no polygon) with the densities w and w·p, at 8 and 16
  panels per curve interval; ``moments_change`` is the largest difference between the two, scaled
  as ``crates/haecceity/tests/face_moments.rs`` compares moments.

The slices take minutes on the largest faces (the recorded evidence omits them on nine
cgb243 faces, run without ``--slices``). Each is taken with 2000 and 4000 samples per edge and extrapolated (the polygon's chords are
off the curve by the square of the spacing). A face whose boundary is not one simple polygon in
these parameters (a loop running round a closed B-spline surface) gets ``null``.
"""

from __future__ import annotations

import gzip
import json
import math
import os
import subprocess
import sys
import tempfile
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

import numpy as np
from scipy.interpolate import RectBivariateSpline

QUIDDITY = Path(os.environ.get("QUIDDITY", "../quiddity")).resolve()
sys.path[:0] = [str(QUIDDITY / "src")]

from OCP.BRep import BRep_Tool  # noqa: E402
from OCP.BRepAdaptor import BRepAdaptor_Curve, BRepAdaptor_Surface  # noqa: E402
from OCP.BRepTools import BRepTools_WireExplorer  # noqa: E402
from OCP.GeomAbs import (  # noqa: E402
    GeomAbs_BSplineSurface,
    GeomAbs_Cone,
    GeomAbs_Cylinder,
    GeomAbs_Plane,
    GeomAbs_Sphere,
    GeomAbs_Torus,
)
from OCP.GeomAPI import GeomAPI_ProjectPointOnSurf  # noqa: E402
from OCP.gp import gp_Pnt, gp_Vec  # noqa: E402
from OCP.TopAbs import TopAbs_REVERSED, TopAbs_WIRE  # noqa: E402
from OCP.TopExp import TopExp_Explorer  # noqa: E402
from OCP.TopoDS import TopoDS  # noqa: E402

from quiddity import import_step_geometry  # noqa: E402

CORPUS = QUIDDITY / "tests" / "corpus"
FIXTURES = Path(__file__).resolve().parent.parent / "tests" / "fixtures"
OUT = FIXTURES / "face_area_evidence.json.gz"
TAU = 2 * math.pi
GRID = 600
GAUSS8 = np.polynomial.legendre.leggauss(8)
GAUSS3 = np.polynomial.legendre.leggauss(3)


def _load(name: str):
    path = CORPUS / name
    if path.suffix != ".gz":
        return import_step_geometry(str(path))
    with tempfile.NamedTemporaryFile(suffix=".step") as tmp:
        tmp.write(gzip.decompress(path.read_bytes()))
        tmp.flush()
        return import_step_geometry(tmp.name)


def _analytic(surf):
    """(parameters of a point, G(u, v) = ∫ density du, H(u, v) = ∫ density dv, v periodic), or
    None on a B-spline surface."""

    def frame(ax):
        return ax.Location().XYZ(), ax.XDirection().XYZ(), ax.YDirection().XYZ(), ax.Direction().XYZ()

    kind = surf.GetType()
    if kind == GeomAbs_Plane:
        o, x, y, _ = frame(surf.Plane().Position())

        def param(p):
            d = p.XYZ() - o
            return d.Dot(x), d.Dot(y)

        return param, (lambda u, v: u), (lambda u, v: v), False
    if kind == GeomAbs_Cylinder:
        c = surf.Cylinder()
        o, x, y, z = frame(c.Position())
        r = c.Radius()

        def param(p):
            d = p.XYZ() - o
            return math.atan2(d.Dot(y), d.Dot(x)), d.Dot(z)

        return param, (lambda u, v: r * u), (lambda u, v: r * v), False
    if kind == GeomAbs_Cone:
        c = surf.Cone()
        o, x, y, z = frame(c.Position())
        r, a = c.RefRadius(), c.SemiAngle()
        sa, ca = math.sin(a), math.cos(a)

        def param(p):
            d = p.XYZ() - o
            rho = math.hypot(d.Dot(x), d.Dot(y))
            return math.atan2(d.Dot(y), d.Dot(x)), (rho - r) * sa + d.Dot(z) * ca

        return param, (lambda u, v: u * abs(r + v * sa)), (lambda u, v: r * v + 0.5 * sa * v * v), False
    if kind == GeomAbs_Sphere:
        s = surf.Sphere()
        o, x, y, z = frame(s.Position())
        big = s.Radius() ** 2

        def param(p):
            d = p.XYZ() - o
            rho = math.hypot(d.Dot(x), d.Dot(y))
            return math.atan2(d.Dot(y), d.Dot(x)), math.atan2(d.Dot(z), rho)

        return param, (lambda u, v: big * u * math.cos(v)), (lambda u, v: big * math.sin(v)), False
    if kind == GeomAbs_Torus:
        t = surf.Torus()
        o, x, y, z = frame(t.Position())
        major, minor = t.MajorRadius(), t.MinorRadius()

        def param(p):
            d = p.XYZ() - o
            rho = math.hypot(d.Dot(x), d.Dot(y))
            return math.atan2(d.Dot(y), d.Dot(x)), math.atan2(d.Dot(z), rho - major)

        return (
            param,
            (lambda u, v: minor * u * (major + minor * math.cos(v))),
            (lambda u, v: minor * (major * v + minor * math.sin(v))),
            True,
        )
    return None


def _loops(face, samples: int, place):
    """Each wire's boundary as a list of parameter points, or None where a sample is not placed."""

    loops = []
    wires = TopExp_Explorer(face, TopAbs_WIRE)
    while wires.More():
        points = []
        edges = BRepTools_WireExplorer(TopoDS.Wire_s(wires.Current()), face)
        while edges.More():
            edge = edges.Current()
            if not BRep_Tool.Degenerated_s(edge):
                curve = BRepAdaptor_Curve(edge)
                a, b = curve.FirstParameter(), curve.LastParameter()
                ts = [a + (b - a) * k / samples for k in range(samples + 1)]
                if edge.Orientation() == TopAbs_REVERSED:
                    ts.reverse()
                for t in ts:
                    uv = place(curve.Value(t))
                    if uv is None:
                        return None
                    points.append(uv)
            edges.Next()
        loops.append(points)
        wires.Next()
    return loops


def _density(surf, u: float, v: float) -> float:
    if not isinstance(surf, BRepAdaptor_Surface):
        u, v, _ = _wrap(surf, u, v)
    p, du, dv = gp_Pnt(), gp_Vec(), gp_Vec()
    surf.D1(u, v, p, du, dv)
    return du.Crossed(dv).Magnitude()


def _green_analytic(face, samples: int):
    surf = BRepAdaptor_Surface(face)
    param, g, h, v_periodic = _analytic(surf)
    loops = []
    for points in _loops(face, samples, param):
        out = [points[0]]
        for u, v in points[1:]:
            pu, pv = out[-1]
            u -= TAU * round((u - pu) / TAU)
            if v_periodic:
                v -= TAU * round((v - pv) / TAU)
            out.append((u, v))
        loops.append(out)
    # A loop running round u encloses no area in u, v: integrate H du instead of G dv (the
    # bands between such loops close through the seam, where G jumps and H does not).
    winds = any(abs(pts[-1][0] - pts[0][0]) > 1.0 for pts in loops)
    total = 0.0
    for pts in loops:
        for (u0, v0), (u1, v1) in zip(pts, pts[1:] + pts[:1]):
            um, vm = 0.5 * (u0 + u1), 0.5 * (v0 + v1)
            if winds:
                total -= (h(u0, v0) + 4 * h(um, vm) + h(u1, v1)) / 6 * (u1 - u0)
            else:
                total += (g(u0, v0) + 4 * g(um, vm) + g(u1, v1)) / 6 * (v1 - v0)
    return abs(total)


def _bspline_loops(face, samples: int):
    surf = BRep_Tool.Surface_s(face)
    project = GeomAPI_ProjectPointOnSurf()

    def place(p):
        project.Init(p, surf)
        if project.NbPoints() == 0:
            return None
        return project.LowerDistanceParameters()

    loops = _loops(face, samples, place)
    if loops is None:
        return surf, None
    u0, u1, v0, v1 = surf.Bounds()
    # Round a closed surface each point takes the parameters a whole span round that lie beside
    # the one before (a face's own seam need not be the surface's).
    spans = (u1 - u0 if surf.IsUClosed() else None, v1 - v0 if surf.IsVClosed() else None)
    for pts in loops:
        for k in range(1, len(pts)):
            (pu, pv), (u, v) = pts[k - 1], pts[k]
            if spans[0]:
                u -= spans[0] * round((u - pu) / spans[0])
            if spans[1]:
                v -= spans[1] * round((v - pv) / spans[1])
            pts[k] = (u, v)
    # A loop that ends far from where it began runs round a closed surface (elsewhere, its
    # closing side is a collapsed side of the domain, or a degenerate edge, which is skipped).
    for pts in loops:
        if (surf.IsUClosed() and abs(pts[-1][0] - pts[0][0]) > 0.5 * (u1 - u0)) or (
            surf.IsVClosed() and abs(pts[-1][1] - pts[0][1]) > 0.5 * (v1 - v0)
        ):
            return surf, None
    return surf, loops


def _wrap(surf, u: float, v: float) -> tuple[float, float, int]:
    """(u, v) brought into a closed surface's domain, and how many u spans that took."""

    u0, u1, v0, v1 = surf.Bounds()
    turns = 0
    if surf.IsUClosed() and not u0 <= u <= u1:
        turns = math.floor((u - u0) / (u1 - u0))
        u -= turns * (u1 - u0)
    if surf.IsVClosed() and not v0 <= v <= v1:
        v -= math.floor((v - v0) / (v1 - v0)) * (v1 - v0)
    return u, v, turns


def _collapsed_u(surf):
    """The u of a side of the domain that collapses to a point, if any (the inner integral of
    Green's theorem starts there, where the parameter v is undetermined)."""

    u0, u1, v0, v1 = surf.Bounds()
    for side in (u0, u1):
        p = surf.Value(side, v0)
        if all(surf.Value(side, v0 + (v1 - v0) * k / 8).Distance(p) < 1e-9 for k in range(9)):
            return side
    return None


def _green_bspline(face, samples: int, table=None):
    surf, loops = _bspline_loops(face, samples)
    if loops is None:
        return None, table
    if table is None:
        u0, u1, v0, v1 = surf.Bounds()
        us, vs = np.linspace(u0, u1, GRID + 1), np.linspace(v0, v1, GRID + 1)
        # g[i, j] = ∫ from u0 to us[i] of the density at vs[j].
        g = np.zeros((GRID + 1, GRID + 1))
        xs, ws = GAUSS8
        for j, v in enumerate(vs):
            acc = 0.0
            for i in range(GRID):
                half, mid = 0.5 * (us[i + 1] - us[i]), 0.5 * (us[i + 1] + us[i])
                acc += half * sum(w * _density(surf, mid + half * x, v) for x, w in zip(xs, ws))
                g[i + 1, j] = acc
        table = RectBivariateSpline(us, vs, g, kx=3, ky=3)
    ref = _collapsed_u(surf)

    u_end = surf.Bounds()[1]

    def inner(u, v):
        # Each whole span round adds the integral across the domain.
        u, v, turns = _wrap(surf, u, v)
        value = float(table(u, v)[0, 0]) + turns * float(table(u_end, v)[0, 0])
        return value - float(table(ref, v)[0, 0]) if ref is not None else value

    total = 0.0
    for pts in loops:
        for (ua, va), (ub, vb) in zip(pts, pts[1:] + pts[:1]):
            mid = inner(0.5 * (ua + ub), 0.5 * (va + vb))
            total += (inner(ua, va) + 4 * mid + inner(ub, vb)) / 6 * (vb - va)
    return abs(total), table


def _breaks(knots, lo, hi):
    inside = sorted({k for k in knots if lo < k < hi})
    return [lo, *inside, hi]


def _slices(face, samples: int):
    surf, loops = _bspline_loops(face, samples)
    if loops is None:
        return None
    knots_v = [surf.VKnot(k) for k in range(1, surf.NbVKnots() + 1)]
    segments = np.array(
        [(*a, *b) for pts in loops for a, b in zip(pts, pts[1:] + pts[:1]) if a[0] != b[0]]
    )
    ua, va, ub, vb = segments.T
    lo, hi = np.minimum(ua, ub), np.maximum(ua, ub)
    cuts = np.unique(np.concatenate([ua, ub]))
    xs3, ws3 = GAUSS3
    xs8, ws8 = GAUSS8
    total = 0.0
    for start in range(0, len(cuts) - 1, 256):
        a, b = cuts[start : start + 257][:-1], cuts[start + 1 : start + 257]
        mids = 0.5 * (a + b)
        # Between consecutive vertices the same sides cross every slice.
        active = (lo[None, :] <= mids[:, None]) & (mids[:, None] < hi[None, :])
        for k in range(len(mids)):
            half = 0.5 * (b[k] - a[k])
            sides = np.nonzero(active[k])[0]
            if len(sides) % 2:
                return None
            for x, w in zip(xs3, ws3):
                u = mids[k] + half * x
                s = sides
                cross = np.sort(va[s] + (u - ua[s]) * (vb[s] - va[s]) / (ub[s] - ua[s]))
                for v_lo, v_hi in zip(cross[::2], cross[1::2]):
                    for p, q in zip(_breaks(knots_v, v_lo, v_hi)[:-1], _breaks(knots_v, v_lo, v_hi)[1:]):
                        h, c = 0.5 * (q - p), 0.5 * (q + p)
                        total += w * half * h * sum(ww * _density(surf, u, c + h * xx) for xx, ww in zip(xs8, ws8))
    return total


def _extrapolate(coarse, fine):
    if coarse is None or fine is None:
        return None
    return fine + (fine - coarse) / 3


def evidence(face, slices: bool) -> dict:
    surf = BRepAdaptor_Surface(face)
    if _analytic(surf) is not None:
        return {"green": _extrapolate(_green_analytic(face, 2000), _green_analytic(face, 4000))}
    if surf.GetType() != GeomAbs_BSplineSurface:
        return {}
    coarse, table = _green_bspline(face, 2000)
    fine, _ = _green_bspline(face, 4000, table)
    out = {"green": _extrapolate(coarse, fine)}
    if slices:
        out["slices"] = _extrapolate(_slices(face, 2000), _slices(face, 4000))
    return out


def moments(face, panels=(8, 16)) -> dict:
    """[area, cx, cy, cz] over the region the 3D edges bound, by ``tools/kernel_evidence.py``'s
    Green's theorem along the edges' foot points (OpenCascade's evaluator and projection, high-
    order panels; nothing from either kernel's integration or pcurves) with the densities w and
    w·p in place of the area and flux densities, at two panel counts: the finer one's moments and
    ``change``, the largest difference between the two scaled as the test compares them."""

    import kernel_evidence

    class Moments(kernel_evidence._Surface):
        # Two components per pass: kernel_evidence integrates pairs.
        pair = (0, 1)

        def densities(self, u, v):
            u, v = self._wrapped(u, v)
            p, du, dv = gp_Pnt(), gp_Vec(), gp_Vec()
            self.adaptor.D1(u, v, p, du, dv)
            w = du.Crossed(dv).Magnitude()
            values = (w, w * p.X(), w * p.Y(), w * p.Z())
            return np.array([values[k] for k in self.pair])

    surf = Moments(face)
    found = []
    for n in panels:
        signed = []
        for pair in ((0, 1), (2, 3)):
            surf.pair = pair
            pass_ = kernel_evidence._integrate(face, surf, n)
            if pass_ is None:
                return {"moments": None}
            signed += list(pass_)
        area = abs(signed[0])
        found.append([area] + [s / signed[0] for s in signed[1:]])
    coarse, fine = found
    change = max(
        [abs(fine[0] - coarse[0]) / fine[0]] + [abs(fine[k] - coarse[k]) / max(abs(fine[k]), 1.0) for k in (1, 2, 3)]
    )
    return {"moments": fine, "change": change}


def _work(job):
    file, faces, slices, with_moments = job
    all_faces = list(_load(file).faces())
    out = {}
    for i in faces:
        try:
            if with_moments:
                found = moments(all_faces[i].wrapped)
                out[f"{file}#{i}"] = {"moments": found["moments"], "moments_change": found.get("change")}
            else:
                out[f"{file}#{i}"] = evidence(all_faces[i].wrapped, slices)
        except Exception as error:  # noqa: BLE001 - recorded as no evidence
            print("failed", file, i, error, file=sys.stderr)
            out[f"{file}#{i}"] = {}
        print(file, i, out[f"{file}#{i}"], file=sys.stderr, flush=True)
    return out


def main() -> None:
    slices = "--slices" in sys.argv
    with_moments = "--moments" in sys.argv
    args = [a for a in sys.argv[1:] if a not in ("--slices", "--moments")]
    if args[0] == "--show":
        print(json.dumps(_work((args[1], [int(i) for i in args[2:]], slices, with_moments)), indent=1))
        return
    pairs = json.loads(Path(args[0]).read_text())
    jobs: dict[str, list[int]] = {}
    for file, face in pairs:
        jobs.setdefault(file, []).append(face)
    # The moments take minutes on a large B-spline face: one face per job.
    size = 1 if with_moments else 4
    chunks = [
        (f, faces[k : k + size], slices, with_moments)
        for f, faces in jobs.items()
        for k in range(0, len(faces), size)
    ]
    found = json.loads(gzip.decompress(OUT.read_bytes())) if OUT.exists() else {"faces": {}}
    found["quiddity"] = subprocess.run(
        ["git", "-C", str(QUIDDITY), "rev-parse", "HEAD"], capture_output=True, text=True, check=True
    ).stdout.strip()
    # Written after each chunk, so an interrupted run keeps what it has.
    with ProcessPoolExecutor(max_workers=os.cpu_count()) as pool:
        for out in pool.map(_work, chunks):
            for key, value in out.items():
                found["faces"].setdefault(key, {}).update(value)
            found["faces"] = dict(sorted(found["faces"].items()))
            text = json.dumps(found, indent=0, allow_nan=False) + "\n"
            OUT.write_bytes(gzip.compress(text.encode(), mtime=0))


if __name__ == "__main__":
    main()
