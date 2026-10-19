//! Geometry census for the modelled-thread survey.
use quiddity::kernel::geom::{self, Curve, Surface, V3};
use std::collections::BTreeMap;
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    for name in args {
        let part = quiddity::read_step_file(std::path::Path::new(&name)).unwrap();
        let mut kinds = BTreeMap::new();
        let mut axes = Vec::<(V3, V3)>::new();
        for f in &part.faces {
            let k = match &f.surface {
                Surface::Freeform { kind, .. } | Surface::Other { kind } => kind.to_string(),
                s => format!("{:?}", s.kind()),
            };
            *kinds.entry(k).or_insert(0) += 1;
            let frame = match f.surface {
                Surface::Cylinder { frame, .. }
                | Surface::Cone { frame, .. }
                | Surface::Torus { frame, .. } => Some(frame),
                _ => None,
            };
            if let Some(frame) = frame {
                let mut d = frame.z;
                if d[geom::dominant_axis_preferring_z(d)] < 0.0 {
                    d = geom::scale(d, -1.0);
                }
                let p = geom::sub(frame.origin, geom::scale(d, geom::dot(frame.origin, d)));
                if !axes
                    .iter()
                    .any(|(q, e)| geom::dist(p, *q) < 1e-5 && geom::dist(d, *e) < 1e-5)
                {
                    axes.push((p, d));
                }
            }
        }
        let mut helices = Vec::new();
        for (ai, &(p, d)) in axes.iter().enumerate() {
            let x = geom::default_x_axis(d);
            let y = geom::cross(d, x);
            for (ei, e) in part.edges.iter().enumerate() {
                if !matches!(e.curve, Curve::Nurbs(_)) || e.samples.len() < 5 {
                    continue;
                }
                let mut a = Vec::new();
                let mut z = Vec::new();
                let mut r = Vec::new();
                for &s in &e.samples {
                    let q = geom::sub(s, p);
                    let u = geom::dot(q, x);
                    let v = geom::dot(q, y);
                    let theta = v.atan2(u);
                    a.push(
                        a.last()
                            .map_or(theta, |&prev| geom::nearest_turn(theta, prev)),
                    );
                    z.push(geom::dot(q, d));
                    r.push(u.hypot(v));
                }
                let da = a[a.len() - 1] - a[0];
                let dz = z[z.len() - 1] - z[0];
                let rlo = r.iter().copied().fold(f64::INFINITY, f64::min);
                let rhi = r.iter().copied().fold(0.0, f64::max);
                if da.abs() < 0.7 || dz.abs() < 1e-4 || rhi - rlo > 0.01_f64.max(rhi * 0.002) {
                    continue;
                }
                let slope = dz / da;
                let err = a
                    .iter()
                    .zip(&z)
                    .map(|(aa, zz)| (zz - (z[0] + slope * (aa - a[0]))).abs())
                    .fold(0.0, f64::max);
                if err > 0.01_f64.max(dz.abs() * 0.002) {
                    continue;
                }
                helices.push(serde_json::json!({"axis":ai,"edge":ei,"faces":part.edge_faces()[ei],"radius":[rlo,rhi],"lead":slope*std::f64::consts::TAU,"turns":da/std::f64::consts::TAU,"z":[z.iter().copied().fold(f64::INFINITY,f64::min),z.iter().copied().fold(f64::NEG_INFINITY,f64::max)],"error":err}));
            }
        }
        if let Ok(dir) = std::env::var("QUIDDITY_SURVEY_DETAIL") {
            let faces: Vec<_> = part.faces.iter().enumerate().map(|(fi,f)| {
                let mut points = Vec::new();
                if let Some((u0,u1,v0,v1)) = part.uv_bounds(fi) {
                    for i in 0..=16 { for j in 0..=16 {
                        let u=u0+(u1-u0)*i as f64/16.0;let v=v0+(v1-v0)*j as f64/16.0;
                        points.push(serde_json::json!([f.surface.value(u,v),part.face_normal(fi,u,v),part.domain(fi).is_some_and(|d|d.contains(u,v)),[u,v]]));
                    }}
                }
                let kind = match &f.surface {Surface::Freeform{kind,..}=>kind.to_string(),s=>format!("{:?}",s.kind())};
                serde_json::json!({"face":fi,"kind":kind,"solid":f.solid,"edges":part.face_edges(fi),"neighbours":part.neighbours(fi),"points":points})
            }).collect();
            let edges: Vec<_> = part.edges.iter().enumerate().map(|(ei,e)|serde_json::json!({"edge":ei,"points":e.samples,"faces":part.edge_faces()[ei]})).collect();
            let out = std::path::Path::new(&dir).join(format!(
                "{}.json",
                std::path::Path::new(&name)
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
            ));
            std::fs::write(
                out,
                serde_json::to_vec(&serde_json::json!({"faces":faces,"edges":edges,"axes":axes}))
                    .unwrap(),
            )
            .unwrap();
        }
        println!(
            "{}",
            serde_json::json!({"file":name,"faces":part.faces.len(),"kinds":kinds,"axes":axes,"helices":helices})
        );
    }
}
