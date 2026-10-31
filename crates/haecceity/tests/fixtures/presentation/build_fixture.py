"""Derive a mixed text/filled fixture from the unchanged committed NIST CTC-01.

The NIST archive contains no TEXT_LITERAL entities. The added literal is authored
regression data; it is NOT an annotation supplied by NIST. All original geometry,
PMI and tessellated graphics remain byte-for-byte unchanged (only view membership
lists gain the added annotation plane).
"""
import gzip
import pathlib
import re

here = pathlib.Path(__file__).resolve().parent
source = here.parents[4] / "tests/fixtures/ap242/nist/nist_ctc_01_asme1_ap242-e1.stp.gz"
raw = gzip.decompress(source.read_bytes())
text = raw.decode("ascii")
assert "TEXT_LITERAL" not in text
first = max(map(int, re.findall(r"#(\d+)\s*=", text))) + 1
ids = dict(zip(["point", "z", "x", "axis", "font", "extent", "literal", "colour", "appearance", "text_style", "assignment", "occurrence", "callout", "plane", "annotation"], range(first, first + 15)))
def n(key):
    return f"#{ids[key]}"
records = [
    f"{n('point')}=CARTESIAN_POINT('',(10.,20.,30.));",
    f"{n('z')}=DIRECTION('',(0.,-1.,0.));",
    f"{n('x')}=DIRECTION('',(1.,0.,0.));",
    f"{n('axis')}=AXIS2_PLACEMENT_3D('',{n('point')},{n('z')},{n('x')});",
    f"{n('font')}=DRAUGHTING_PRE_DEFINED_TEXT_FONT('ISO 3098');",
    f"{n('extent')}=PLANAR_EXTENT('',60.,3.);",
    f"{n('literal')}=TEXT_LITERAL_WITH_EXTENT('authored regression literal','CTC-01 editable test text',{n('axis')},'baseline left',.RIGHT.,{n('font')},{n('extent')});",
    f"{n('colour')}=COLOUR_RGB('',0.,0.,0.);",
    f"{n('appearance')}=TEXT_STYLE_FOR_DEFINED_FONT({n('colour')});",
    f"{n('text_style')}=TEXT_STYLE('',{n('appearance')});",
    f"{n('assignment')}=PRESENTATION_STYLE_ASSIGNMENT(({n('text_style')}));",
    f"{n('occurrence')}=ANNOTATION_TEXT_OCCURRENCE('authored regression literal',({n('assignment')}),{n('literal')});",
    f"{n('callout')}=DRAUGHTING_CALLOUT('authored regression text',({n('occurrence')}));",
    f"{n('plane')}=PLANE('authored regression text',{n('axis')});",
    f"{n('annotation')}=ANNOTATION_PLANE('authored regression text',(#3460),{n('plane')},({n('callout')}));",
]
# Both the global draughting model and saved view MBD_0 must contain the annotation.
text, count = re.subn(r"(#630=DRAUGHTING_MODEL\('',\([^;]*?)(\),#4351\);)", lambda m: m[1] + "," + n('annotation') + m[2], text)
assert count == 1
text, count = re.subn(r"(REPRESENTATION\('MBD_0',\([^;]*?)(\),#4351\))", lambda m: m[1] + "," + n('annotation') + m[2], text)
assert count == 1
at = text.rfind("ENDSEC;")
text = text[:at] + "\r\n".join(records) + "\r\n" + text[at:]
(here / "ctc01-text-filled.step.gz").write_bytes(gzip.compress(text.encode("ascii"), mtime=0))
print("authored ids", ids)
