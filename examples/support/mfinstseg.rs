//! Independent dataset evaluation shared by the example and the external-data ratchet.
use std::collections::{BTreeMap, BTreeSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use serde::{Deserialize, Serialize};

pub type Faces = BTreeSet<usize>;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Class {
    pub id: usize,
    pub name: String,
    pub families: Vec<String>,
}

pub fn mapping() -> Vec<Class> {
    #[derive(Deserialize)]
    struct Mapping {
        classes: Vec<Class>,
    }
    let mapping: Mapping =
        serde_json::from_str(include_str!("../../tests/fixtures/mfinstseg_mapping.json")).unwrap();
    assert_eq!(mapping.classes.len(), 25);
    for (i, c) in mapping.classes.iter().enumerate() {
        assert_eq!(c.id, i);
    }
    mapping.classes
}

/// Zero denominators score zero, never vacuous perfect precision or recall.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Counts {
    pub true_positive: u64,
    pub predicted: u64,
    pub labelled: u64,
}
impl Counts {
    pub fn precision(&self) -> f64 {
        ratio(self.true_positive, self.predicted)
    }
    pub fn recall(&self) -> f64 {
        ratio(self.true_positive, self.labelled)
    }
    fn add(&mut self, other: &Self) {
        self.true_positive += other.true_positive;
        self.predicted += other.predicted;
        self.labelled += other.labelled;
    }
}
fn ratio(n: u64, d: u64) -> f64 {
    if d == 0 { 0.0 } else { n as f64 / d as f64 }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Score {
    pub faces: Counts,
    /// None for semantic-only MFCAD++ labels: instances cannot be inferred from adjacency.
    pub instances: Option<Counts>,
}
impl Score {
    pub fn add(&mut self, other: &Self) {
        self.faces.add(&other.faces);
        if let Some(other) = &other.instances {
            self.instances.get_or_insert_default().add(other);
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ModelScore {
    pub id: String,
    pub scores: Vec<Score>,
    /// Read failures/panics remain in denominators as missed labelled faces and instances.
    pub error: Option<String>,
    pub section_recess_refusal: Option<String>,
    pub label_warnings: Vec<String>,
    pub instances_available: bool,
    pub accounting: Accounting,
}

/// Measured on the supported inferred-frame recognition document. Labels never select rules.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Accounting {
    pub feature_as_body: u64,
    pub stock_unexplained: u64,
    #[serde(default)]
    pub stock_body: u64,
    #[serde(default)]
    pub body_reasons: BTreeMap<quiddity::recognition::BodyReason, u64>,
    pub feature: u64,
    pub body: u64,
    pub refused: u64,
    pub unexplained: u64,
}
impl Accounting {
    pub fn add(&mut self, other: &Self) {
        self.feature_as_body += other.feature_as_body;
        self.stock_unexplained += other.stock_unexplained;
        self.stock_body += other.stock_body;
        for (&reason, &count) in &other.body_reasons {
            *self.body_reasons.entry(reason).or_default() += count;
        }
        self.feature += other.feature;
        self.body += other.body;
        self.refused += other.refused;
        self.unexplained += other.unexplained;
    }
    fn measure(groups: &[quiddity::recognition::FaceAccounting], seg: &[usize]) -> Self {
        use quiddity::recognition::FaceAccounting;
        assert_eq!(groups.len(), seg.len());
        let mut out = Self::default();
        for (group, &label) in groups.iter().zip(seg) {
            match group {
                FaceAccounting::Feature { .. } => out.feature += 1,
                FaceAccounting::Body { reason } => {
                    out.body += 1;
                    out.stock_body += u64::from(label == 24);
                    *out.body_reasons.entry(*reason).or_default() += 1;
                    out.feature_as_body += u64::from(label != 24);
                }
                FaceAccounting::Refused { .. } => out.refused += 1,
                FaceAccounting::Unexplained => {
                    out.unexplained += 1;
                    out.stock_unexplained += u64::from(label == 24);
                }
            }
        }
        out
    }
}

#[derive(Deserialize)]
struct Labels {
    seg: BTreeMap<String, usize>,
    inst: Vec<Vec<u8>>,
}

#[derive(Debug)]
pub struct Truth {
    pub seg: Vec<usize>,
    pub instances: Option<Vec<Vec<Faces>>>,
    pub warnings: Vec<String>,
}
impl Truth {
    pub fn read(root: &Path, id: &str) -> Result<Self, String> {
        let bytes = std::fs::read(root.join("labels").join(format!("{id}.json")))
            .map_err(|e| e.to_string())?;
        let mut rows: Vec<(String, Labels)> =
            serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if rows.len() != 1 {
            return Err(format!("{id}: expected one label record"));
        }
        let mut warnings = Vec::new();
        let declared = &rows[0].0;
        if declared != id {
            if declared.is_empty()
                || !declared
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_')
            {
                return Err(format!("{id}: invalid label identity"));
            }
            // A few published files are renamed copies. Accept only provably identical
            // DATA records and byte-identical labels, never just equal face counts.
            fn records(path: &Path) -> Result<BTreeMap<u64, String>, String> {
                use quiddity::kernel::p21::{Document, encode};
                let doc = Document::parse(std::fs::read(path).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
                doc.graph()
                    .entities
                    .iter()
                    .map(|(&id, e)| encode(e).map(|s| (id, s)).map_err(|e| e.to_string()))
                    .collect()
            }
            let original_labels =
                std::fs::read(root.join("labels").join(format!("{declared}.json")))
                    .map_err(|e| e.to_string())?;
            if original_labels != bytes
                || records(&root.join("steps").join(format!("{id}.step")))?
                    != records(&root.join("steps").join(format!("{declared}.step")))?
            {
                return Err(format!(
                    "{id}: label identity differs and is not an identical STEP/label duplicate"
                ));
            }
            warnings.push(format!(
                "label identity {declared}: verified identical STEP DATA records and label bytes"
            ));
        }
        let labels = rows.pop().unwrap().1;
        Self::parse(labels, warnings, id)
    }

    fn parse(labels: Labels, mut warnings: Vec<String>, id: &str) -> Result<Self, String> {
        let seg: Vec<_> = (0..labels.seg.len())
            .map(|i| {
                labels
                    .seg
                    .get(&i.to_string())
                    .copied()
                    .ok_or_else(|| format!("{id}: non-contiguous face labels"))
            })
            .collect::<Result<_, _>>()?;
        if seg.iter().any(|&c| c >= 25) {
            return Err(format!("{id}: unknown class"));
        }
        let n = seg.len();
        if labels.inst.len() != n
            || labels
                .inst
                .iter()
                .any(|r| r.len() != n || r.iter().any(|&v| v > 1))
        {
            return Err(format!("{id}: invalid instance matrix dimensions/values"));
        }
        let mut instances = vec![Vec::new(); 25];
        let rows: Vec<Faces> = labels
            .inst
            .iter()
            .map(|r| {
                r.iter()
                    .enumerate()
                    .filter_map(|(i, &x)| (x == 1).then_some(i))
                    .collect()
            })
            .collect();
        let mut seen = BTreeSet::new();
        let mut unassigned = Vec::new();
        for (i, row) in rows.iter().enumerate() {
            if row.is_empty() {
                if seg[i] != 24 {
                    unassigned.push(i);
                }
                continue;
            }
            if !row.contains(&i)
                || row.iter().any(|&j| rows[j] != *row || seg[j] != seg[i])
                || seg[i] == 24
            {
                return Err(format!(
                    "{id}: instance is not a same-class equivalence clique at face {i}"
                ));
            }
            if seen.insert(row.clone()) {
                instances[seg[i]].push(row.clone());
            }
        }
        let instances = if unassigned.is_empty() {
            Some(instances)
        } else {
            warnings.push(format!("semantic feature faces {unassigned:?} have no instance annotation; face metrics retained, instance metrics unavailable for this model"));
            None
        };
        Ok(Self {
            seg,
            instances,
            warnings,
        })
    }

    /// AAGNet's MFCAD++ extractor reads the numeric STEP representation-item name.
    /// Bind it through the Rust face's source entity, rather than guessing STEP text order.
    pub fn mfcadpp(part: &quiddity::Part, bytes: Vec<u8>) -> Result<Self, String> {
        use quiddity::kernel::p21::{Attribute, Document, RawEntity};
        let doc = Document::parse(bytes).map_err(|e| e.to_string())?;
        let mut seg = Vec::new();
        for i in 0..part.faces.len() {
            let source = part.face_source(i).ok_or("missing face source")?;
            let e = doc.get(source.entity).ok_or("missing source entity")?;
            let RawEntity::Simple {
                name: kind,
                attributes,
                ..
            } = e
            else {
                return Err("complex face entity".into());
            };
            if kind != "ADVANCED_FACE" && kind != "FACE_SURFACE" {
                return Err("source is not a face".into());
            }
            let Some(Attribute::String(name)) = attributes.first() else {
                return Err("missing face name".into());
            };
            let c: usize = name
                .parse()
                .map_err(|_| format!("face {i}: non-numeric class {name:?}"))?;
            if c >= 25 {
                return Err(format!("face {i}: unknown class {c}"));
            }
            seg.push(c);
        }
        Ok(Self {
            seg,
            instances: None,
            warnings: Vec::new(),
        })
    }
}

/// Maximum-cardinality one-to-one matching at inclusive face-set IoU >= 0.5.
/// An augmenting path avoids the order dependence and undercounting of greedy matching.
pub fn matched(predicted: &[Faces], labelled: &[Faces]) -> u64 {
    let edges: Vec<Vec<usize>> = predicted
        .iter()
        .map(|p| {
            labelled
                .iter()
                .enumerate()
                .filter_map(|(i, g)| {
                    let intersection = p.intersection(g).count();
                    let union = p.len() + g.len() - intersection;
                    (union > 0 && 2 * intersection >= union).then_some(i)
                })
                .collect()
        })
        .collect();
    fn augment(
        p: usize,
        edges: &[Vec<usize>],
        used: &mut [bool],
        owner: &mut [Option<usize>],
    ) -> bool {
        for &g in &edges[p] {
            if used[g] {
                continue;
            }
            used[g] = true;
            if owner[g].is_none_or(|old| augment(old, edges, used, owner)) {
                owner[g] = Some(p);
                return true;
            }
        }
        false
    }
    let mut owner = vec![None; labelled.len()];
    (0..predicted.len())
        .filter(|&p| augment(p, &edges, &mut vec![false; labelled.len()], &mut owner))
        .count() as u64
}

pub fn score(
    truth: &Truth,
    defining: &quiddity::features::Defining,
    classes: &[Class],
) -> Vec<Score> {
    classes
        .iter()
        .map(|class| {
            let labelled: Faces = truth
                .seg
                .iter()
                .enumerate()
                .filter_map(|(i, &c)| (c == class.id).then_some(i))
                .collect();
            // Same physical face set under two mapped family names is one instance prediction.
            let instances: BTreeSet<Faces> = class
                .families
                .iter()
                .flat_map(|f| defining.get(f.as_str()).into_iter().flatten())
                .map(|fs| fs.iter().copied().collect())
                .collect();
            let predicted: Faces = instances.iter().flatten().copied().collect();
            assert!(
                predicted.iter().all(|&i| i < truth.seg.len()),
                "out-of-range predicted face"
            );
            let face_counts = Counts {
                true_positive: predicted.intersection(&labelled).count() as u64,
                predicted: predicted.len() as u64,
                labelled: labelled.len() as u64,
            };
            let instance_counts = truth.instances.as_ref().map(|g| Counts {
                true_positive: matched(
                    &instances.iter().cloned().collect::<Vec<_>>(),
                    &g[class.id],
                ),
                predicted: instances.len() as u64,
                labelled: g[class.id].len() as u64,
            });
            Score {
                faces: face_counts,
                instances: instance_counts,
            }
        })
        .collect()
}

pub fn evaluate(
    root: &Path,
    id: &str,
    mfcadpp: bool,
    classes: &[Class],
) -> Result<ModelScore, String> {
    let mut truth = if mfcadpp {
        None
    } else {
        Some(Truth::read(root, id)?)
    };
    let path = if mfcadpp {
        root.join(format!("{id}.step"))
    } else {
        root.join("steps").join(format!("{id}.step"))
    };
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<_, String> {
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        let part = quiddity::read_step(&bytes).map_err(|e| e.to_string())?;
        if mfcadpp {
            truth = Some(Truth::mfcadpp(&part, bytes)?);
        }
        if part.faces.len() != truth.as_ref().unwrap().seg.len() {
            return Err("STEP/label face count mismatch".into());
        }
        // Keep the historical caller-space feature scorer unchanged. Accounting uses
        // the supported default route, including frame refusal and document precedence.
        let features = quiddity::features::recognise(&part);
        let recognition = quiddity::correspondence::recognise(&part);
        let doc = quiddity::recognition::document(&recognition, part.faces.len());
        let accounting = Accounting::measure(
            doc.face_accounting.as_ref().unwrap(),
            &truth.as_ref().unwrap().seg,
        );
        Ok((features, accounting))
    }));
    let (features, accounting, error) = match result {
        Ok(Ok((f, a))) => (Some(f), Some(a), None),
        Ok(Err(e)) => (None, None, Some(e)),
        Err(e) => (
            None,
            None,
            Some(
                e.downcast_ref::<String>()
                    .cloned()
                    .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_else(|| "recognition panic".into()),
            ),
        ),
    };
    // Without labels no accuracy denominator is possible: propagate, never silently skip.
    let truth =
        truth.ok_or_else(|| format!("{id}: {}", error.as_deref().unwrap_or("missing labels")))?;
    let empty = BTreeMap::new();
    Ok(ModelScore {
        id: id.into(),
        // Failed recognition is fully unexplained, never silently absent from totals.
        accounting: accounting.unwrap_or_else(|| Accounting {
            unexplained: truth.seg.len() as u64,
            stock_unexplained: truth.seg.iter().filter(|&&c| c == 24).count() as u64,
            ..Accounting::default()
        }),
        scores: score(
            &truth,
            features.as_ref().map_or(&empty, |f| &f.defining),
            classes,
        ),
        error,
        section_recess_refusal: features
            .and_then(|f| f.section_recess_refusal.map(|e| e.to_string())),
        label_warnings: truth.warnings,
        instances_available: truth.instances.is_some(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accounting_measures_all_feature_labels_and_only_explicit_stock() {
        use quiddity::recognition::{BodyReason, FaceAccounting};
        let groups = vec![
            FaceAccounting::Body {
                reason: BodyReason::StockBoxSide,
            },
            FaceAccounting::Body {
                reason: BodyReason::StockBoxSide,
            },
            FaceAccounting::Unexplained,
            FaceAccounting::Unexplained,
        ];
        let counts = Accounting::measure(&groups, &[23, 24, 24, 0]);
        assert_eq!(counts.feature_as_body, 1);
        assert_eq!(counts.stock_unexplained, 1);
        assert_eq!(counts.stock_body, 1);
        assert_eq!(counts.body_reasons[&BodyReason::StockBoxSide], 2);
        assert_eq!(counts.body, 2);
        assert_eq!(counts.unexplained, 2);
    }
    fn faces(fs: &[usize]) -> Faces {
        fs.iter().copied().collect()
    }
    #[test]
    fn incomplete_instances_retain_semantics_but_invalid_cliques_fail() {
        let partial = Labels {
            seg: BTreeMap::from([("0".into(), 0), ("1".into(), 0)]),
            inst: vec![vec![1, 0], vec![0, 0]],
        };
        let truth = Truth::parse(partial, Vec::new(), "partial").unwrap();
        assert_eq!(truth.seg, vec![0, 0]);
        assert!(truth.instances.is_none());
        assert_eq!(truth.warnings.len(), 1);
        let invalid = Labels {
            seg: BTreeMap::from([("0".into(), 0), ("1".into(), 0)]),
            inst: vec![vec![1, 1], vec![0, 1]],
        };
        assert!(Truth::parse(invalid, Vec::new(), "invalid").is_err());
    }
    #[test]
    fn inclusive_iou_and_maximum_matching() {
        assert_eq!(
            matched(&[faces(&[0, 1]), faces(&[0])], &[faces(&[0]), faces(&[1])]),
            2
        );
        assert_eq!(matched(&[faces(&[0, 1, 2])], &[faces(&[0])]), 0);
        assert_eq!(matched(&[Faces::new()], &[Faces::new()]), 0);
        assert_eq!(matched(&[faces(&[0]), faces(&[0])], &[faces(&[0])]), 1);
    }
    #[test]
    fn empty_face_claim_is_a_false_positive_instance() {
        let classes = vec![Class {
            id: 0,
            name: "test".into(),
            families: vec!["holes".into()],
        }];
        let truth = Truth {
            seg: vec![0],
            instances: Some(vec![vec![faces(&[0])]]),
            warnings: Vec::new(),
        };
        let defining = BTreeMap::from([("holes", vec![vec![]])]);
        let s = score(&truth, &defining, &classes);
        assert_eq!(
            s[0].instances,
            Some(Counts {
                true_positive: 0,
                predicted: 1,
                labelled: 1
            })
        );
    }
    #[test]
    fn defining_only_deduplicated_predictions_do_not_consult_labels() {
        let classes = vec![Class {
            id: 0,
            name: "test".into(),
            families: vec!["holes".into(), "pockets".into()],
        }];
        let truth = Truth {
            seg: vec![0, 1, 0],
            instances: Some(vec![vec![faces(&[0, 2])]]),
            warnings: Vec::new(),
        };
        let defining = BTreeMap::from([("holes", vec![vec![0, 1]]), ("pockets", vec![vec![0, 1]])]);
        let s = score(&truth, &defining, &classes);
        assert_eq!(
            s[0].faces,
            Counts {
                true_positive: 1,
                predicted: 2,
                labelled: 2
            }
        );
        assert_eq!(
            s[0].instances,
            Some(Counts {
                true_positive: 0,
                predicted: 1,
                labelled: 1
            })
        );
        assert_eq!(Counts::default().precision(), 0.0);
        assert_eq!(Counts::default().recall(), 0.0);
    }
}
