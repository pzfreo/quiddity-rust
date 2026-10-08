//! The read-only geometry facade (`quiddity.experimental_geometry.GeometryGraph`): one run's
//! face graph over a set of faces (a solid's, or the whole part's), with each face's planarity,
//! normal, box and effective analytic surface, its neighbours and arcs, and the native
//! cylindrical blend chains among the faces with the support bridges a selection of them
//! collapses to.
//!
//! Python hands out opaque handles (`FaceRef`, `BlendRef`, `BoundaryRef`) and checks on every
//! call that they came from this graph. Here a face is its index into the part and a blend its
//! index into [`GeometryGraph::blend_facts`]; the graph borrows its part, so it cannot outlive
//! or change under it, and a face outside the graph's faces is the caller's mistake (it panics
//! where Python raises). The surface inspection values are [`AnalyticFact`] and
//! [`SurfaceKind`]; `surface_anchor` and `smooth_region` have no Rust reader yet and are not
//! ported.

use std::collections::BTreeSet;
use std::sync::OnceLock;

pub use super::analytic_surfaces::{AnalyticFact, SurfaceKind};
use super::blend_view::{BlendChain, BlendGraph, Shared, SmoothSide};
use super::graph;
use crate::kernel::brep::{Arc, Part};
use crate::kernel::geom::V3;

/// One complete native cylindrical blend chain (`BlendFact`): its rolling faces, its two
/// support regions, its proved side and radius, and every original edge occurrence that
/// bounds it (spring, internal and terminal, `BlendFact.boundary`).
#[derive(Clone, Debug, PartialEq)]
pub struct BlendFact {
    /// Ascending.
    pub blend_faces: Vec<usize>,
    /// Each ascending, the two ordered by their least face.
    pub supports: [Vec<usize>; 2],
    /// Convex or concave.
    pub side: SmoothSide,
    pub radius: f64,
    pub boundary: Vec<Shared>,
}

/// The logical arc a selected chain collapses to between its two single-face supports
/// (`CollapsedBridge`): its kind (the chain's side as an arc) and its provenance, the faces and
/// original edge occurrences it stands for.
#[derive(Clone, Debug, PartialEq)]
pub struct CollapsedBridge {
    /// The supports of the chain asked about.
    pub supports: (usize, usize),
    pub kind: Arc,
    /// Ascending.
    pub faces: Vec<usize>,
    /// In `_arc_key` order.
    pub boundary: Vec<Shared>,
}

/// `GeometryGraph`: the facade over one run's faces.
pub struct GeometryGraph<'a> {
    pub part: &'a Part,
    /// Ascending.
    faces: Vec<usize>,
    blends: BlendGraph<'a>,
    facts: OnceLock<Vec<BlendFact>>,
}

impl<'a> GeometryGraph<'a> {
    /// The graph over every face of the part (`GeometryGraph(part)`).
    pub fn new(part: &'a Part) -> Self {
        Self::over(part, (0..part.faces.len()).collect())
    }

    /// The graph over one solid's faces (`GeometryGraph(solid)`).
    pub fn for_solid(part: &'a Part, solid: usize) -> Self {
        Self::over(part, part.solids[solid].faces.clone())
    }

    fn over(part: &'a Part, mut faces: Vec<usize>) -> Self {
        faces.sort_unstable();
        faces.dedup();
        GeometryGraph {
            part,
            faces,
            blends: BlendGraph::new(part),
            facts: OnceLock::new(),
        }
    }

    /// The graph's faces, ascending (`faces`).
    pub fn faces(&self) -> &[usize] {
        &self.faces
    }

    pub fn len(&self) -> usize {
        self.faces.len()
    }

    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }

    pub fn contains(&self, face: usize) -> bool {
        self.faces.binary_search(&face).is_ok()
    }

    fn require(&self, face: usize) -> usize {
        assert!(
            self.contains(face),
            "face {face} is foreign to this geometry graph"
        );
        face
    }

    /// The graph's faces sharing an edge with *face*, each once, in the face's edge order.
    pub fn neighbours(&self, face: usize) -> Vec<usize> {
        let mut out = self.part.neighbours(self.require(face));
        out.retain(|&n| self.contains(n));
        out
    }

    /// `arc`: the kind of the join between two neighbours.
    pub fn arc(&self, a: usize, b: usize) -> Option<Arc> {
        self.blends.arc(self.require(a), self.require(b))
    }

    /// `smooth_side`: the proved side of a smooth join.
    pub fn smooth_side(&self, a: usize, b: usize) -> Option<SmoothSide> {
        self.blends.smooth_side(self.require(a), self.require(b))
    }

    /// `bounds`: the face's box as `(low, high)` per axis.
    pub fn bounds(&self, face: usize) -> [(f64, f64); 3] {
        let b = self.part.face_bounds(self.require(face));
        [0, 1, 2].map(|i| (b.min[i], b.max[i]))
    }

    /// `normal`: the unit normal (`face.normal_at()`), `None` where the face has none.
    pub fn normal(&self, face: usize) -> Option<V3> {
        graph::normal(self.part, self.require(face))
    }

    /// `is_planar`: a native plane.
    pub fn is_planar(&self, face: usize) -> bool {
        graph::is_planar(self.part, self.require(face))
    }

    /// `surface_fact`, when it is an `AnalyticSurface`: the native surface, or the one primitive
    /// a B-spline or Bezier face is certified to be. `None` is a `RefusedSurface`.
    pub fn surface_fact(&self, face: usize) -> Option<&AnalyticFact> {
        self.blends.effective(self.require(face))
    }

    /// `blend_facts`: every complete native cylindrical blend chain among the graph's faces, in
    /// discovery order (by least rolling face).
    pub fn blend_facts(&self) -> &[BlendFact] {
        self.facts.get_or_init(|| {
            self.blends
                .chains()
                .into_iter()
                .filter(|c| self.within(c))
                .map(|c| {
                    let mut boundary = c.spring_arcs.clone();
                    boundary.extend(&c.internal_arcs);
                    boundary.extend(&c.terminal_arcs);
                    BlendFact {
                        blend_faces: c.blend_nodes,
                        supports: c.supports,
                        side: c.side,
                        radius: c.radius,
                        boundary,
                    }
                })
                .collect()
        })
    }

    /// Whether every face of the chain is the graph's. The chains are found over the whole part,
    /// where a chain is within one solid ([`BlendChain::solid`]); a graph over one solid keeps
    /// that solid's.
    fn within(&self, chain: &BlendChain) -> bool {
        chain
            .blend_nodes
            .iter()
            .chain(chain.supports.iter().flatten())
            .all(|&f| self.contains(f))
    }

    /// `collapsed_bridges`: for each selected chain (an index into [`Self::blend_facts`]), in
    /// the order given, every synthetic bridge the collapsed view draws between its two
    /// supports, in the view's order (`BlendCollapseIndex._validate_selection`'s). `Err` with
    /// Python's message where Python raises: a selection whose chains overlap or share original
    /// arcs, or a chain whose supports are not single faces (Python's view has no single-face
    /// logical node to look the bridge up from).
    pub fn collapsed_bridges(
        &self,
        selected: &[usize],
    ) -> Result<Vec<CollapsedBridge>, &'static str> {
        let facts = self.blend_facts();
        let chosen: Vec<&BlendFact> = selected.iter().map(|&i| &facts[i]).collect();
        let mut seen_faces: BTreeSet<usize> = BTreeSet::new();
        let mut seen_arcs: Vec<Shared> = Vec::new();
        for fact in &chosen {
            if fact.blend_faces.iter().any(|f| seen_faces.contains(f)) {
                return Err("selected blend chains overlap");
            }
            if fact.boundary.iter().any(|a| seen_arcs.contains(a)) {
                return Err("selected blend chains share original arcs");
            }
            seen_faces.extend(&fact.blend_faces);
            seen_arcs.extend(&fact.boundary);
        }
        // The view's order: by least rolling face (Python breaks ties by the arcs' keys, but the
        // selected chains' rolling faces were just shown to be disjoint).
        let mut view = chosen.clone();
        view.sort_by_key(|f| f.blend_faces[0]);
        let single = |fact: &BlendFact| match &fact.supports {
            [l, r] if l.len() == 1 && r.len() == 1 => Some((l[0], r[0])),
            _ => None,
        };
        let mut out = Vec::new();
        for fact in &chosen {
            let (left, right) =
                single(fact).ok_or("selected blend chain support is not one face")?;
            for bridge in &view {
                if bridge.supports != fact.supports
                    && bridge.supports != [fact.supports[1].clone(), fact.supports[0].clone()]
                {
                    continue;
                }
                let mut faces: Vec<usize> = bridge
                    .blend_faces
                    .iter()
                    .chain(bridge.supports.iter().flatten())
                    .copied()
                    .collect();
                faces.sort_unstable();
                faces.dedup();
                let mut boundary = bridge.boundary.clone();
                boundary.sort_by_key(Shared::key);
                out.push(CollapsedBridge {
                    supports: (left, right),
                    kind: if bridge.side == SmoothSide::Convex {
                        Arc::Convex
                    } else {
                        Arc::Concave
                    },
                    faces,
                    boundary,
                });
            }
        }
        Ok(out)
    }
}
