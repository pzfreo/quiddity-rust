"""Record what OpenCascade's XCAF reader makes of the semantic PMI in AP242 files.

    /Users/paul/repos/specify-core/.venv/bin/python tools/capture_pmi_occt.py [NIST_DIR]

``NIST_DIR`` is the unzipped NIST-PMI-STEP-Files directory (default ``$NIST_PMI``). Every
``*_ap242-*.stp`` in it, and every ``tests/fixtures/ap242/specify/*.step.gz``, is read with
``STEPCAFControl_Reader`` (GD&T and names on, as specify-core's ``load.py`` reads), and
``tests/fixtures/ap242/occt/<file stem>.json.gz`` gets, per distinct part (``load.py``'s
numbering: ``TopExp::MapShapes`` face order on each part's own shape):

- dimensions: type, value, raw values, plus/minus tolerances, range bounds, class of tolerance,
  qualifier, angular qualifier, modifiers, decimal places, direction, path edge, semantic name;
- geometric tolerances: type, value, type of value, material and zone modifiers, modifiers,
  maximum value, affected plane, semantic name, and the datums XCAF links to the tolerance, each
  with its position and modifiers;
- datums: name, position, modifiers, datum target type, number, length, width, axis;
- materials (``XCAFDoc_Material``: name, description, density and its unit names);
- for every item, the faces it references: the face index on its part and the ``ADVANCED_FACE``
  instance (``#N``) OpenCascade read the face from (``TransferReader::EntityFromShapeResult``).

Values are recorded exactly as OpenCascade returns them. Its known misreads (specify-core
``writer.py`` and ``existing.py``) are listed in ``known_misreads`` of every capture, never
corrected here. The class of tolerance comes from ``DumpJson``, because the OCP binding of
``GetClassOfTolerance`` cannot return its output arguments. An item none of whose faces is on a
part is listed under ``unattached``. Run with the specify-core venv (OCP 7.9).
"""

from __future__ import annotations

import gzip
import io
import json
import os
import re
import sys
import tempfile
from pathlib import Path

import OCP  # noqa: E402
import OCP.XCAFDimTolObjects as X  # noqa: E402
from OCP.gp import gp_Dir  # noqa: E402
from OCP.IFSelect import IFSelect_RetDone  # noqa: E402
from OCP.STEPCAFControl import STEPCAFControl_Reader  # noqa: E402
from OCP.STEPControl import STEPControl_Controller  # noqa: E402
from OCP.TCollection import TCollection_ExtendedString  # noqa: E402
from OCP.TDataStd import TDataStd_Name  # noqa: E402
from OCP.TDF import TDF_Label, TDF_LabelSequence, TDF_Tool  # noqa: E402
from OCP.TCollection import TCollection_AsciiString  # noqa: E402
from OCP.TDocStd import TDocStd_Document  # noqa: E402
from OCP.TopAbs import TopAbs_FACE  # noqa: E402
from OCP.TopExp import TopExp, TopExp_Explorer  # noqa: E402
from OCP.TopTools import TopTools_IndexedMapOfShape  # noqa: E402
from OCP.XCAFApp import XCAFApp_Application  # noqa: E402
from OCP.XCAFDoc import (  # noqa: E402
    XCAFDoc_Datum,
    XCAFDoc_Dimension,
    XCAFDoc_DocumentTool,
    XCAFDoc_GeomTolerance,
    XCAFDoc_Material,
    XCAFDoc_ShapeTool,
)

sys.path.insert(0, str(Path(__file__).resolve().parent))
from _provenance import sha256  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
FIXTURES = ROOT / "tests" / "fixtures" / "ap242"
OUT = FIXTURES / "occt"

#: What OpenCascade is known to misread, as specify-core measured it (writer.py, existing.py).
#: Recorded with every capture so a reader of the values knows them; nothing here corrects them.
KNOWN_MISREADS = [
    "lower plus/minus deviation: GetLowerTolValue may lose its sign (specify-core existing.py)",
    "both deviations below nominal (g6, f7): limits dropped and the value garbled, e.g. Ø70 g6"
    " reads 35.0145 (specify-core writer.py)",
    "geometric tolerance magnitude held in a complex entity reads 0 (specify-core existing.py:"
    " every NIST file)",
    "datum position is held on the datum, one datum label per tolerance that cites it"
    " (specify-core writer.py)",
    "a datum is imported only when some tolerance references it (specify-core writer.py)",
    "a datum whose feature names its faces as a set has no faces (NIST CTC-01 B and C;"
    " specify-core existing.py)",
    "one datum per name per document: in an assembly a second part's datum A is the first"
    " part's (specify-core writer.py)",
    "the class of tolerance (ISO 286) is written wrongly by OpenCascade's writer; files"
    " specify-core wrote state fits as limits only (specify-core writer.py)",
]

_FORM = re.compile(r'"(IsHole|FormVariance|Grade)": (-?\d+)')


def instances(data: bytes) -> list[tuple[int, str]]:
    """``(id, first keyword)`` of every instance of the DATA sections, in file order, scanning
    the text outside strings and comments."""
    text = data.decode("latin-1")
    out, i, n = [], 0, len(text)
    start = text.find("DATA;")
    i = start + 5 if start >= 0 else n
    statement = []
    while i < n:
        c = text[i]
        if c == "'":
            j = i + 1
            while j < n:
                if text[j] == "'":
                    if j + 1 < n and text[j + 1] == "'":
                        j += 2
                        continue
                    break
                j += 1
            statement.append("''")
            i = j + 1
            continue
        if text.startswith("/*", i):
            end = text.find("*/", i + 2)
            i = n if end < 0 else end + 2
            continue
        if c == ";":
            head = "".join(statement).strip()
            m = re.match(r"#(\d+)\s*=\s*\(?\s*([A-Za-z_][A-Za-z0-9_]*)", head)
            if m:
                out.append((int(m.group(1)), m.group(2).upper()))
            statement = []
            i += 1
            continue
        statement.append(c)
        i += 1
    return out


def _enum(value) -> str:
    return getattr(value, "name", str(value)).split(".")[-1]


def _strip(value, prefix: str) -> str:
    return _enum(value).removeprefix(prefix)


def _text(handle) -> str | None:
    if handle is None:
        return None
    for attr in ("ToCString", "ToExtString"):
        if hasattr(handle, attr):
            return getattr(handle, attr)()
    return str(handle)


def _entry(label) -> str:
    out = TCollection_AsciiString()
    TDF_Tool.Entry_s(label, out)
    return out.ToCString()


def _each(labels: TDF_LabelSequence):
    for i in range(1, labels.Length() + 1):
        yield labels.Value(i)


def _dumped(obj) -> dict[str, int]:
    stream = io.BytesIO()
    obj.DumpJson(stream)
    return {k: int(v) for k, v in _FORM.findall(stream.getvalue().decode(errors="replace"))}


class Capture:
    def __init__(self, path: Path) -> None:
        STEPControl_Controller.Init_s()
        fmt = TCollection_ExtendedString("MDTV-XCAF")
        self.doc = TDocStd_Document(fmt)
        XCAFApp_Application.GetApplication_s().NewDocument(fmt, self.doc)
        self.reader = STEPCAFControl_Reader()
        self.reader.SetGDTMode(True)
        self.reader.SetNameMode(True)
        self.reader.SetColorMode(True)
        if self.reader.ReadFile(str(path)) != IFSelect_RetDone:
            raise ValueError(f"cannot read {path}")
        if not self.reader.Transfer(self.doc):
            raise ValueError(f"cannot transfer {path}")
        session = self.reader.Reader().WS()
        self.model = session.Model()
        self.transfer = session.TransferReader()
        self.ranks = {self.model.Value(k): k for k in range(1, self.model.NbEntities() + 1)}
        self.instances = instances(path.read_bytes())
        if len(self.instances) != self.model.NbEntities():
            raise ValueError(
                f"{path}: {len(self.instances)} instances in the text, {self.model.NbEntities()} read"
            )
        self.shapes = XCAFDoc_DocumentTool.ShapeTool_s(self.doc.Main())
        self.tool = XCAFDoc_DocumentTool.DimTolTool_s(self.doc.Main())
        roots = TDF_LabelSequence()
        self.shapes.GetFreeShapes(roots)
        self.roots = list(_each(roots))
        self.parts = []
        for root in self.roots:
            for label in self._simple(root):
                if self._faces(label).Extent() and not any(label.IsEqual(p) for p in self.parts):
                    self.parts.append(label)
        self.face_maps = [self._faces(label) for label in self.parts]

    # -- structure -------------------------------------------------------------------------

    def _referred(self, label):
        if not XCAFDoc_ShapeTool.IsReference_s(label):
            return label
        out = TDF_Label()
        XCAFDoc_ShapeTool.GetReferredShape_s(label, out)
        return out

    def _simple(self, label):
        label = self._referred(label)
        if not XCAFDoc_ShapeTool.IsAssembly_s(label):
            return [label]
        components = TDF_LabelSequence()
        XCAFDoc_ShapeTool.GetComponents_s(label, components)
        out = []
        for c in _each(components):
            out.extend(self._simple(c))
        return out

    @staticmethod
    def _faces(label) -> TopTools_IndexedMapOfShape:
        faces = TopTools_IndexedMapOfShape()
        shape = XCAFDoc_ShapeTool.GetShape_s(label)
        if not shape.IsNull():
            TopExp.MapShapes_s(shape, TopAbs_FACE, faces)
        return faces

    @staticmethod
    def _name(label) -> str | None:
        attribute = TDataStd_Name()
        if label.FindAttribute(TDataStd_Name.GetID_s(), attribute):
            return attribute.Get().ToExtString()
        return None

    def _instance(self, shape) -> str | None:
        """``#N``, the file instance OpenCascade read ``shape`` from, if it says. The model
        numbers instances by rank (file order); ``instances`` maps rank to id."""
        entity = self.transfer.EntityFromShapeResult(shape, 1)
        rank = self.ranks.get(entity) if entity is not None else None
        if rank is None:
            return None
        number, keyword = self.instances[rank - 1]
        found = entity.DynamicType().Name().split("_", 1)[-1]
        if keyword.replace("_", "") != found.upper():
            raise ValueError(f"rank {rank} is #{number} {keyword}, OpenCascade read {found}")
        return f"#{number}"

    def _refs(self, labels: TDF_LabelSequence) -> list[dict]:
        out, seen = [], set()
        for label in _each(labels):
            shape = self.shapes.GetShape_s(label)
            if shape.IsNull():
                out.append({"shape_label": _entry(label), "null": True})
                continue
            explorer = TopExp_Explorer(shape, TopAbs_FACE)
            found = False
            while explorer.More():
                found = True
                face = explorer.Current()
                part = index = None
                for p, faces in enumerate(self.face_maps):
                    i = faces.FindIndex(face)
                    if i > 0:
                        part, index = p, i - 1
                        break
                key = (
                    part,
                    index,
                    self._instance(
                        self.face_maps[part].FindKey(index + 1) if part is not None else face
                    ),
                )
                if key not in seen:
                    seen.add(key)
                    out.append({"part": part, "face": index, "advanced_face": key[2]})
                explorer.Next()
            if not found:
                kind = _strip(shape.ShapeType(), "TopAbs_")
                out.append({"shape": kind, "instance": self._instance(shape)})
        return out

    def _shape_refs(self, label) -> tuple[list[dict], list[dict]]:
        first, second = TDF_LabelSequence(), TDF_LabelSequence()
        self.tool.GetRefShapeLabel_s(label, first, second)
        return self._refs(first), self._refs(second)

    # -- items -----------------------------------------------------------------------------

    def dimension(self, label) -> dict:
        obj = XCAFDoc_Dimension.Set_s(label).GetObject()
        values = obj.GetValues()
        raw = [values.Value(i) for i in range(values.Lower(), values.Upper() + 1)] if values else []
        direction = gp_Dir()
        has_direction = obj.GetDirection(direction)
        path = obj.GetPath()
        left, right = obj.GetNbOfDecimalPlaces()
        out = {
            "label": _entry(label),
            "type": _strip(obj.GetType(), "XCAFDimTolObjects_DimensionType_"),
            "value": obj.GetValue(),
            "values": raw,
            "plus_minus": obj.IsDimWithPlusMinusTolerance(),
            "upper_tol": obj.GetUpperTolValue(),
            "lower_tol": obj.GetLowerTolValue(),
            "range": obj.IsDimWithRange(),
            "upper_bound": obj.GetUpperBound(),
            "lower_bound": obj.GetLowerBound(),
            "class_of_tolerance": obj.IsDimWithClassOfTolerance(),
            "qualifier": _strip(obj.GetQualifier(), "XCAFDimTolObjects_DimensionQualifier_")
            if obj.HasQualifier()
            else None,
            "angular_qualifier": _strip(
                obj.GetAngularQualifier(), "XCAFDimTolObjects_AngularQualifier_"
            )
            if obj.HasAngularQualifier()
            else None,
            "modifiers": [
                _strip(obj.GetModifiers().Value(i), "XCAFDimTolObjects_DimensionModif_")
                for i in range(1, obj.GetModifiers().Length() + 1)
            ],
            "decimal_places": [left, right],
            "direction": [direction.X(), direction.Y(), direction.Z()] if has_direction else None,
            "path_edge": None if path.IsNull() else self._instance(path),
            "semantic_name": _text(obj.GetSemanticName()),
        }
        if out["class_of_tolerance"]:
            dumped = _dumped(obj)
            out["class"] = {
                "is_hole": dumped.get("IsHole"),
                "form_variance": _enum_name(
                    "XCAFDimTolObjects_DimensionFormVariance", dumped.get("FormVariance")
                ),
                "grade": _enum_name("XCAFDimTolObjects_DimensionGrade", dumped.get("Grade")),
                "via": "DumpJson",
            }
        out["faces"], out["faces2"] = self._shape_refs(label)
        return out

    def datum_object(self, label) -> dict:
        obj = XCAFDoc_Datum.Set_s(label).GetObject()
        modifiers = obj.GetModifiers()
        out = {
            "label": _entry(label),
            "name": _text(obj.GetName()),
            "position": obj.GetPosition(),
            "modifiers": [
                _strip(modifiers.Value(i), "XCAFDimTolObjects_DatumSingleModif_")
                for i in range(1, modifiers.Length() + 1)
            ],
            "semantic_name": _text(obj.GetSemanticName()),
            "is_target": obj.IsDatumTarget(),
        }
        if obj.IsDatumTarget():
            out["target"] = {
                "type": _strip(obj.GetDatumTargetType(), "XCAFDimTolObjects_DatumTargetType_"),
                "number": obj.GetDatumTargetNumber(),
                "has_params": obj.HasDatumTargetParams(),
                "length": obj.GetDatumTargetLength(),
                "width": obj.GetDatumTargetWidth(),
            }
            if obj.HasDatumTargetParams():
                axis = obj.GetDatumTargetAxis()
                loc, d = axis.Location(), axis.Direction()
                out["target"]["axis"] = [[loc.X(), loc.Y(), loc.Z()], [d.X(), d.Y(), d.Z()]]
            shape = obj.GetDatumTarget()
            if not shape.IsNull():
                out["target"]["shape"] = _strip(shape.ShapeType(), "TopAbs_")
                out["target"]["instance"] = self._instance(shape)
        return out

    def tolerance(self, label) -> dict:
        obj = XCAFDoc_GeomTolerance.Set_s(label).GetObject()
        modifiers = obj.GetModifiers()
        datums = TDF_LabelSequence()
        self.tool.GetDatumWithObjectOfTolerLabels_s(label, datums)
        out = {
            "label": _entry(label),
            "type": _strip(obj.GetType(), "XCAFDimTolObjects_GeomToleranceType_"),
            "value": obj.GetValue(),
            "type_of_value": _strip(
                obj.GetTypeOfValue(), "XCAFDimTolObjects_GeomToleranceTypeValue_"
            ),
            "material_modifier": _strip(
                obj.GetMaterialRequirementModifier(), "XCAFDimTolObjects_GeomToleranceMatReqModif_"
            ),
            "zone_modifier": _strip(
                obj.GetZoneModifier(), "XCAFDimTolObjects_GeomToleranceZoneModif_"
            ),
            "zone_value": obj.GetValueOfZoneModifier(),
            "modifiers": [
                _strip(modifiers.Value(i), "XCAFDimTolObjects_GeomToleranceModif_")
                for i in range(1, modifiers.Length() + 1)
            ],
            "max_value": obj.GetMaxValueModifier(),
            "affected_plane": _strip(
                obj.GetAffectedPlaneType(), "XCAFDimTolObjects_ToleranceZoneAffectedPlane_"
            )
            if obj.HasAffectedPlane()
            else None,
            "semantic_name": _text(obj.GetSemanticName()),
            "datums": [self.datum_object(d) for d in _each(datums)],
        }
        out["faces"], _ = self._shape_refs(label)
        return out

    def datum(self, label) -> dict:
        out = self.datum_object(label)
        out["faces"], _ = self._shape_refs(label)
        tolerances = TDF_LabelSequence()
        self.tool.GetTolerOfDatumLabels(label, tolerances)
        out["tolerances"] = [_entry(t) for t in _each(tolerances)]
        return out

    def materials(self) -> list[dict]:
        tool = XCAFDoc_DocumentTool.MaterialTool_s(self.doc.Main())
        labels = TDF_LabelSequence()
        tool.GetMaterialLabels(labels)
        out = []
        for label in _each(labels):
            attribute = XCAFDoc_Material()
            if not label.FindAttribute(XCAFDoc_Material.GetID_s(), attribute):
                continue
            out.append(
                {
                    "label": _entry(label),
                    "name": _text(attribute.GetName()),
                    "description": _text(attribute.GetDescription()),
                    "density": attribute.GetDensity(),
                    "density_name": _text(attribute.GetDensName()),
                    "density_value_type": _text(attribute.GetDensValType()),
                }
            )
        return out

    def capture(self) -> dict:
        parts = [
            {
                "part": i,
                "label": _entry(label),
                "name": self._name(label),
                "faces": self.face_maps[i].Extent(),
                "dimensions": [],
                "geometric_tolerances": [],
                "datums": [],
            }
            for i, label in enumerate(self.parts)
        ]
        unattached = {"dimensions": [], "geometric_tolerances": [], "datums": []}
        sources = [
            ("dimensions", self.tool.GetDimensionLabels, self.dimension),
            ("geometric_tolerances", self.tool.GetGeomToleranceLabels, self.tolerance),
            ("datums", self.tool.GetDatumLabels, self.datum),
        ]
        for key, labels_of, read in sources:
            labels = TDF_LabelSequence()
            labels_of(labels)
            for label in _each(labels):
                item = read(label)
                owners = sorted(
                    {
                        r["part"]
                        for r in item["faces"] + item.get("faces2", [])
                        if r.get("part") is not None
                    }
                )
                item["parts"] = owners
                (parts[owners[0]][key] if owners else unattached[key]).append(item)
        return {
            "parts": parts,
            "unattached": unattached,
            "materials": self.materials(),
            "free_shapes": len(self.roots),
        }


def _enum_name(enum: str, value: int | None) -> str | None:
    if value is None:
        return None
    members = getattr(X, enum).__members__
    for name, member in members.items():
        if int(member) == value:
            return name.removeprefix(enum + "_")
    return f"<{value}>"


def inputs(nist: Path | None) -> list[Path]:
    files = sorted(nist.glob("*_ap242-*.stp")) if nist else []
    return files + sorted((FIXTURES / "specify").glob("*.step.gz"))


def capture_file(path: Path) -> dict:
    if path.suffix == ".gz":
        with tempfile.TemporaryDirectory() as tmp:
            plain = Path(tmp) / path.name.removesuffix(".gz")
            plain.write_bytes(gzip.decompress(path.read_bytes()))
            body = Capture(plain).capture()
    else:
        body = Capture(path).capture()
    return {
        "file": path.name,
        "sha256": sha256(path),
        "ocp": OCP.__version__,
        "reader": "STEPCAFControl_Reader GDT+names+colours (specify-core load.py)",
        "known_misreads": KNOWN_MISREADS,
        **body,
    }


def main() -> None:
    nist = (
        Path(sys.argv[1])
        if len(sys.argv) > 1
        else (Path(os.environ["NIST_PMI"]) if os.environ.get("NIST_PMI") else None)
    )
    OUT.mkdir(parents=True, exist_ok=True)
    for path in inputs(nist):
        record = capture_file(path)
        stem = path.name.removesuffix(".gz").rsplit(".", 1)[0]
        text = json.dumps(record, allow_nan=False, sort_keys=True, separators=(",", ":")) + "\n"
        (OUT / f"{stem}.json.gz").write_bytes(gzip.compress(text.encode(), mtime=0))
        counts = [
            sum(len(p[k]) for p in record["parts"])
            for k in ("dimensions", "geometric_tolerances", "datums")
        ]
        print(path.name, "dims/tols/datums", *counts, file=sys.stderr)


if __name__ == "__main__":
    main()
