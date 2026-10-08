//! The named WHERE and UNIQUE rule checks (`express_rules.rs`): every rule's label is the
//! schema's, every rule has a passing and a failing hand-made instance set
//! (`tests/fixtures/ap242/rules/{valid,invalid}.stp`, the expected violations in
//! `invalid.json`), and every NIST AP242 file's violations are pinned by file and rule with a
//! verdict (`known_nist_rule_violations.json`; `HAECCEITY_RULES_ACTUAL=<dir>` writes the
//! computed list there).
//!
//! NIST files: `HAECCEITY_NIST_PMI` names their `NIST-PMI-STEP-Files` directory (else the AP242
//! track's download); skipped when absent unless `HAECCEITY_NIST_PMI_REQUIRED=1`.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use haecceity::express;
use haecceity::express_rules::{RULES, RuleViolation, check_all};
use haecceity::p21::Document;
use serde_json::{Value, json};

fn fixtures() -> PathBuf {
    common::fixtures().join("ap242/rules")
}

fn load(name: &str) -> Document {
    Document::parse(std::fs::read(fixtures().join(name)).unwrap()).unwrap()
}

/// NIST's MBE PMI test models: `HAECCEITY_NIST_PMI`, else the AP242 track's download.
fn nist_dir() -> Option<PathBuf> {
    let dir = std::env::var("HAECCEITY_NIST_PMI")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(
                "/private/tmp/claude-501/-Users-paul-repos-quiddity-rust/\
                 98a3d09f-22d9-418c-b8db-3b48db0cb346/scratchpad/ap242-nist/NIST-PMI-STEP-Files",
            )
        });
    if dir.is_dir() {
        Some(dir)
    } else {
        assert!(
            std::env::var_os("HAECCEITY_NIST_PMI_REQUIRED").is_none(),
            "NIST PMI files required but {} is missing",
            dir.display()
        );
        None
    }
}

#[test]
fn rule_labels_are_the_schemas() {
    let mut seen = BTreeSet::new();
    for r in RULES {
        assert!(seen.insert(r.rule), "{} listed twice", r.rule);
        assert_eq!(r.rule, format!("{}.{}", r.entity, r.label));
        assert!(
            express::is_declared(r.entity),
            "{} is not declared",
            r.entity
        );
        if r.label.starts_with("WR") || r.label.starts_with("UR") {
            assert!(
                express::rules(r.entity).any(|l| l == r.label),
                "{} has no rule {}",
                r.entity,
                r.label
            );
        } else {
            // An INVERSE attribute's cardinality: the attribute is the entity's own.
            assert_eq!(r.rule, "datum_reference_compartment.owner");
        }
    }
    // The rules the writer relies on, by the design's list.
    let want = |e: &str, labels: &[&str]| {
        for l in labels {
            assert!(
                seen.contains(format!("{e}.{l}").as_str()),
                "{e}.{l} missing"
            );
        }
    };
    let wr = |n: usize| (1..=n).map(|i| format!("WR{i}")).collect::<Vec<_>>();
    want(
        "thread",
        &wr(16).iter().map(String::as_str).collect::<Vec<_>>(),
    );
    want(
        "turned_knurl",
        &wr(12).iter().map(String::as_str).collect::<Vec<_>>(),
    );
    want("default_tolerance_table", &["WR1", "WR2"]);
    want(
        "default_tolerance_table_cell",
        &["WR1", "WR2", "WR3", "WR4", "WR5"],
    );
    want("datum", &["WR1", "WR2", "WR3", "WR4", "UR1"]);
    want("datum_feature", &["WR1", "WR2"]);
    want("datum_target", &["WR1", "WR2", "WR3", "WR4", "WR5", "UR1"]);
    want("placed_datum_target_feature", &["WR1", "WR2", "WR3"]);
    want("datum_system", &["WR1", "UR1"]);
    want("geometric_tolerance", &["WR1", "WR3", "WR5"]);
    want("geometric_tolerance_with_datum_reference", &["WR1"]);
    for form in [
        "flatness_tolerance",
        "straightness_tolerance",
        "roundness_tolerance",
        "cylindricity_tolerance",
    ] {
        want(form, &["WR1"]);
    }
    want("geometric_tolerance_relationship", &["WR1", "WR2"]);
    want("plus_minus_tolerance", &["UR1"]);
    want(
        "item_identified_representation_usage",
        &["UR1", "UR2", "WR1"],
    );
}

/// The valid instance sets violate nothing, and hold an instance of every rule's entity.
#[test]
fn valid_instances_pass_every_rule() {
    let doc = load("valid.stp");
    let v = check_all(&doc);
    assert!(
        v.is_empty(),
        "{}",
        v.iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    );
    for r in RULES {
        assert!(
            doc.ids().any(|id| {
                let e = doc.get(id).unwrap();
                let names: Vec<String> = match e {
                    haecceity::p21::RawEntity::Simple { name, .. } => vec![name.clone()],
                    haecceity::p21::RawEntity::Complex { parts, .. } => {
                        parts.iter().map(|p| p.name.clone()).collect()
                    }
                };
                names.iter().any(|n| express::is_a(n, r.entity))
            }),
            "no {} in valid.stp, so {} has no passing case",
            r.entity,
            r.rule
        );
    }
}

/// Each invalid instance set violates the rules `invalid.json` names, and nothing else; every
/// rule has a failing case.
#[test]
fn invalid_instances_fail_their_rules() {
    let doc = load("invalid.stp");
    let expected: Value =
        serde_json::from_str(&std::fs::read_to_string(fixtures().join("invalid.json")).unwrap())
            .unwrap();
    let want: BTreeSet<(u64, String)> = expected["violations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            (
                v["id"].as_u64().unwrap(),
                v["rule"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    let got: BTreeSet<(u64, String)> = check_all(&doc)
        .iter()
        .map(|v| (v.id, v.rule.to_string()))
        .collect();
    let all = check_all(&doc);
    let missing: Vec<_> = want.difference(&got).collect();
    let extra: Vec<String> = all
        .iter()
        .filter(|v| !want.contains(&(v.id, v.rule.to_string())))
        .map(ToString::to_string)
        .collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "missing {missing:?}\nunexpected:\n{}",
        extra.join("\n")
    );
    for r in RULES {
        assert!(
            want.iter().any(|(_, rule)| rule == r.rule),
            "{} has no failing case in invalid.stp",
            r.rule
        );
    }
}

#[test]
fn check_all_is_deterministic() {
    let doc = load("invalid.stp");
    assert_eq!(check_all(&doc), check_all(&load("invalid.stp")));
}

fn groups(v: &[RuleViolation]) -> BTreeMap<String, u64> {
    let mut out = BTreeMap::new();
    for x in v {
        *out.entry(x.rule.to_string()).or_insert(0) += 1;
    }
    out
}

const VERDICTS: [&str; 1] = ["file-wrong"];

/// Every NIST AP242 file's violations, by rule, pinned with a verdict and reason. A check
/// error is fixed in the check, not pinned: every pinned group is the file's.
#[test]
fn nist_rule_violations_are_pinned() {
    let Some(dir) = nist_dir() else { return };
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".stp") && n.contains("_ap242"))
        .collect();
    names.sort();
    assert!(!names.is_empty());
    let results = common::parallel::map(&names, |n| {
        let doc = Document::parse(std::fs::read(dir.join(n)).unwrap()).unwrap();
        let v = check_all(&doc);
        let examples: BTreeMap<String, String> = v
            .iter()
            .rev()
            .map(|x| (x.rule.to_string(), x.to_string()))
            .collect();
        (n.clone(), groups(&v), examples)
    });
    let pinned: Value = serde_json::from_str(
        &std::fs::read_to_string(fixtures().join("known_nist_rule_violations.json")).unwrap(),
    )
    .unwrap();
    let mut problems = Vec::new();
    let mut now = serde_json::Map::new();
    for (name, groups, examples) in &results {
        let p = &pinned["files"][name.as_str()];
        let mut entry = serde_json::Map::new();
        for (rule, count) in groups {
            let g = &p[rule.as_str()];
            if g["count"].as_u64() != Some(*count) {
                problems.push(format!("{name}: {rule} x{count}, pinned {}", g["count"]));
            }
            if !g["verdict"].as_str().is_some_and(|v| VERDICTS.contains(&v))
                || !g["reason"].as_str().is_some_and(|r| !r.is_empty())
            {
                problems.push(format!("{name}: {rule} has no verdict or reason"));
            }
            let mut e = json!({
                "count": count,
                "example": examples[rule],
            });
            for k in ["verdict", "reason"] {
                if let Some(v) = g.get(k) {
                    e[k] = v.clone();
                }
            }
            entry.insert(rule.clone(), e);
        }
        for (rule, _) in p.as_object().into_iter().flatten() {
            if !groups.contains_key(rule) {
                problems.push(format!("{name}: {rule} is pinned but not found"));
            }
        }
        now.insert(name.clone(), Value::Object(entry));
    }
    for name in pinned["files"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(k, _)| k)
    {
        if !now.contains_key(name) {
            problems.push(format!("{name} is pinned but not among the files"));
        }
    }
    if let Some(d) = std::env::var_os("HAECCEITY_RULES_ACTUAL") {
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(
            Path::new(&d).join("known_nist_rule_violations.json"),
            serde_json::to_string_pretty(&json!({"files": now})).unwrap() + "\n",
        )
        .unwrap();
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
