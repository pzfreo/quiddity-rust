r"""Build ``wrapped.step``: one part held alone by an assembly, as an exporter wraps a part.

    /path/to/specify-core/.venv/bin/python tests/fixtures/ap242/assembly/make_wrapped.py

``assembly.step``'s plate, labelled "plate", placed once (translated) in an assembly whose name
is not ASCII ("Gehäuse Ø"). OpenCascade writes a non-ASCII label without Part 21's escapes (as
UTF-8, re-encoded), so the assembly is labelled with a placeholder and the script writes the name
as a conforming writer escapes it (``Geh\X2\00E4\X0\use \X2\00D8\X0\``). specify-core
(through OpenCascade XCAF) names the part after the assembly that holds it alone.
"""

from pathlib import Path

from build123d import BuildPart, Box, Compound, Cylinder, GridLocations, Location, Locations, Mode, export_step

with BuildPart() as plate:
    Box(100, 60, 10)
    with GridLocations(30, 30, 3, 2):
        Cylinder(3.3, 10, mode=Mode.SUBTRACT)
    with Locations((45, 0, 0)):
        Cylinder(2.1, 10, mode=Mode.SUBTRACT)
plate.part.label = "plate"
placed = plate.part.moved(Location((10, -20, 5)))
out = Path(__file__).resolve().parent / "wrapped.step"
export_step(Compound(children=[placed], label="WRAPPER"), str(out))
text = out.read_bytes()
assert text.count(b"'WRAPPER'") == 3  # FILE_NAME, PRODUCT id and name
out.write_bytes(text.replace(b"'WRAPPER'", rb"'Geh\X2\00E4\X0\use \X2\00D8\X0\'"))
