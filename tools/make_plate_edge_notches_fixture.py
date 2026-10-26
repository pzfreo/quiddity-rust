"""Regenerate tests/fixtures/plate_edge_notches.step with build123d.

A 300 x 160 x 12 plate between two 50 x 220 end blocks, with two rectangular through-notches
(R6 corners and sharp), a V notch with an R5 trough, a floored edge-open recess and a closed
through slot: the positive and negative cases of rectangular_through_notches and v_channels
(src/features/through_notches.rs, src/features/v_channels.rs). Those tests pin file face
numbers, so a regenerated file must be checked against them. The committed file was written by
Open CASCADE 7.9; build123d on Open CASCADE 8.0 reproduces its DATA section exactly, only the
header's timestamp and processor differ. No external dataset is used.

Usage: uv run --no-project --python 3.12 --with build123d \
    python tools/make_plate_edge_notches_fixture.py [output directory]
"""

import math
import sys
from pathlib import Path

from build123d import (
    Align,
    Box,
    BuildPart,
    BuildSketch,
    GeomType,
    Location,
    Plane,
    Polygon,
    export_step,
    extrude,
    fillet,
)

FIXTURES = Path(__file__).resolve().parents[1] / "tests" / "fixtures"
out = Path(sys.argv[1]) if len(sys.argv) > 1 else FIXTURES
T = 12.0
# Plate 300 x 160 x 12 (z 0..12) between two taller, wider end blocks.
plate = Box(300, 160, T, align=(Align.CENTER, Align.CENTER, Align.MIN))
for sx in (-1, 1):
    plate = plate + Box(50, 220, T + 30, align=(Align.CENTER, Align.CENTER, Align.MIN)).moved(Location((sx * 175, 0, -30)))

def prism(pts, z0, z1):
    with BuildPart() as p:
        with BuildSketch(Plane.XY.offset(z0)):
            Polygon(*pts, align=None)
        extrude(amount=z1 - z0)
    return p.part

# Rectangular through-notch with R6 corners at x=-90 (mouth on y=80), width 40, depth 25.
n1 = Box(40, 25 + 10, T + 2, align=(Align.CENTER, Align.MIN, Align.CENTER)).moved(Location((-90, 80 - 25, T / 2)))
plate = plate - n1
# Sharp rectangular through-notch at x=0, width 30, depth 20.
n2 = Box(30, 20 + 10, T + 2, align=(Align.CENTER, Align.MIN, Align.CENTER)).moved(Location((0, 80 - 20, T / 2)))
plate = plate - n2
# V notch at x=90: 60 degree included angle, virtual apex 35 below the mouth.
h = 45.0
half = h * math.tan(math.radians(30))
apex_y = 80 - 35
v = prism([(90, apex_y), (90 + half, apex_y + h), (90 - half, apex_y + h)], -1, T + 1)
plate = plate - v
# Blind edge-open recess from the -y edge, floor at z=6 (not through).
r = Box(40, 25 + 10, T, align=(Align.CENTER, Align.MIN, Align.MIN)).moved(Location((-60, -80 - 10, 6)))
plate = plate - r
# Closed through slot (two facing walls, no mouth).
s = Box(60, 20, T + 2, align=(Align.CENTER, Align.CENTER, Align.CENTER)).moved(Location((60, -20, T / 2)))
plate = plate - s

def edges_parallel_z_at(part, test):
    return [e for e in part.edges() if e.geom_type == GeomType.LINE and abs(e.tangent_at(0).Z) > 0.999 and test(e.center())]

# R6 at the two bottom corners of notch 1, R5 trough of the V, R4 corners of the closed slot.
plate = fillet(edges_parallel_z_at(plate, lambda c: abs(c.Y - 55) < 1e-6 and abs(abs(c.X + 90) - 20) < 1e-6), 6)
plate = fillet(edges_parallel_z_at(plate, lambda c: abs(c.X - 90) < 1e-6 and abs(c.Y - apex_y) < 1e-6), 5)
plate = fillet(edges_parallel_z_at(plate, lambda c: abs(c.Y + 20) < 10.001 and abs(abs(c.X - 60) - 30) < 1e-6 and abs(abs(c.Y + 20) - 10) < 1e-6), 4)
# R8 where each plate edge meets a block wall (the waist corners).
plate = fillet(edges_parallel_z_at(plate, lambda c: abs(abs(c.X) - 150) < 1e-6 and abs(abs(c.Y) - 80) < 1e-6), 8)
export_step(plate, str(out / "plate_edge_notches.step"))
print(len(plate.faces()))

