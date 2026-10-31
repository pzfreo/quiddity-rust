"""Outside-producer API: no CLI argument parser, subprocess, or temporary STEP/JSON files."""
from copy import deepcopy
import gzip
from importlib.resources import files
import json
from pathlib import Path
import re
import subprocess
import tempfile

import pytest
import quiddity
from quiddity import _native, pmi

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "tests/fixtures"
XCAF = ROOT / "crates/haecceity/tests/fixtures/xcaf"


@pytest.fixture(scope="module")
def producer():
    geometry = gzip.decompress((XCAF / "pmi_plate.geom.step.gz").read_bytes())
    document = json.loads((XCAF / "pmi_plate.pmi.json").read_text())
    return geometry, document


@pytest.fixture(scope="module")
def authored(producer):
    return pmi.write(*producer)


def test_nist_roundtrip_onto_stripped_geometry():
    source = FIXTURES / "ap242/nist/nist_ctc_02_asme1_ap242-e2.stp.gz"
    document = pmi.read(source)
    assert document["pmi"]["dimensions"] and document["pmi"]["tolerances"]
    empty = pmi.read(FIXTURES / "ties/cube.step")
    geometry = pmi.write(source, empty)
    stripped = pmi.read(geometry)
    assert not stripped["pmi"]["dimensions"] and not stripped["pmi"]["tolerances"]
    output = pmi.write(geometry, document)
    assert isinstance(output, bytes)
    assert pmi.read(output) == document
    faces = re.findall(rb"#\d+\s*=\s*ADVANCED_FACE\b.*?;", geometry, re.DOTALL)
    assert faces and all(face in output for face in faces)
    assert pmi.validate(document) is None


def test_bytes_without_temp_files_or_cli(producer, tmp_path, monkeypatch):
    def forbidden(*args, **kwargs):
        pytest.fail("in-memory PMI operation attempted a temp file or subprocess")

    monkeypatch.chdir(tmp_path)
    for name in ("TemporaryDirectory", "NamedTemporaryFile", "mkstemp", "mkdtemp"):
        monkeypatch.setattr(tempfile, name, forbidden)
    monkeypatch.setattr(subprocess, "run", forbidden)
    geometry, document = producer
    output = pmi.write(geometry, json.dumps(document))
    assert isinstance(output, bytes) and output.startswith(b"ISO-10303-21;")
    assert pmi.read(output, anchors="name") == document
    pmi.validate(pmi.presentation(output))
    assert list(tmp_path.iterdir()) == []


def test_name_and_index_anchors(authored, producer):
    assert pmi.read(authored, anchors="name") == producer[1]
    indexed = pmi.read(authored)
    assert indexed != producer[1]
    for feature in indexed["pmi"]["features"]:
        for item in feature["value"].get("items", []):
            if "face" in item:
                assert isinstance(item["face"], int)
    # Index mode clears original transport bindings; re-import retains the same indices.
    assert pmi.read(pmi.write(producer[0], indexed)) == indexed


def test_generated_presentation(authored):
    document = pmi.presentation(authored)
    assert document["schema"] == "haecceity.presentation/1"
    assert document["items"]
    assert any(graphic["source"] == "generated" for graphics in document["items"].values() for graphic in graphics)
    assert pmi.validate(document) is None


def test_paths_and_part_selection(authored, tmp_path):
    path = tmp_path / "part.step"
    path.write_bytes(authored)
    class StepPath:
        def __fspath__(self):
            return str(path)
    assert pmi.read(StepPath(), part=0) == pmi.read(authored)
    assert pmi.presentation(path, part=0) == pmi.presentation(authored)
    assert pmi.read(pmi.write(path, pmi.read(path), part=0)) == pmi.read(path)
    with pytest.raises(quiddity.PmiError, match="unknown part"):
        pmi.read(authored, part=999)
    for invalid in (-1, 2**64, True):
        with pytest.raises(quiddity.PmiError, match="part must be"):
            pmi.read(authored, part=invalid)
    assembly = FIXTURES / "ap242/specify/assembly_plate_pin.step.gz"
    with pytest.raises(quiddity.PmiError, match="select part explicitly"):
        pmi.read(assembly)
    assert pmi.read(assembly, part=0)["schema"] == "haecceity.pmi/1"


@pytest.mark.parametrize("numeric", [False, True])
def test_outside_producer_refusals(producer, numeric):
    geometry, original = producer
    document = deepcopy(original)
    if numeric:
        document["pmi"]["datums"][0]["value"]["feature"] = 0
        reason = "/pmi/datums/0/value/feature"
        with pytest.raises(quiddity.PmiError, match=reason):
            pmi.validate(document)
    else:
        document["pmi"]["features"][0]["value"]["items"][0]["face"] = {"name": "no-such-face"}
        reason = "unknown face name"
        assert pmi.validate(document) is None  # Existence requires geometry.
    with pytest.raises(quiddity.PmiError, match=reason) as caught:
        pmi.write(geometry, document)
    assert isinstance(caught.value, ValueError)
    assert isinstance(caught.value.findings, list)


def test_writer_refusals_and_findings():
    source = FIXTURES / "ap242/nist/nist_ctc_01_asme1_ap242-e1.stp.gz"
    document = pmi.read(source)
    with pytest.raises(quiddity.PmiError, match="dimension without a nominal value") as caught:
        pmi.write(source, document)
    assert caught.value.findings
    for finding in caught.value.findings:
        assert {"kind", "part", "ids", "entity", "detail"} == finding.keys()
    with pytest.raises(quiddity.PmiError, match="dimension without a nominal value"):
        pmi.validate(document)


@pytest.mark.parametrize("document", ["{}", "not json", '{"schema":1,"schema":2}', {"schema": "unknown"}, {"bad": float("nan")}])
def test_malformed_documents(document, producer):
    with pytest.raises(quiddity.PmiError) as caught:
        pmi.validate(document)
    assert str(caught.value)
    assert isinstance(caught.value.findings, list)
    with pytest.raises(quiddity.PmiError):
        pmi.write(producer[0], document)


@pytest.mark.parametrize("operation", [pmi.read, pmi.presentation, lambda step: pmi.write(step, {})])
@pytest.mark.parametrize("step", [b"", b"not STEP", str(FIXTURES / "missing.step")])
def test_step_refusals(operation, step):
    with pytest.raises(quiddity.PmiError) as caught:
        operation(step)
    assert str(caught.value) and isinstance(caught.value.findings, list)


def test_name_anchor_refusals(producer):
    with pytest.raises(quiddity.PmiError, match="anchors must be"):
        pmi.read(producer[0], anchors="guess")
    # A producer may name an unambiguous face, but ambiguous names cannot be exported.
    geometry, document = producer
    names = [item["face"]["name"] for feature in document["pmi"]["features"]
             for item in feature["value"].get("items", []) if "face" in item]
    first, second = list(dict.fromkeys(names))[:2]
    ambiguous = geometry.replace(second.encode(), first.encode())
    indexed = pmi.read(pmi.write(geometry, document))
    output = pmi.write(ambiguous, indexed)
    with pytest.raises(quiddity.PmiError, match="ambiguous face name"):
        pmi.read(output, anchors="name")
    unnamed = geometry.replace(first.encode(), b"")
    output = pmi.write(unnamed, indexed)
    with pytest.raises(quiddity.PmiError, match="has no STEP name"):
        pmi.read(output, anchors="name")


def test_packaged_schemas():
    for name, value, native in [
        ("haecceity.pmi-1.json", pmi.SCHEMA, _native.PMI_SCHEMA),
        ("haecceity.presentation-1.json", pmi.PRESENTATION_SCHEMA, _native.PRESENTATION_SCHEMA),
    ]:
        packaged = files("quiddity").joinpath("schema", name).read_bytes()
        assert packaged == (ROOT / "crates/haecceity/schema" / name).read_bytes()
        assert packaged.decode() == native
        assert json.loads(packaged) == value
