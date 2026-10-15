"""Build ``nested.step``: a plate and a sub-assembly of two pins, the sub-assembly placed away
from the origin, so a pin's placement composes two occurrences.

    /path/to/specify-core/.venv/bin/python tests/fixtures/ap242/assembly/make_nested.py

The plate and pin are ``make_assembly.py``'s. The sub-assembly ``pins`` sits at z=5 and holds
the pin twice, at x=-30 and at x=+30 turned 37° about an oblique axis; build123d writes ``pins``
as its own product, so ``assembly`` > ``pins`` > ``pin`` is two levels of
``next_assembly_usage_occurrence``.
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
left = pin.part.moved(Location((-30, 0, 0)))
right = pin.part.moved(Location((30, 0, 0), (1, 2, 3), 37))
pins = Compound(children=[left, right], label="pins")
pins.location = Location((0, 0, 5))
out = Path(__file__).resolve().parent / "nested.step"
export_step(Compound(children=[plate.part, pins], label="assembly"), str(out))
