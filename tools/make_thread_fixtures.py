#!/usr/bin/env python3
"""Regenerate the original thread positives and negatives (OCP 7.9.3.1).

OCP is needed only for regeneration, not at test time.

These are original constructions: a cylinder cut with opposed helical channels and a
round-wire spring. STEP headers are normalised, gzip has no timestamp.
"""
import gzip
import math
import pathlib
import tempfile

from OCP.BRepBuilderAPI import BRepBuilderAPI_MakeEdge, BRepBuilderAPI_MakeFace, BRepBuilderAPI_MakePolygon, BRepBuilderAPI_MakeWire
from OCP.BRepAlgoAPI import BRepAlgoAPI_Cut
from OCP.BRepLib import BRepLib
from OCP.BRepOffsetAPI import BRepOffsetAPI_MakePipe
from OCP.BRepPrimAPI import BRepPrimAPI_MakeCylinder
from OCP.GCE2d import GCE2d_MakeSegment
from OCP.Geom import Geom_CylindricalSurface
from OCP.GeomFill import GeomFill_IsFrenet, GeomFill_IsCorrectedFrenet
from OCP.STEPControl import STEPControl_Writer, STEPControl_AsIs
from OCP.gp import gp_Ax2, gp_Ax3, gp_Circ, gp_Dir, gp_Pnt, gp_Pnt2d

ROOT = pathlib.Path(__file__).resolve().parents[1] / 'tests/fixtures/modelled_threads'


def helix(radius, lead, turns, phase=0.0):
    surface = Geom_CylindricalSurface(gp_Ax3(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), radius)
    segment = GCE2d_MakeSegment(gp_Pnt2d(phase, 0), gp_Pnt2d(phase + turns*2*math.pi, lead*turns)).Value()
    edge = BRepBuilderAPI_MakeEdge(segment, surface).Edge()
    BRepLib.BuildCurves3d_s(edge)
    return BRepBuilderAPI_MakeWire(edge).Wire()


def write(name, shape):
    ROOT.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory() as temp:
        path = pathlib.Path(temp) / 'part.step'
        writer = STEPControl_Writer()
        writer.Transfer(shape, STEPControl_AsIs)
        writer.Write(str(path))
        data = path.read_bytes()
    # The output is a fixed test fixture; timestamp/name do not describe a user's document.
    import re
    data = re.sub(rb"FILE_NAME\(.*?\);", b"FILE_NAME('modelled-thread-fixture','',(''),(''),'OCP','quiddity','');", data, flags=re.S)
    (ROOT / (name + '.step.gz')).write_bytes(gzip.compress(data, mtime=0))


def groove(lead, phase, turns=2, depth=1.0, widths=(0.35, 1.0), frenet=True):
    radius = 10.0
    # Polygon lies in the normal plane at the path's start.
    h = lead/(2*math.pi)
    unit = math.hypot(radius, h)
    radial = (math.cos(phase), math.sin(phase), 0)
    across = (h*math.sin(phase)/unit, -h*math.cos(phase)/unit, radius/unit)
    polygon = BRepBuilderAPI_MakePolygon()
    for dr, width in [(-depth, -widths[0]), (-depth, widths[0]), (depth, widths[1]), (depth, -widths[1])]:
        polygon.Add(gp_Pnt(*[(radius+dr)*radial[i] + width*across[i] for i in range(3)]))
    polygon.Close()
    face = BRepBuilderAPI_MakeFace(polygon.Wire()).Face()
    return BRepOffsetAPI_MakePipe(helix(radius, lead, turns, phase), face, GeomFill_IsFrenet if frenet else GeomFill_IsCorrectedFrenet).Shape()


def main():
    knurl = BRepPrimAPI_MakeCylinder(10, 24).Shape()
    for lead in [12.0, -12.0]:
        for phase in [0.0, math.pi]:
            tool = groove(lead, phase)
            if lead < 0:
                from OCP.BRepBuilderAPI import BRepBuilderAPI_Transform
                from OCP.gp import gp_Trsf, gp_Vec
                move = gp_Trsf()
                move.SetTranslation(gp_Vec(0, 0, 24))
                tool = BRepBuilderAPI_Transform(tool, move, True).Shape()
            cut = BRepAlgoAPI_Cut(knurl, tool)
            cut.Build()
            assert cut.IsDone()
            knurl = cut.Shape()
            if lead == 12 and phase == 0:
                write('helical_groove', knurl)
    write('diamond_knurl', knurl)
    # Circular profile normal to the helix, swept without a supporting shaft.
    tangent = gp_Dir(0, 6, 4/(2*math.pi))
    profile = BRepBuilderAPI_MakeWire(BRepBuilderAPI_MakeEdge(gp_Circ(gp_Ax2(gp_Pnt(6, 0, 0), tangent), 0.75)).Edge()).Wire()
    face = BRepBuilderAPI_MakeFace(profile).Face()
    spring = BRepOffsetAPI_MakePipe(helix(6, 4, 3), face).Shape()
    write('round_wire_spring', spring)
    # A compact non-standard left-hand, two-start thread exercises measurements without
    # relying on a nominal designation. Its paired flanks have a 0.5 mm radial depth.
    from OCP.BRepBuilderAPI import BRepBuilderAPI_Transform
    from OCP.gp import gp_Trsf, gp_Vec
    threaded = BRepPrimAPI_MakeCylinder(10, 8).Shape()
    move = gp_Trsf()
    move.SetTranslation(gp_Vec(0, 0, 8))
    for phase in [0.0, math.pi]:
        tool = BRepBuilderAPI_Transform(groove(-2, phase, 4, 0.5, (0.125, 0.3)), move, True).Shape()
        cut = BRepAlgoAPI_Cut(threaded, tool)
        cut.Build()
        assert cut.IsDone()
        threaded = cut.Shape()
    write('left_two_start', threaded)
    # Deliberately let the profile twist around the spine. The cylindrical intersections
    # then advance by different amounts from turn to turn; this must report a refusal.
    varying = BRepPrimAPI_MakeCylinder(10, 8).Shape()
    for phase in [0.0, math.pi]:
        tool = BRepBuilderAPI_Transform(groove(-2, phase, 4, 0.5, (0.125, 0.3), frenet=False), move, True).Shape()
        cut = BRepAlgoAPI_Cut(varying, tool)
        cut.Build()
        assert cut.IsDone()
        varying = cut.Shape()
    write('varying_pitch_sweep', varying)


if __name__ == '__main__':
    main()
