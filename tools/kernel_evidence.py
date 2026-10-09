"""Independent face areas and fluxes, and solid masses built from them: the evidence behind the
kernel verdicts in ``tests/fixtures/known_divergences.json`` (solid masses) and the B-spline
faces of ``tests/fixtures/known_face_areas.json`` that ``tools/face_area_evidence.py`` left
undetermined.

    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/kernel_evidence.py faces FILE FACE...
    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/kernel_evidence.py solids FILE...
    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/kernel_evidence.py uv FILE FACE...
    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/kernel_evidence.py boxes FILE FACE...
    QUIDDITY=../quiddity ../quiddity/.venv/bin/python tools/kernel_evidence.py arcs FILE A-B...

``FILE`` is a corpus path (``nist/nist_ctc_05_asme1_rd.stp``); faces are indexed in
OpenCascade's traversal, as the port's are. Output is JSON on stdout; nothing is written.

``faces`` integrates the area density |S_u x S_v| and the flux density (S - c) . (S_u x S_v) of
each face's exact surface (OpenCascade's evaluator, which only evaluates the file's surface)
over the region the face's 3D edge curves bound, by Green's theorem: the integral of G dv round
each loop, G the integral of the density along u from a fixed reference (or of H du, H along v,
when the loops run round the surface in u). Nothing comes from either kernel's integration or
pcurves: each edge is cut at its curve's own breaks into panels of 12 Gauss-Lobatto points, each
point is placed on the surface (in closed form on planes, cylinders, cones, spheres and tori; by
``GeomAPI_ProjectPointOnSurf`` otherwise), and dv along the path is the derivative of the
interpolating polynomial of the placed points (so the path is the edge's foot points, not a
polygon). Consecutive edges are joined straight in the parameters (a degenerate edge's pcurve,
or an edge's gap within its tolerance). The inner integral is 16-point Gauss-Legendre over each
smoothness interval of the surface, cut to at most 0.25 in an angular parameter. Every face is
integrated with 8 and with 16 panels per curve interval; ``change`` is the difference, an upper
bound on the error where the path is smooth. ``c`` is ``--centre x y z`` (default: the mean of
the face's solid's vertex uses, as the port's ``solid_mass`` takes it), and
``volume`` is a third of the outward flux (the face's orientation applied). Where a parameter is
arbitrary (at a pole, an apex or a B-spline side collapsed to a point) a placed point takes the
nearest regular sample's value of it, the inner integral starts from a collapsed side, and a loop
through a pole takes the turn across it that closes the loop (the density vanishes there). A face
whose loops run round the surface without closing (a lone band, a cap round a pole) gets ``null``.
``KERNEL_EVIDENCE_PANELS="32 64"`` replaces the two panel counts, for a face whose change is too
large to decide.

``solids`` gives, per solid, OpenCascade's adaptive volume and area (``VolumeProperties_s`` and
``SurfaceProperties_s`` with Eps 1e-10 and 1e-12; these integrate the pcurves), and the sums
over its faces of the ``faces`` integrals about the solid's vertex-use centre: a volume and area
that share no step with either kernel's integration. With ``--faces`` it also lists every face.

``uv`` gives the parameter range each face's 3D edges reach (``uv_extent``), and ``arcs`` the fold
between two faces along their shared edges, to first and second order (``arc_reading``).
``boxes`` gives, per edge of each face, the extent of its 3D curve and of the curve's foot points
on the face's surface along each axis, with how far the curve strays from the surface and the
edge's and face's tolerances (``edge_boxes``): where an edge strays, which one bounds the face.

Verdicts on solid masses and body keys built from these follow the policy the divergence
verdicts set (whose threshold is an open maintainer question, docs/status-2026-10-08.md):
rust-correct where the port matches the independent sums to about 1e-9 relative with no known
face defect in ``tests/fixtures/known_face_areas.json``; rust-wrong where a known face defect
shifts the masses.
"""

from __future__ import annotations

import gzip
import json
import math
import os
import sys
import tempfile
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

import numpy as np

QUIDDITY = Path(os.environ.get("QUIDDITY", "../quiddity")).resolve()
sys.path[:0] = [str(QUIDDITY / "src"), str(Path(__file__).resolve().parent)]

from OCP.BRep import BRep_Tool  # noqa: E402
from OCP.BRepAdaptor import BRepAdaptor_Curve, BRepAdaptor_Surface  # noqa: E402
from OCP.BRepGProp import BRepGProp  # noqa: E402
from OCP.BRepTools import BRepTools_WireExplorer  # noqa: E402
from OCP.GeomAbs import GeomAbs_CN, GeomAbs_Cone, GeomAbs_Cylinder, GeomAbs_Plane  # noqa: E402
from OCP.GeomAbs import GeomAbs_Sphere, GeomAbs_Torus  # noqa: E402
from OCP.GeomAPI import GeomAPI_ProjectPointOnSurf  # noqa: E402
from OCP.GProp import GProp_GProps  # noqa: E402
from OCP.gp import gp_Pnt, gp_Vec  # noqa: E402
from OCP.TColStd import TColStd_Array1OfReal  # noqa: E402
from OCP.TopAbs import TopAbs_EDGE, TopAbs_FACE, TopAbs_REVERSED, TopAbs_WIRE  # noqa: E402
from OCP.TopExp import TopExp_Explorer  # noqa: E402
from OCP.TopoDS import TopoDS  # noqa: E402

from face_area_evidence import _analytic  # noqa: E402
from quiddity import import_step_geometry  # noqa: E402

CORPUS = QUIDDITY / "tests" / "corpus"
TAU = 2 * math.pi
LOBATTO_N = 12
# The two panel counts per curve interval each face is integrated with (``change`` compares
# them); ``KERNEL_EVIDENCE_PANELS="16 32"`` refines a face whose change is too large to decide.
PANELS = tuple(int(n) for n in os.environ.get("KERNEL_EVIDENCE_PANELS", "8 16").split())
GL16 = np.polynomial.legendre.leggauss(16)
GL8 = np.polynomial.legendre.leggauss(8)
ANGULAR_STEP = 0.25


def _lobatto(n: int):
    """Gauss-Lobatto nodes and weights on [-1, 1] and the differentiation matrix at the nodes."""

    legendre = np.polynomial.legendre.Legendre.basis(n - 1)
    x = np.concatenate([[-1.0], np.sort(legendre.deriv().roots().real), [1.0]])
    w = 2.0 / (n * (n - 1) * legendre(x) ** 2)
    bary = np.array([1.0 / np.prod([x[j] - x[k] for k in range(n) if k != j]) for j in range(n)])
    d = np.zeros((n, n))
    for i in range(n):
        for j in range(n):
            if i != j:
                d[i, j] = bary[j] / bary[i] / (x[i] - x[j])
        d[i, i] = -d[i].sum()
    return x, w, d


LOB_X, LOB_W, LOB_D = _lobatto(LOBATTO_N)


def _load(name: str):
    path = CORPUS / name
    if path.suffix != ".gz":
        return import_step_geometry(str(path))
    with tempfile.NamedTemporaryFile(suffix=".step") as tmp:
        tmp.write(gzip.decompress(path.read_bytes()))
        tmp.flush()
        return import_step_geometry(tmp.name)


def _intervals(n: int, fill) -> list[float]:
    arr = TColStd_Array1OfReal(1, n + 1)
    fill(arr, GeomAbs_CN)
    return [arr.Value(k) for k in range(1, n + 2)]


class _Surface:
    """A face's surface: placing points, evaluating densities, and its smoothness breaks."""

    def __init__(self, face):
        self.adaptor = BRepAdaptor_Surface(face)
        kind = self.adaptor.GetType()
        analytic = _analytic(self.adaptor)
        self.place_closed_form = analytic[0] if analytic else None
        self.angular = (
            kind in (GeomAbs_Cylinder, GeomAbs_Cone, GeomAbs_Sphere, GeomAbs_Torus),
            kind in (GeomAbs_Sphere, GeomAbs_Torus),
        )
        a = self.adaptor
        self.bounds = (a.FirstUParameter(), a.LastUParameter(), a.FirstVParameter(), a.LastVParameter())
        if analytic:
            self.periods = (TAU if self.angular[0] else None, TAU if kind == GeomAbs_Torus else None)
            self.wrap = (False, False)
            self.breaks = ([], [])
        else:
            self.periods = (
                self.bounds[1] - self.bounds[0] if a.IsUClosed() else None,
                self.bounds[3] - self.bounds[2] if a.IsVClosed() else None,
            )
            self.wrap = (a.IsUClosed(), a.IsVClosed())
            self.breaks = (
                _intervals(a.NbUIntervals(GeomAbs_CN), a.UIntervals),
                _intervals(a.NbVIntervals(GeomAbs_CN), a.VIntervals),
            )
            self.project = GeomAPI_ProjectPointOnSurf()
            self.geom = BRep_Tool.Surface_s(face)
        # Sides of the domain that collapse to a point (u = c for each v, or v = c for each u):
        # there the other parameter is arbitrary.
        self.collapsed = ([], [])
        if not analytic:
            u0, u1, v0, v1 = self.bounds
            for axis, sides, span in ((0, (u0, u1), (v0, v1)), (1, (v0, v1), (u0, u1))):
                for side in sides:
                    at = (lambda s: a.Value(side, s)) if axis == 0 else (lambda s: a.Value(s, side))
                    p = at(span[0])
                    if all(at(span[0] + (span[1] - span[0]) * k / 8).Distance(p) < 1e-9 for k in range(9)):
                        self.collapsed[axis].append(side)
        self.centre = np.zeros(3)

    def place(self, p: gp_Pnt):
        if self.place_closed_form:
            return self.place_closed_form(p)
        self.project.Init(p, self.geom)
        if self.project.NbPoints() == 0:
            raise ValueError("a boundary point does not project onto the surface")
        return self.project.LowerDistanceParameters()

    def arbitrary(self, u: float, v: float) -> tuple[bool, bool]:
        """Whether u (and v) is arbitrary at the point: its partial derivative vanishes there."""

        u, v = self._wrapped(u, v)
        p, du, dv = gp_Pnt(), gp_Vec(), gp_Vec()
        self.adaptor.D1(u, v, p, du, dv)
        a, b = du.Magnitude(), dv.Magnitude()
        return a <= 1e-9 * b, b <= 1e-9 * a

    def _wrapped(self, u: float, v: float):
        u0, u1, v0, v1 = self.bounds
        if self.wrap[0]:
            u = u0 + (u - u0) % (u1 - u0)
        if self.wrap[1]:
            v = v0 + (v - v0) % (v1 - v0)
        return u, v

    def densities(self, u: float, v: float) -> np.ndarray:
        """Area density and flux density about ``centre`` along the natural normal."""

        u, v = self._wrapped(u, v)
        p, du, dv = gp_Pnt(), gp_Vec(), gp_Vec()
        self.adaptor.D1(u, v, p, du, dv)
        n = du.Crossed(dv)
        r = np.array([p.X(), p.Y(), p.Z()]) - self.centre
        return np.array([n.Magnitude(), r[0] * n.X() + r[1] * n.Y() + r[2] * n.Z()])

    def inner(self, axis: int, fixed: float, lo: float, hi: float) -> np.ndarray:
        """The densities integrated along parameter ``axis`` from ``lo`` to ``hi``, the other
        parameter held at ``fixed``."""

        if lo == hi:
            return np.zeros(2)
        a, b = min(lo, hi), max(lo, hi)
        cuts = {a, b}
        period = self.periods[axis] if self.wrap[axis] else None
        for k in self.breaks[axis]:
            if period:
                first = math.ceil((a - k) / period)
                for m in range(first, first + int((b - a) / period) + 2):
                    if a < k + m * period < b:
                        cuts.add(k + m * period)
            elif a < k < b:
                cuts.add(k)
        if self.angular[axis]:
            steps = math.ceil((b - a) / ANGULAR_STEP)
            cuts.update(a + (b - a) * j / steps for j in range(1, steps))
        cuts = sorted(cuts)
        xs, ws = GL16
        total = np.zeros(2)
        for p, q in zip(cuts, cuts[1:]):
            h, m = 0.5 * (q - p), 0.5 * (q + p)
            for x, w in zip(xs, ws):
                s = m + h * x
                total += w * h * (self.densities(s, fixed) if axis == 0 else self.densities(fixed, s))
        return total if hi >= lo else -total


def _loop_paths(face, surf: _Surface, panels: int):
    """Each wire as a list of pieces in the parameters: ``('edge', u[], v[])`` for a panel of
    Lobatto points, ``('join', (u0, v0), (u1, v1))`` for a straight join, all unwrapped."""

    loops = []
    wires = TopExp_Explorer(face, TopAbs_WIRE)
    while wires.More():
        pieces, last, first = [], None, None
        edges = BRepTools_WireExplorer(TopoDS.Wire_s(wires.Current()), face)
        while edges.More():
            edge = edges.Current()
            edges.Next()
            if BRep_Tool.Degenerated_s(edge):
                # A pole or apex: marked by a join of no length, where the loop may change turn.
                if last is not None:
                    pieces.append(("join", last, last))
                continue
            curve = BRepAdaptor_Curve(edge)
            cuts = _intervals(curve.NbIntervals(GeomAbs_CN), curve.Intervals)
            ts = []
            for a, b in zip(cuts, cuts[1:]):
                ts += [(a + (b - a) * k / panels, a + (b - a) * (k + 1) / panels) for k in range(panels)]
            if edge.Orientation() == TopAbs_REVERSED:
                ts = [(b, a) for a, b in reversed(ts)]
            for a, b in ts:
                raw = [surf.place(curve.Value(0.5 * (a + b) + 0.5 * (b - a) * x)) for x in LOB_X]
                # At a pole or apex (or a B-spline side collapsed to a point) one parameter is
                # arbitrary: it takes the nearest regular sample's, so that it neither sets the
                # turn the rest of the loop unwraps against nor bends the path.
                arbitrary = [surf.arbitrary(u, v) for u, v in raw]
                for axis in (0, 1):
                    regular = [k for k in range(LOBATTO_N) if not arbitrary[k][axis]]
                    for k in range(LOBATTO_N):
                        if arbitrary[k][axis] and regular:
                            near = min(regular, key=lambda j: abs(j - k))
                            raw[k] = (raw[near][0], raw[k][1]) if axis == 0 else (raw[k][0], raw[near][1])
                us, vs = [], []
                for u, v in raw:
                    ref = (us[-1], vs[-1]) if us else last
                    if ref is not None:
                        if surf.periods[0]:
                            u -= surf.periods[0] * round((u - ref[0]) / surf.periods[0])
                        if surf.periods[1]:
                            v -= surf.periods[1] * round((v - ref[1]) / surf.periods[1])
                    us.append(u)
                    vs.append(v)
                if last is not None and (us[0], vs[0]) != last:
                    pieces.append(("join", last, (us[0], vs[0])))
                if first is None:
                    first = (us[0], vs[0])
                pieces.append(("edge", np.array(us), np.array(vs)))
                last = (us[-1], vs[-1])
        if last is None:
            wires.Next()
            continue
        # Across a join at a pole the turn the next edge takes is not set by continuity (u
        # changes by about half a turn there either way); a loop through a pole that would
        # otherwise wind takes the other turn from its first join at the pole, where the
        # density vanishes, so the lift closes.
        poles = [
            k
            for k, p in enumerate(pieces)
            if p[0] == "join" and surf.arbitrary(*p[1])[0] and surf.arbitrary(*p[2])[0]
        ]
        turns = round((last[0] - first[0]) / surf.periods[0]) if surf.periods[0] else 0
        if turns and surf.arbitrary(*last)[0] and surf.arbitrary(*first)[0]:
            # The loop closes at the pole or apex itself: straight back to where it began.
            pieces.append(("join", last, first))
            last, turns = first, 0
        if turns and poles:
            shift = turns * surf.periods[0]
            k = poles[0]
            pieces[k] = ("join", pieces[k][1], (pieces[k][2][0] - shift, pieces[k][2][1]))
            for j in range(k + 1, len(pieces)):
                p = pieces[j]
                if p[0] == "edge":
                    pieces[j] = ("edge", p[1] - shift, p[2])
                else:
                    pieces[j] = ("join", (p[1][0] - shift, p[1][1]), (p[2][0] - shift, p[2][1]))
            last = (last[0] - shift, last[1])
        loops.append((pieces, first, last))
        wires.Next()
    return loops


def _integrate(face, surf: _Surface, panels: int):
    """(signed area integral, signed flux integral) over the face's parameter region, or None."""

    loops = _loop_paths(face, surf, panels)
    windings = []
    for pieces, first, last in loops:
        turns = []
        for axis in (0, 1):
            period = surf.periods[axis]
            gap = last[axis] - first[axis]
            turns.append(round(gap / period) if period else 0)
        windings.append(turns)
    u_turns = sum(t[0] for t in windings)
    v_turns = sum(t[1] for t in windings)
    if u_turns or v_turns:
        return None
    # Close each loop; a loop running round u (a band's edge) closes through the seam, where
    # G jumps and H does not, so then integrate H du instead.
    axis = 1 if any(t[0] for t in windings) else 0
    if any(t[1] for t in windings) and axis == 1:
        return None
    other = 1 - axis
    # The inner integral starts from a collapsed side across it, where it vanishes whatever the
    # arbitrary parameter there, as it does from a sphere's pole.
    ref = surf.collapsed[axis][0] if surf.collapsed[axis] else loops[0][1][axis]

    def g(point):
        # G(u, v) = ∫ from ref along `axis` of the densities, the other parameter fixed.
        return surf.inner(axis, point[other], ref, point[axis])

    total = np.zeros(2)
    for pieces, first, last in loops:
        closing = list(pieces)
        shift = [0.0, 0.0]
        for k in (0, 1):
            if surf.periods[k]:
                shift[k] = surf.periods[k] * round((last[k] - first[k]) / surf.periods[k])
        if (first[0] + shift[0], first[1] + shift[1]) != last:
            closing.append(("join", last, (first[0] + shift[0], first[1] + shift[1])))
        for piece in closing:
            if piece[0] == "edge":
                _, us, vs = piece
                coords = (us, vs)
                d_other = LOB_D @ coords[other]
                for k in range(LOBATTO_N):
                    total += LOB_W[k] * g((us[k], vs[k])) * d_other[k]
            else:
                _, p, q = piece
                for x, w in zip(*GL8):
                    s = 0.5 * (1 + x)
                    point = (p[0] + (q[0] - p[0]) * s, p[1] + (q[1] - p[1]) * s)
                    total += 0.5 * w * g(point) * (q[other] - p[other])
    # ∬ f du dv = ∮ G dv = -∮ H du.
    return total if axis == 0 else -total


def face_terms(face, centre) -> dict:
    """Area, outward flux and volume contribution (flux / 3) about ``centre``, at both panel
    counts, of one face (``TopoDS_Face``, its orientation as the solid uses it)."""

    surf = _Surface(face)
    surf.centre = np.asarray(centre, dtype=float)
    found = []
    for panels in PANELS:
        signed = _integrate(face, surf, panels)
        if signed is None:
            return {"area": None, "volume": None}
        area, flux = signed
        # The region's orientation in the parameters is the area's sign; the outward normal is
        # the natural one unless the face is reversed.
        flux = flux * math.copysign(1.0, area)
        if face.Orientation() == TopAbs_REVERSED:
            flux = -flux
        found.append((abs(area), flux / 3.0))
    (a0, v0), (a1, v1) = found
    return {"area": a1, "volume": v1, "change": [abs(a1 - a0), abs(v1 - v0)]}


def vertex_centre(shape) -> list[float]:
    """The mean of the shape's vertex uses (each edge use's two vertices), as the port's
    ``solid_mass`` takes it: a degenerate edge at a pole or apex is not an edge of the port's
    loops and is skipped, a vertex loop (a wire of one degenerate edge) counts its vertex twice.
    Where OpenCascade's import changes the file's topology (splitting closed edges, adding seams:
    nist_ftc_07) its vertex uses are not the port's, and ``--centre`` gives the port's."""

    from OCP.TopExp import TopExp

    total, n = np.zeros(3), 0
    wires = TopExp_Explorer(shape, TopAbs_WIRE)
    while wires.More():
        edges = []
        explorer = TopExp_Explorer(wires.Current(), TopAbs_EDGE)
        while explorer.More():
            edges.append(TopoDS.Edge_s(explorer.Current()))
            explorer.Next()
        wires.Next()
        for edge in edges:
            if BRep_Tool.Degenerated_s(edge) and len(edges) > 1:
                continue
            for vertex in (TopExp.FirstVertex_s(edge), TopExp.LastVertex_s(edge)):
                total += BRep_Tool.Pnt_s(vertex).Coord()
                n += 1
    return list(total / max(n, 1))


def _faces_of(shape):
    out = []
    explorer = TopExp_Explorer(shape, TopAbs_FACE)
    while explorer.More():
        out.append(TopoDS.Face_s(explorer.Current()))
        explorer.Next()
    return out


def _face_job(job):
    file, indices, centre = job
    part = _load(file)
    faces = [f.wrapped for f in part.faces()]
    out = {}
    for i in indices:
        try:
            out[i] = face_terms(faces[i], centre)
        except Exception as error:  # noqa: BLE001 - recorded as no evidence
            out[i] = {"area": None, "volume": None, "error": str(error)}
    return out


def _run_faces(file: str, indices: list[int], centre) -> dict:
    # Each job reads the file once; a few jobs per worker keep them all busy.
    size = max(4, math.ceil(len(indices) / (3 * (os.cpu_count() or 1))))
    jobs = [(file, indices[k : k + size], centre) for k in range(0, len(indices), size)]
    found = {}
    with ProcessPoolExecutor(max_workers=os.cpu_count()) as pool:
        for out in pool.map(_face_job, jobs):
            found.update(out)
    return found


def solids(file: str, list_faces: bool, centre=None) -> list[dict]:
    part = _load(file)
    all_faces = [f.wrapped for f in part.faces()]
    out = []
    for solid in [s.wrapped for s in part.solids()]:
        entry = {}
        for eps in (1e-10, 1e-12):
            v, a = GProp_GProps(), GProp_GProps()
            BRepGProp.VolumeProperties_s(solid, v, eps)
            BRepGProp.SurfaceProperties_s(solid, a, eps)
            entry[f"occ_adaptive_{eps:g}"] = [v.Mass(), a.Mass()]
        about = centre or vertex_centre(solid)
        uses = _faces_of(solid)
        indices = [next(k for k, f in enumerate(all_faces) if f.IsSame(u)) for u in uses]
        # The face as this solid uses it (its orientation in the shell).
        found = _run_faces(file, indices, about)
        missing = [i for i in indices if found[i]["area"] is None]
        entry["centre"] = about
        entry["independent"] = (
            None
            if missing
            else [sum(found[i]["volume"] for i in indices), sum(found[i]["area"] for i in indices)]
        )
        entry["independent_change"] = None if missing else [
            sum(found[i]["change"][1] for i in indices),
            sum(found[i]["change"][0] for i in indices),
        ]
        entry["unplaced_faces"] = missing
        if list_faces:
            entry["faces"] = {str(i): found[i] for i in indices}
        out.append(entry)
    return out


def uv_extent(face) -> dict:
    """The parameter range the face's 3D edges reach once placed on the surface: each edge
    sampled at 2001 points and every sampled extreme refined by a bounded Brent search over the
    edge's parameter, with how far off the surface that extreme point lies (B-spline surfaces).
    Samples at a side of the domain that collapses to a point (a pole) do not bound the parameter
    along that side; the edges' unwrapped longitudes are not reconciled across turns."""

    from scipy.optimize import minimize_scalar

    surf = _Surface(face)
    collapsed = []
    if surf.place_closed_form is None:
        u0, u1, v0, v1 = surf.bounds
        a = surf.adaptor
        for axis, sides, span in ((1, (u0, u1), (v0, v1)), (0, (v0, v1), (u0, u1))):
            for side in sides:
                at = (lambda s: a.Value(side, s)) if axis == 1 else (lambda s: a.Value(s, side))
                p = at(span[0])
                if all(at(span[0] + (span[1] - span[0]) * k / 8).Distance(p) < 1e-9 for k in range(9)):
                    collapsed.append((axis, p))
    elif surf.adaptor.GetType() == GeomAbs_Sphere:
        # The poles, where the longitude is arbitrary.
        collapsed = [(0, surf.adaptor.Value(0.0, h)) for h in (-0.5 * math.pi, 0.5 * math.pi)]
    found = {}
    edges = TopExp_Explorer(face, TopAbs_EDGE)
    while edges.More():
        edge = TopoDS.Edge_s(edges.Current())
        edges.Next()
        if BRep_Tool.Degenerated_s(edge):
            continue
        curve = BRepAdaptor_Curve(edge)
        a, b = curve.FirstParameter(), curve.LastParameter()
        ts = np.linspace(a, b, 2001)
        uvs = np.array([surf.place(curve.Value(t)) for t in ts])
        # At a side of the domain that collapses to a point (a B-spline surface's pole) the
        # parameter along that side is arbitrary: such samples do not bound it.
        masked = np.zeros((len(ts), 2), dtype=bool)
        for axis, point in collapsed:
            for k, t in enumerate(ts):
                masked[k, axis] |= curve.Value(t).Distance(point) < 1e-6
        for axis in (0, 1):
            for sense in (1, -1):
                key = ("u", "v")[axis] + ("max" if sense > 0 else "min")
                if masked[:, axis].all():
                    continue
                k = int(np.argmax(np.where(masked[:, axis], -np.inf, sense * uvs[:, axis])))
                lo = ts[k - 1] if k > 0 and not masked[k - 1, axis] else ts[k]
                hi = ts[k + 1] if k + 1 < len(ts) and not masked[k + 1, axis] else ts[k]
                best = minimize_scalar(
                    lambda t: -sense * surf.place(curve.Value(t))[axis],
                    bounds=(lo, hi),
                    method="bounded",
                    options={"xatol": 1e-14 * max(1.0, abs(b - a))},
                )
                t = best.x if lo < hi and -best.fun * sense > uvs[k, axis] * sense else ts[k]
                p = curve.Value(t)
                u, v = surf.place(p)
                value = (u, v)[axis]
                gap = None
                if surf.place_closed_form is None:
                    gap = surf.project.LowerDistance()
                if key not in found or sense * value > sense * found[key]["value"]:
                    found[key] = {"value": value, "t": t, "point": [p.X(), p.Y(), p.Z()], "off_surface": gap}
    return found


def edge_boxes(face) -> dict:
    """Per edge (2001 samples of its 3D curve): the curve's extent along x, y and z, its foot
    points' extent on the face's surface, its largest stray from the surface and its tolerance."""

    surf = _Surface(face)
    edges = []
    explorer = TopExp_Explorer(face, TopAbs_EDGE)
    while explorer.More():
        edge = TopoDS.Edge_s(explorer.Current())
        explorer.Next()
        if BRep_Tool.Degenerated_s(edge):
            continue
        curve = BRepAdaptor_Curve(edge)
        raw, feet, stray = [], [], 0.0
        for t in np.linspace(curve.FirstParameter(), curve.LastParameter(), 2001):
            p = curve.Value(t)
            q = surf.adaptor.Value(*surf._wrapped(*surf.place(p)))
            raw.append(p.Coord())
            feet.append(q.Coord())
            stray = max(stray, p.Distance(q))
        raw, feet = np.array(raw), np.array(feet)
        edges.append({
            "curve": [list(raw.min(axis=0)), list(raw.max(axis=0))],
            "feet": [list(feet.min(axis=0)), list(feet.max(axis=0))],
            "stray": stray,
            "tolerance": BRep_Tool.Tolerance_s(edge),
        })
    return {"tolerance": BRep_Tool.Tolerance_s(face), "edges": edges}


def arc_reading(shape, a: int, b: int, steps=(1e-3, 1e-2)) -> list[dict]:
    """The fold between faces ``a`` and ``b`` read without either kernel's walk direction, at 9
    points along each shared edge: each face's outward normal at the edge point's foot on its
    surface (the face's orientation applied), the angle between them, the fold
    (n_a x n_b) . t about the edge's tangent, how far the point lies off each surface, and, for
    each step length h, which side of the edge face ``a`` lies on (the point h along
    +-(n_a x t), classified by ``BRepClass_FaceClassifier``; ``None`` unless exactly one side is
    in) and d_a . n_b for that inward direction d_a: negative is convex (a wedge), positive
    concave. Where that first-order reading vanishes (outward normals opposite, a cusp), the
    second-order one decides: ``lift@h`` is n_b . (q - p), q the foot on face ``a``'s surface of
    the point h along d_a, which is positive when face ``a`` curls to the outside of face ``b``
    (concave: the material fills all round the edge but a zero-angle notch) and negative when
    to its inside (convex: a zero-angle wedge)."""

    from OCP.BRepClass import BRepClass_FaceClassifier
    from OCP.GeomLProp import GeomLProp_SLProps
    from OCP.TopAbs import TopAbs_IN

    faces = [f.wrapped for f in shape.faces()]
    fa, fb = faces[a], faces[b]

    def edges_of(face):
        out, explorer = [], TopExp_Explorer(face, TopAbs_EDGE)
        while explorer.More():
            out.append(TopoDS.Edge_s(explorer.Current()))
            explorer.Next()
        return out

    def normal(face, p):
        geom = BRep_Tool.Surface_s(face)
        project = GeomAPI_ProjectPointOnSurf(p, geom)
        u, v = project.LowerDistanceParameters()
        n = GeomLProp_SLProps(geom, u, v, 1, 1e-12).Normal()
        sign = -1.0 if face.Orientation() == TopAbs_REVERSED else 1.0
        return sign * np.array([n.X(), n.Y(), n.Z()]), project.LowerDistance()

    out = []
    seen = []
    for edge in edges_of(fa):
        if any(edge.IsSame(e) for e in seen) or not any(edge.IsSame(e) for e in edges_of(fb)):
            continue
        seen.append(edge)
        curve = BRepAdaptor_Curve(edge)
        t0, t1 = curve.FirstParameter(), curve.LastParameter()
        for k in range(1, 10):
            t = t0 + (t1 - t0) * k / 10
            p, d = gp_Pnt(), gp_Vec()
            curve.D1(t, p, d)
            tangent = np.array([d.X(), d.Y(), d.Z()])
            tangent /= np.linalg.norm(tangent)
            na, off_a = normal(fa, p)
            nb, off_b = normal(fb, p)
            side = np.cross(na, tangent)
            side /= np.linalg.norm(side)
            row = {
                "t": t,
                "angle_deg": math.degrees(math.acos(max(-1.0, min(1.0, float(na @ nb))))),
                "one_minus_cos": 1.0 - float(na @ nb),
                "fold": float(np.cross(na, nb) @ tangent),
                "off_a": off_a,
                "off_b": off_b,
            }
            for h in steps:
                inside = []
                for sense in (1.0, -1.0):
                    q = np.array([p.X(), p.Y(), p.Z()]) + sense * h * side
                    state = BRepClass_FaceClassifier(fa, gp_Pnt(*q), 1e-9).State()
                    inside.append(state == TopAbs_IN)
                if inside[0] != inside[1]:
                    inward = side if inside[0] else -side
                    row[f"d_a.n_b@{h:g}"] = float(inward @ nb)
                    q = gp_Pnt(*(np.array([p.X(), p.Y(), p.Z()]) + h * inward))
                    foot = GeomAPI_ProjectPointOnSurf(q, BRep_Tool.Surface_s(fa)).NearestPoint()
                    row[f"lift@{h:g}"] = float((np.array(foot.Coord()) - np.array(p.Coord())) @ nb)
                else:
                    row[f"d_a.n_b@{h:g}"] = None
                    row[f"lift@{h:g}"] = None
            out.append(row)
    return out


def main() -> None:
    args = sys.argv[1:]
    centre = None
    if "--centre" in args:
        k = args.index("--centre")
        centre = [float(x) for x in args[k + 1 : k + 4]]
        del args[k : k + 4]
    list_faces = "--faces" in args
    args = [a for a in args if a != "--faces"]
    if args[0] == "faces":
        file, indices = args[1], [int(i) for i in args[2:]]
        if centre is None:
            part = _load(file)
            solid = next(iter(part.solids())).wrapped
            centre = vertex_centre(solid)
        found = _run_faces(file, indices, centre)
        print(json.dumps({"file": file, "centre": centre, "faces": {str(i): found[i] for i in indices}}, indent=1))
    elif args[0] == "uv":
        part = _load(args[1])
        faces = [f.wrapped for f in part.faces()]
        print(json.dumps({i: uv_extent(faces[int(i)]) for i in args[2:]}, indent=1))
    elif args[0] == "boxes":
        part = _load(args[1])
        faces = [f.wrapped for f in part.faces()]
        print(json.dumps({i: edge_boxes(faces[int(i)]) for i in args[2:]}, indent=1))
    elif args[0] == "arcs":
        part = _load(args[1])
        pairs = [tuple(int(x) for x in pair.split("-")) for pair in args[2:]]
        print(json.dumps({f"{a}-{b}": arc_reading(part, a, b) for a, b in pairs}, indent=1))
    elif args[0] == "solids":
        print(json.dumps({f: solids(f, list_faces, centre) for f in args[1:]}, indent=1))
    else:
        raise SystemExit(__doc__)


if __name__ == "__main__":
    main()
