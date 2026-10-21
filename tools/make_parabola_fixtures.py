"""Regenerate the two small parabola fixtures with cadquery-ocp (Open CASCADE 8.0).

The cut cone has analytic volume 1178.246346046747 and area 723.8637273389423.
OCCT's pcurve integrals at Eps=1e-12 are lower by 1.37e-7 and 6.55e-8.
See the independent cross-section integral in step.rs's regression test. The wireframe export deliberately retains OCCT's
conflicting trim points and PARAMETER_VALUEs, including .PARAMETER. as master.
No external dataset is used.
"""

from pathlib import Path

from OCP.BRepAlgoAPI import BRepAlgoAPI_Cut
from OCP.BRepBuilderAPI import BRepBuilderAPI_MakeEdge, BRepBuilderAPI_MakeFace
from OCP.BRepPrimAPI import BRepPrimAPI_MakeCone, BRepPrimAPI_MakeHalfSpace
from OCP.Geom import Geom_Parabola
from OCP.STEPControl import (
    STEPControl_AsIs,
    STEPControl_GeometricCurveSet,
    STEPControl_Writer,
)
from OCP.gp import gp_Ax2, gp_Dir, gp_Parab, gp_Pln, gp_Pnt

FIXTURES = Path(__file__).resolve().parents[1] / "tests" / "fixtures"


def write(name, shape, mode):
    writer = STEPControl_Writer()
    writer.Transfer(shape, mode)
    writer.Write(str(FIXTURES / name))


cone = BRepPrimAPI_MakeCone(10.0, 5.0, 10.0).Shape()
plane = BRepBuilderAPI_MakeFace(
    gp_Pln(gp_Pnt(4, 0, 0), gp_Dir(1, 0, 0.5)), -100, 100, -100, 100
).Face()
half_space = BRepPrimAPI_MakeHalfSpace(plane, gp_Pnt(20, 0, 0)).Solid()
write("parabola_cut_cone.step", BRepAlgoAPI_Cut(cone, half_space).Shape(), STEPControl_AsIs)

parabola = Geom_Parabola(gp_Parab(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), 2.0))
write(
    "occt_trimmed_parabola.step",
    BRepBuilderAPI_MakeEdge(parabola, -4.0, 4.0).Edge(),
    STEPControl_GeometricCurveSet,
)
