"""Independent solid centroid evidence, using OCCT's surface evaluator and 3D edges.

Run in the read-only OCCT environment with QUIDDITY pointing to the reference source:
    python tools/centroid_evidence.py CORPUS_FILE > /scratch/centroid.json

No recogniser or Python frame result is used. Green's theorem boundary interpolation
comes from kernel_evidence; the divergence theorem supplies flux moments. Each face
is refined separately, and the report exposes missing faces and convergence residuals.
For imperfectly closed shells we report the zero-first-moment fixed point, as defined
by Part::volume_centroid, rather than an origin-dependent volume integral. This does
not establish that a malformed shell bounds material. Reports belong outside the repo.
"""
from __future__ import annotations

import argparse
from concurrent.futures import ProcessPoolExecutor
import math
import os

from kernel_evidence import (
    _Surface, _integrate, _load, vertex_centre, np, gp_Pnt, gp_Vec,
    TopAbs_REVERSED, json,
)


class FluxSurface(_Surface):
    components = 17

    def densities(self, u, v):
        u, v = self._wrapped(u, v)
        p, du, dv = gp_Pnt(), gp_Vec(), gp_Vec()
        self.adaptor.D1(u, v, p, du, dv)
        x, y, z = (p.X()-self.centre[0], p.Y()-self.centre[1], p.Z()-self.centre[2])
        normal = du.Crossed(dv)
        nx, ny, nz = normal.Coord()
        qn = x*nx+y*ny+z*nz
        return np.array([normal.Magnitude(), qn, nx, ny, nz,
                         x*nx, x*ny, x*nz, y*nx, y*ny, y*nz, z*nx, z*ny, z*nz,
                         x*qn, y*qn, z*qn])


def fixed_point(values, centre):
    s, m = values[1], values[2:5]
    a, b = values[5:14].reshape(3, 3), values[14:17]
    d = np.zeros(3)
    for _ in range(100):
        volume = (s - d @ m) / 3
        if not math.isfinite(volume) or volume <= 0:
            return None
        step = (b - a @ d - s*d + d*(d @ m)) / (4*volume)
        d += step
        if np.linalg.norm(step) <= 1e-12:
            return dict(volume=float((s-d @ m)/3), centroid=(centre+d).tolist(),
                        area=float(values[0]), area_vector=m.tolist())
    return None


def centroid_panel_change(values, change, centre, centroid):
    """Linear propagation of the sum of absolute last-panel changes, in mm.

    This is a convergence diagnostic, not a certified quadrature error bound.
    Per-face relative tests can fail on cancelling moments near zero even when
    the resulting centroid has converged well inside the frame tolerance.
    """
    d = np.array(centroid)-centre
    s, m = values[1], values[2:5]
    a = values[5:14].reshape(3, 3)
    jacobian = (-a-s*np.eye(3)+np.eye(3)*(d @ m)+np.outer(d, m))/4
    residual = (change[14:17] + change[5:14].reshape(3, 3) @ np.abs(d)
                + change[1]*np.abs(d) + np.abs(d)*(np.abs(d) @ change[2:5]))/4
    return (np.abs(np.linalg.inv(jacobian)) @ residual).tolist()


def face_job(job):
    file, indices, centre, panels, tolerance = job
    faces = list(_load(file).faces())
    rows = []
    for index in indices:
        try:
            face = faces[index].wrapped
            surf = FluxSurface(face)
            surf.centre = np.array(centre)
            previous, change = None, None
            converged = False
            for count in panels:
                values = _integrate(face, surf, count)
                if values is None:
                    raise ValueError('boundary has unresolved winding')
                # Area fixes the boundary orientation; face orientation fixes the normal.
                values *= math.copysign(1, values[0])
                values[1:] *= -1 if face.Orientation() == TopAbs_REVERSED else 1
                if previous is not None:
                    change = np.abs(values-previous)
                    if np.all(change <= tolerance*np.maximum(np.abs(values), 1)):
                        converged = True
                        break
                previous = values
            rows.append(dict(face=index, panels=count, converged=converged,
                             values=values.tolist(), change=change.tolist()))
        except Exception as error:
            rows.append(dict(face=index, error=str(error)))
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('file')
    parser.add_argument('--panels', type=int, nargs='+', default=[8, 16, 32, 64])
    parser.add_argument('--tolerance', type=float, default=1e-9)
    args = parser.parse_args()
    if len(args.panels) < 2 or any(n <= 0 for n in args.panels):
        parser.error('--panels needs at least two positive counts')
    part = _load(args.file)
    # Integrate the solid's faces only; a compound may also contain loose shells.
    all_faces = list(part.faces())
    solids = list(part.solids())
    indices = sorted({next(i for i, f in enumerate(all_faces) if f.wrapped.IsSame(sf.wrapped))
                      for solid in solids for sf in solid.faces()})
    if not indices:
        parser.error('no solid: no material centroid reference')
    centre = vertex_centre(part.wrapped)
    size = max(1, math.ceil(len(indices)/18))
    jobs = [(args.file, indices[k:k+size], centre, args.panels, args.tolerance)
            for k in range(0, len(indices), size)]
    rows = []
    with ProcessPoolExecutor(max_workers=min(6, os.cpu_count() or 1)) as pool:
        for batch in pool.map(face_job, jobs):
            rows.extend(batch)
    missing = [r['face'] for r in rows if 'error' in r]
    result = dict(file=args.file, centre=centre, faces=rows, missing=missing,
                  nonconverged=[r['face'] for r in rows if not r.get('converged', False)])
    if not missing:
        totals = np.sum([r['values'] for r in rows], axis=0)
        result['independent'] = fixed_point(totals, np.array(centre))
        change = np.sum([r['change'] for r in rows], axis=0)
        result['sum_absolute_change'] = change.tolist()
        if result['independent'] is not None:
            result['centroid_panel_change'] = centroid_panel_change(
                totals, change, np.array(centre), result['independent']['centroid'])
    print(json.dumps(result, indent=2, allow_nan=False))


if __name__ == '__main__':
    main()
