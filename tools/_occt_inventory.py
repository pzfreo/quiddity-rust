"""OpenCascade face fingerprints for independent geometry measurements."""

def _inventory(part) -> list[dict]:
    """A per-face fingerprint: proves the Rust reader walks faces in OCCT's order."""

    out = []
    for face in part.faces():
        bb = face.bounding_box()
        out.append(
            {
                "type": face.geom_type.name,
                "edges": len(face.edges()),
                "min": [round(bb.min.X, 3), round(bb.min.Y, 3), round(bb.min.Z, 3)],
                "max": [round(bb.max.X, 3), round(bb.max.Y, 3), round(bb.max.Z, 3)],
            }
        )
    return out

