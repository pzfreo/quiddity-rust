"""Optional build123d migration trial; run in a scratch venv with the installed wheel.

No production package dependency: build123d/OCP are used only by this experiment.
The result is a JSON report. STEP exports and the environment remain in scratch space.
"""
import argparse
import io
import json
from pathlib import Path
import re
import tempfile

import build123d as b
from OCP.BRepBuilderAPI import BRepBuilderAPI_MakeVertex
from OCP.BRepExtrema import BRepExtrema_DistShapeShape
from OCP.gp import gp_Pnt
from OCP.IFSelect import IFSelect_RetDone
from OCP.STEPConstruct import STEPConstruct
from OCP.STEPControl import STEPControl_AsIs, STEPControl_Writer
from OCP.TCollection import TCollection_HAsciiString
import quiddity


def named_export(shape, faces, output):
    writer = STEPControl_Writer()
    assert writer.Transfer(shape.wrapped, STEPControl_AsIs) == IFSelect_RetDone
    process = writer.WS().TransferWriter().FinderProcess()
    for index, face in enumerate(faces):
        entity = STEPConstruct.FindEntity_s(process, face.wrapped)
        if entity is None:
            raise RuntimeError(f"no transferred entity for face {index}")
        entity.SetName(TCollection_HAsciiString(f"quiddity_trial_face_{index}"))
    assert writer.Write(str(output)) == IFSelect_RetDone
    return output.read_bytes()


def trial(name, shape, directory):
    faces = list(shape.faces())
    stream = io.BytesIO()
    assert b.export_step(shape, stream)
    step = stream.getvalue()
    (directory / f"{name}.step").write_bytes(step)
    recognition = json.loads(quiddity.recognise(step))["document"]
    evidence = json.loads(quiddity.evidence(step))
    assert recognition["face_count"] == evidence["face_count"] == len(faces)
    mapping = []
    for face_index in range(len(faces)):
        anchor = json.loads(quiddity.anchor(step, face_index))
        vertex = BRepBuilderAPI_MakeVertex(gp_Pnt(*anchor["point"])).Vertex()
        distances = [BRepExtrema_DistShapeShape(vertex, face.wrapped).Value() for face in faces]
        matches = [index for index, distance in enumerate(distances) if distance < 1e-6]
        if len(matches) != 1:
            raise RuntimeError(f"{name} face {face_index}: anchor map ambiguous: {matches}")
        mapping.append(matches[0])
    assert sorted(mapping) == list(range(len(faces)))

    named = named_export(shape, faces, directory / f"{name}.named.step")
    named_evidence = json.loads(quiddity.evidence(named))
    names = {
        int(entity): int(index)
        for entity, index in re.findall(
            rb"#(\d+)\s*=\s*ADVANCED_FACE\s*\(\s*'quiddity_trial_face_(\d+)'", named
        )
    }
    named_mapping = [names[source["entity"]] for source in named_evidence["face_sources"]]
    assert sorted(named_mapping) == list(range(len(faces)))
    # Verify the transfer-map join against the same independent geometric check.
    for index, original in enumerate(named_mapping):
        point = json.loads(quiddity.anchor(named, index))["point"]
        vertex = BRepBuilderAPI_MakeVertex(gp_Pnt(*point)).Vertex()
        assert BRepExtrema_DistShapeShape(vertex, faces[original].wrapped).Value() < 1e-6
    return {
        "shape": name,
        "face_count": len(faces),
        "standard_export_order_matches": mapping == list(range(len(faces))),
        "standard_export_index_to_original": mapping,
        "named_export_order_matches": named_mapping == list(range(len(faces))),
        "named_export_bijection_verified": True,
        "named_export_index_to_original": named_mapping,
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    box = b.Box(40, 30, 20) - b.Cylinder(4, 40)
    turned = (b.Cylinder(12, 12) + b.Pos(0, 0, 8) * b.Cylinder(7, 12)) - b.Cylinder(3, 50)
    stock = b.Box(40, 30, 20)
    rounded = b.fillet(stock.edges(), 2) - b.Cylinder(4, 40)
    with tempfile.TemporaryDirectory(prefix="quiddity-face-trial-") as directory:
        report = {
            "build123d_version": b.__version__,
            "quiddity_version": quiddity.__version__,
            "tolerance_mm": 1e-6,
            "cases": [trial(name, shape, Path(directory)) for name, shape in
                      [("box_with_hole", box), ("turned_part", turned), ("fillet_heavy", rounded)]],
        }
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(args.output)


if __name__ == "__main__":
    main()
