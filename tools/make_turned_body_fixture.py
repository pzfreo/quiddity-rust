#!/usr/bin/env python3
"""Write an original Apache-2.0 chamfered bearing-like disc (#123).

Run: python3 tools/make_turned_body_fixture.py
The profile is drawn from dimensions in the test specification, with no source model.
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
    profile = [(10.75, -3.5), (11., -3.25), (11., 3.25),
               (10.75, 3.5), (4., 3.5), (4., -3.5)]
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
        if z != next_z:
            bottom, top = (i, j) if z < next_z else (j, i)
            if radius == next_radius:
                seam_direction = z_axis
            else:
                dr = profile[top][0] - profile[bottom][0]
                dz = profile[top][1] - profile[bottom][1]
                seam_direction = add(f"DIRECTION('',({dr},0.,{dz}))")
            vector = add(f"VECTOR('',{seam_direction},1.)")
            line = add(f"LINE('',{points[bottom]},{vector})")
            seam = add(f"EDGE_CURVE('',{vertices[bottom]},{vertices[top]},{line},.T.)")
            bounds = [loop_bound([(rims[bottom], z < next_z), (seam, True),
                                  (rims[top], z > next_z), (seam, False)])]
            if radius == next_radius:
                surface = add(f"CYLINDRICAL_SURFACE('',{frame(0.)},{radius})")
            else:
                # Cone with reference radius at the lower rim, slant parameter v.
                import math
                lower_radius, lower_z = profile[bottom]
                upper_radius, upper_z = profile[top]
                angle = math.atan2(upper_radius - lower_radius, upper_z - lower_z)
                surface = add(f"CONICAL_SURFACE('',{frame(lower_z)},{lower_radius},{angle})")
            same_sense = z < next_z
        else:
            assert z == next_z
            bounds = [loop_bound([(rims[i], True)], radius > next_radius),
                      loop_bound([(rims[j], False)], radius < next_radius)]
            surface = add(f"PLANE('',{frame(z)})")
            same_sense = radius > next_radius
        faces.append(add(f"ADVANCED_FACE('',({','.join(bounds)}),{surface},{flag(same_sense)})"))
    shell = add(f"CLOSED_SHELL('',({','.join(faces)}))")
    solid = add(f"MANIFOLD_SOLID_BREP('original chamfered disc',{shell})")
    mm = add("(LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.))")
    rad = add("(NAMED_UNIT(*) PLANE_ANGLE_UNIT() SI_UNIT($,.RADIAN.))")
    sterad = add("(NAMED_UNIT(*) SI_UNIT($,.STERADIAN.) SOLID_ANGLE_UNIT())")
    uncertainty = add(f"UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-7),{mm},'distance_accuracy_value','')")
    context = add(f"(GEOMETRIC_REPRESENTATION_CONTEXT(3) GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT(({uncertainty})) GLOBAL_UNIT_ASSIGNED_CONTEXT(({mm},{rad},{sterad})) REPRESENTATION_CONTEXT('','3D'))")
    shape = add(f"ADVANCED_BREP_SHAPE_REPRESENTATION('',({frame(0.)},{solid}),{context})")
    application = add("APPLICATION_CONTEXT('core data for automotive mechanical design processes')")
    add(f"APPLICATION_PROTOCOL_DEFINITION('international standard','automotive_design',2000,{application})")
    product_context = add(f"PRODUCT_CONTEXT('',{application},'mechanical')")
    product = add(f"PRODUCT('turned_body_disc','original analytic fixture','',({product_context}))")
    formation = add(f"PRODUCT_DEFINITION_FORMATION('','',{product})")
    definition_context = add(f"PRODUCT_DEFINITION_CONTEXT('part definition',{application},'design')")
    definition = add(f"PRODUCT_DEFINITION('design','',{formation},{definition_context})")
    product_shape = add(f"PRODUCT_DEFINITION_SHAPE('','',{definition})")
    add(f"SHAPE_DEFINITION_REPRESENTATION({product_shape},{shape})")
    header = """ISO-10303-21;
HEADER;
FILE_DESCRIPTION(('Original Apache-2.0 analytic turned disc fixture; no external model'),'2;1');
FILE_NAME('turned_body_disc.step','2026-10-28T00:00:00',('quiddity-rust contributors'),('quiddity-rust'),'tools/make_turned_body_fixture.py','quiddity-rust','Apache-2.0');
FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));
ENDSEC;
DATA;
"""
    data = header + "\n".join(f"#{i} = {value};" for i, value in enumerate(entities, 1))
    path = Path(__file__).resolve().parents[1] / "tests/fixtures/turned_body_disc.step"
    path.write_text(data + "\nENDSEC;\nEND-ISO-10303-21;\n")
    print(path)


if __name__ == "__main__":
    main()
