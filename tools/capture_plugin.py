"""pytest plugin: record every call the Python test suite makes to chosen recognisers.

    QUIDDITY_CAPTURE=recognise_holes QUIDDITY_CAPTURE_OUT=tests/fixtures/holes \\
        ../quiddity/.venv/bin/python -m pytest -p capture_plugin <quiddity test files>

(run from the quiddity checkout with this directory on PYTHONPATH). Each call whose arguments
are a part plus plain keyword options is recorded: the part is exported to STEP, re-imported
through ``import_step_geometry`` and recognised again there — the Rust port only ever sees the
STEP file — and both answers are kept so a STEP round trip that changes the Python answer is
visible. Calls with richer arguments (precomputed inventories, injected dependencies) are
recorded by name only, so the port's coverage of them can be reported honestly.
"""

from __future__ import annotations

import dataclasses
import gzip
import hashlib
import json
import math
import os
import sys
import tempfile
from pathlib import Path

import pytest

TARGETS = [t for t in os.environ.get("QUIDDITY_CAPTURE", "").split(",") if t]
OUT = Path(os.environ.get("QUIDDITY_CAPTURE_OUT", "captured")).resolve()
_calls: list[dict] = []
_skipped: dict[str, int] = {}
_active = False


def _plain(value):
    """JSON form of a record, or None when the value is not plain data."""

    if dataclasses.is_dataclass(value) and not isinstance(value, type):
        return {f.name: _plain(getattr(value, f.name)) for f in dataclasses.fields(value)}
    if isinstance(value, (list, tuple)):
        return [_plain(v) for v in value]
    if isinstance(value, dict):
        return {str(k): _plain(v) for k, v in value.items()}
    if isinstance(value, float):
        return value if math.isfinite(value) else repr(value)
    if value is None or isinstance(value, (bool, int, str)):
        return value
    return {"__type__": type(value).__name__}


#: Keyword arguments that hand a recogniser an inventory it would otherwise compute from the
#: same part; the port always computes its own.
HINTS = {"cyls", "face_edges"}


def _real(module, name):
    """The unwrapped function, so a capture's own helper calls are not recorded."""

    function = getattr(module, name)
    return getattr(function, "__wrapped__", function)


def _target(package, name):
    """The recogniser *name*: exported by the package, or defined by one of its modules (a
    family whose entry point the package does not export, such as round-bottom slots)."""

    found = getattr(package, name, None)
    if found is not None:
        return found
    for module in list(sys.modules.values()):
        module_name = getattr(module, "__name__", "")
        function = getattr(module, name, None)
        if (
            function is not None
            and module_name.startswith(package.__name__ + ".")
            and getattr(function, "__module__", None) == module_name
        ):
            return function
    raise AttributeError(f"{name} is not defined by any loaded {package.__name__} module")


def _is_part(value) -> bool:
    return hasattr(value, "wrapped") and hasattr(value, "faces")


def _wrap(name, real):
    def recorded(*args, **kwargs):
        global _active
        if _active:  # a recogniser calling another: record only the outermost call
            return real(*args, **kwargs)
        _active = True
        try:
            result = real(*args, **kwargs)
        except BaseException:
            _active = False
            raise
        try:
            _record(name, real, args, kwargs, result)
        except Exception as error:  # noqa: BLE001 -- capture must never break the suite
            _skipped[f"{name}: capture failed: {type(error).__name__}"] = (
                _skipped.get(f"{name}: capture failed: {type(error).__name__}", 0) + 1
            )
        finally:
            _active = False
        return result

    recorded.__wrapped__ = real
    recorded.__name__ = getattr(real, "__name__", name)
    return recorded


def _record(name, real, args, kwargs, result):
    from build123d import export_step

    from quiddity import import_step_geometry

    simple = {
        k: v
        for k, v in kwargs.items()
        if k not in HINTS and (v is None or isinstance(v, (bool, int, float, str)))
    }
    # Precomputed inventories the port derives itself from the same part.
    hints = sorted(k for k in kwargs if k in HINTS)
    rich = sorted(set(kwargs) - set(simple) - set(hints))
    if "csinks" in rich and args and _is_part(args[0]):
        import quiddity

        if _plain(kwargs["csinks"]) == _plain(_real(quiddity, "recognise_countersinks")(args[0])):
            rich.remove("csinks")
            simple["csinks"] = "auto"
    test = os.environ.get("PYTEST_CURRENT_TEST", "?").split(" ")[0]
    if args and _is_part(args[0]) and len(args) == 1:
        part = args[0]
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "part.step"
            if not export_step(part, str(path)):
                raise RuntimeError("export failed")
            data = path.read_bytes()
            # The header carries a timestamp; key the file on its geometry only.
            body = data[data.index(b"DATA;") :]
            digest = hashlib.sha256(body).hexdigest()[:16]
            target = OUT / f"{digest}.step.gz"
            if not target.exists():
                target.write_bytes(gzip.compress(data, mtime=0))
            reread = import_step_geometry(str(path))
        entry = {
            "test": test,
            "function": name,
            "file": target.name,
            "options": simple,
            "hints": hints,
            "rich_arguments": rich,
            "in_memory": _plain(result),
        }
        if not rich:
            call = dict(simple)
            if call.get("csinks") == "auto":
                import quiddity

                call["csinks"] = _real(quiddity, "recognise_countersinks")(reread)
            entry["result"] = _plain(real(reread, **call))
        _calls.append(entry)
    elif not any(_is_part(a) for a in args):
        _calls.append(
            {
                "test": test,
                "function": name,
                "arguments": _plain(list(args)),
                "options": _plain(simple),
                "rich_arguments": rich,
                "result": _plain(result),
            }
        )
    else:
        key = f"{name}: unsupported call shape"
        _skipped[key] = _skipped.get(key, 0) + 1


def pytest_configure(config):
    if not TARGETS:
        return
    OUT.mkdir(parents=True, exist_ok=True)


_rerun_files: set[str] = set()


def pytest_collection_modifyitems(session, config, items):
    """Rebind the targets wherever a test module or quiddity module imported them by name."""

    _rerun_files.update(item.nodeid.split("::")[0] for item in items)

    import quiddity

    reals = {}
    for target in TARGETS:
        reals[target] = _target(quiddity, target)
    for module in list(sys.modules.values()):
        if module is None:
            continue
        mod_name = getattr(module, "__name__", "")
        if not (mod_name.startswith("quiddity") or mod_name.startswith("test")):
            continue
        for target, real in reals.items():
            if getattr(module, target, None) is real:
                setattr(module, target, _wrap(target, real))


def pytest_sessionfinish(session, exitstatus):
    if not TARGETS:
        return
    manifest = OUT / "calls.json"
    previous = json.loads(manifest.read_text()) if manifest.exists() else {"calls": []}
    # A re-run replaces what the same test files recorded for the same functions before.
    kept = [
        c
        for c in previous["calls"]
        if not (c["function"] in TARGETS and c["test"].split("::")[0] in _rerun_files)
    ]
    merged = kept + _calls
    # Skip counts of functions this run did not capture are kept as they were.
    skipped = {
        k: v
        for k, v in previous.get("skipped", {}).items()
        if k.split(":")[0] not in TARGETS
    }
    skipped.update(_skipped)
    manifest.write_text(
        json.dumps({"calls": merged, "skipped": skipped}, indent=1, allow_nan=False) + "\n"
    )
    print(f"\ncaptured {len(_calls)} calls ({len(merged)} total) into {manifest}", file=sys.stderr)
