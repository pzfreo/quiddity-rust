//! Independent file-space reconstruction for the caller-space equivalence verdicts.
use quiddity::correspondence::Recognition;
use quiddity::features::section_recess::{EndSurface, SectionRecess};
use quiddity::frames::RecordFrame;

fn samples(record: &SectionRecess, recognition: &Recognition) -> Vec<[f64; 3]> {
    let geometry = record.geometry();
    let frame = geometry.frame();
    let boundary = geometry.profile().boundary();
    let mut points = Vec::new();
    for (i, vertex) in boundary.iter().enumerate() {
        points.push(vertex.point);
        if vertex.bulge != 0.0 {
            let a = vertex.point;
            let b = boundary[(i + 1) % boundary.len()].point;
            // Bulge is tan(sweep/4); the arc's midpoint is one sagitta from its chord.
            points.push([
                (a[0] + b[0] + vertex.bulge * (b[1] - a[1])) / 2.0,
                (a[1] + b[1] - vertex.bulge * (b[0] - a[0])) / 2.0,
            ]);
        }
    }
    let local = recognition
        .local
        .contains_key(&format!("section_recesses/{}", record.index()));
    let (low, high) = geometry.run_interval();
    points
        .into_iter()
        .flat_map(|point| {
            [(geometry.ends().low(), low), (geometry.ends().high(), high)].map(|(end, run)| {
                let height = match end.surface() {
                    EndSurface::Planar(plane) => {
                        let gradient = plane.gradient();
                        run + gradient[0] * point[0] + gradient[1] * point[1]
                    }
                    EndSurface::Cylindrical(cylinder) => cylinder.height(point).unwrap(),
                    EndSurface::PlanarEnvelope(envelope) => envelope.height(point).unwrap(),
                };
                let p = std::array::from_fn(|i| {
                    frame.origin[i]
                        + height * frame.run[i]
                        + point[0] * frame.u[i]
                        + point[1] * frame.v[i]
                });
                if local {
                    let RecordFrame::Inferred(part_frame) = &recognition.frame else {
                        panic!("local geometry without inferred frame")
                    };
                    part_frame.to_world(p)
                } else {
                    p
                }
            })
        })
        .collect()
}

pub fn check(file: &str, caller: &Recognition, framed: &Recognition) {
    let ar = &caller.features.section_recess_refusals;
    let br = &framed.features.section_recess_refusals;
    assert_eq!(ar.len(), br.len(), "{file}: equivalent refusal count");
    for refusal in ar {
        let other = br
            .iter()
            .find(|r| {
                r.body() == refusal.body()
                    && r.evidence().constituent_faces() == refusal.evidence().constituent_faces()
            })
            .unwrap_or_else(|| panic!("{file}: equivalent refused region missing"));
        let a = serde_json::to_value(refusal).unwrap();
        let b = serde_json::to_value(other).unwrap();
        assert_eq!(
            a["reason"], b["reason"],
            "{file}: equivalent refusal reason"
        );
        if refusal.evidence().defining_faces() != other.evidence().defining_faces() {
            // Python reproduces these two source associations for the same refused region.
            assert_eq!(file, "mfcadpp_holdout/163.step");
            assert_eq!(refusal.evidence().defining_faces(), [11, 12]);
            assert_eq!(other.evidence().defining_faces(), [10, 11, 12, 13]);
            assert_eq!(refusal.evidence().constituent_faces(), [10, 11, 12, 13, 32]);
        }
    }
    let a = &caller.features.section_recesses;
    let b = &framed.features.section_recesses;
    assert_eq!(a.len(), b.len(), "{file}: equivalent record count");
    for record in a {
        let other = b
            .iter()
            .find(|r| r.body() == record.body() && r.evidence() == record.evidence())
            .unwrap_or_else(|| panic!("{file}: equivalent region missing"));
        assert_eq!(
            record.classification(),
            other.classification(),
            "{file}: equivalent classification"
        );
        assert_eq!(
            record.geometry().profile().is_closed(),
            other.geometry().profile().is_closed(),
            "{file}: equivalent closure"
        );
        let conditions = |r: &SectionRecess| {
            let mut ends = [
                r.geometry().ends().low().condition(),
                r.geometry().ends().high().condition(),
            ];
            ends.sort_unstable();
            ends
        };
        assert_eq!(
            conditions(record),
            conditions(other),
            "{file}: equivalent end conditions"
        );
        let (a, b) = (samples(record, caller), samples(other, framed));
        assert_eq!(a.len(), b.len(), "{file}: equivalent sample count");
        for (source, target) in [(&a, &b), (&b, &a)] {
            for p in source {
                let nearest = target
                    .iter()
                    .map(|q| (0..3).map(|i| (p[i] - q[i]).powi(2)).sum::<f64>().sqrt())
                    .fold(f64::INFINITY, f64::min);
                assert!(
                    nearest <= 0.01,
                    "{file}: equivalent section geometry differs by {nearest} mm"
                );
            }
        }
    }
}
