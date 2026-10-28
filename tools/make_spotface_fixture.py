#!/usr/bin/env python3
"""Write the original Apache-2.0 spotface/outer-step fixture using only Python's stdlib.

Run: python3 tools/make_spotface_fixture.py
No third-party model, CAD package, or external dataset is used. The explicit radial/
axial profile makes a diameter-40 x 12 ring with a diameter-6 through bore and, from
its top mouth inward: diameter 30 x 0.5 spotface, diameter 20 x 2 outer counterbore,
and diameter 12 x 3 inner counterbore. Depths are each cylinder's own axial span.
Full-circle rim edges and straight cylinder seams close the analytic STEP solid.
The fixed header and entity order make regeneration byte-for-byte deterministic.
"""
from pathlib import Path


def main():
    entities = []

    def add(value):
        entities.append(value)
        return f"#{len(entities)}"

    def flag(value):
        return ".T." if value else ".F."

    def point(x, z):
        return add(f"CARTESIAN_POINT('',({x},0.,{z}))")

    z_axis = add("DIRECTION('',(0.,0.,1.))")
    x_axis = add("DIRECTION('',(1.,0.,0.))")

    def frame(z):
        origin = point(0., z)
        return add(f"AXIS2_PLACEMENT_3D('',{origin},{z_axis},{x_axis})")

    def loop_bound(edges, outer=True):
        oriented = [add(f"ORIENTED_EDGE('',*,*,{edge},{flag(sense)})")
                    for edge, sense in edges]
        loop = add(f"EDGE_LOOP('',({','.join(oriented)}))")
        kind = "FACE_OUTER_BOUND" if outer else "FACE_BOUND"
        return add(f"{kind}('',{loop},.T.)")

    # Walk up the outside, then down the internal steps to the bottom of the bore.
    profile = [(20., 0.), (20., 12.), (15., 12.), (15., 11.5),
               (10., 11.5), (10., 9.5), (6., 9.5), (6., 6.5),
               (3., 6.5), (3., 0.)]
    points, vertices, rims = [], [], []
    for radius, z in profile:
        points.append(point(radius, z))
        vertices.append(add(f"VERTEX_POINT('',{points[-1]})"))
        circle = add(f"CIRCLE('',{frame(z)},{radius})")
        rims.append(add(f"EDGE_CURVE('',{vertices[-1]},{vertices[-1]},{circle},.T.)"))
    faces = []
    for i, (radius, z) in enumerate(profile):
        j = (i + 1) % len(profile)
        next_radius, next_z = profile[j]
        if radius == next_radius:
            bottom, top = (i, j) if z < next_z else (j, i)
            vector = add(f"VECTOR('',{z_axis},1.)")
            line = add(f"LINE('',{points[bottom]},{vector})")
            seam = add(f"EDGE_CURVE('',{vertices[bottom]},{vertices[top]},{line},.T.)")
            bounds = [loop_bound([(rims[bottom], z < next_z), (seam, True),
                                  (rims[top], z > next_z), (seam, False)])]
            surface = add(f"CYLINDRICAL_SURFACE('',{frame(0.)},{radius})")
            same_sense = z < next_z
        else:
            assert z == next_z
            bounds = [loop_bound([(rims[i], True)], radius > next_radius),
                      loop_bound([(rims[j], False)], radius < next_radius)]
            surface = add(f"PLANE('',{frame(z)})")
            same_sense = radius > next_radius
        faces.append(add(f"ADVANCED_FACE('',({','.join(bounds)}),{surface},{flag(same_sense)})"))
    shell = add(f"CLOSED_SHELL('',({','.join(faces)}))")
    solid = add(f"MANIFOLD_SOLID_BREP('original stepped ring',{shell})")
    mm = add("(LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.))")
    rad = add("(NAMED_UNIT(*) PLANE_ANGLE_UNIT() SI_UNIT($,.RADIAN.))")
    sterad = add("(NAMED_UNIT(*) SI_UNIT($,.STERADIAN.) SOLID_ANGLE_UNIT())")
    uncertainty = add(f"UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-7),{mm},'distance_accuracy_value','')")
    context = add(f"(GEOMETRIC_REPRESENTATION_CONTEXT(3) GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT(({uncertainty})) GLOBAL_UNIT_ASSIGNED_CONTEXT(({mm},{rad},{sterad})) REPRESENTATION_CONTEXT('','3D'))")
    shape = add(f"ADVANCED_BREP_SHAPE_REPRESENTATION('',({frame(0.)},{solid}),{context})")
    application = add("APPLICATION_CONTEXT('core data for automotive mechanical design processes')")
    add(f"APPLICATION_PROTOCOL_DEFINITION('international standard','automotive_design',2000,{application})")
    product_context = add(f"PRODUCT_CONTEXT('',{application},'mechanical')")
    product = add(f"PRODUCT('spotface_outer_steps','original analytic fixture','',({product_context}))")
    formation = add(f"PRODUCT_DEFINITION_FORMATION('','',{product})")
    definition_context = add(f"PRODUCT_DEFINITION_CONTEXT('part definition',{application},'design')")
    definition = add(f"PRODUCT_DEFINITION('design','',{formation},{definition_context})")
    product_shape = add(f"PRODUCT_DEFINITION_SHAPE('','',{definition})")
    add(f"SHAPE_DEFINITION_REPRESENTATION({product_shape},{shape})")
    header = """ISO-10303-21;
HEADER;
FILE_DESCRIPTION(('Original Apache-2.0 analytic spotface fixture; no external model'),'2;1');
FILE_NAME('spotface_outer_steps.step','2026-10-28T00:00:00',('quiddity-rust contributors'),('quiddity-rust'),'tools/make_spotface_fixture.py','quiddity-rust','Apache-2.0');
FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));
ENDSEC;
DATA;
"""
    data = header + "\n".join(f"#{i} = {value};" for i, value in enumerate(entities, 1))
    path = Path(__file__).resolve().parents[1] / "tests/fixtures/spotface_outer_steps.step"
    path.write_text(data + "\nENDSEC;\nEND-ISO-10303-21;\n")
    print(path)


if __name__ == "__main__":
    main()
