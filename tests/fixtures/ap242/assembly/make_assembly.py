"""Build ``assembly.step``: two parts, one of them placed twice.

    /path/to/specify-core/.venv/bin/python tests/fixtures/ap242/assembly/make_assembly.py

A 100x60x10 plate with a 3x2 grid of M6 clearance holes and an M5 tap drill, and a turned pin
(Ø6 x 12, Ø10 x 20, a blind Ø4.2 x 8 bore in its top) placed twice: once translated, once
translated and turned 37° about an axis off every coordinate axis, so a placement's rotation
is not a permutation. The parts are specify-core's ``assembly`` test fixture (tests/conftest.py)
with the second pin turned. build123d writes the pin once, as one product with two occurrences.
"""

from pathlib import Path

from build123d import Align, BuildPart, Box, Compound, Cylinder, GridLocations, Location, Locations, Mode, export_step

with BuildPart() as plate:
    Box(100, 60, 10)
    with GridLocations(30, 30, 3, 2):
        Cylinder(3.3, 10, mode=Mode.SUBTRACT)
    with Locations((45, 0, 0)):
        Cylinder(2.1, 10, mode=Mode.SUBTRACT)
with BuildPart() as pin:
    Cylinder(3, 12, align=(Align.CENTER, Align.CENTER, Align.MIN))
    with Locations((0, 0, 12)):
        Cylinder(5, 20, align=(Align.CENTER, Align.CENTER, Align.MIN))
    with Locations((0, 0, 32)):
        Cylinder(2.1, 8, align=(Align.CENTER, Align.CENTER, Align.MAX), mode=Mode.SUBTRACT)
plate.part.label = "plate"
pin.part.label = "pin"
left = pin.part.moved(Location((-30, 0, 5)))
right = pin.part.moved(Location((30, 0, 5), (1, 2, 3), 37))
out = Path(__file__).resolve().parent / "assembly.step"
export_step(Compound(children=[plate.part, left, right], label="assembly"), str(out))
