//! Rust snapshots of every legacy passage call, ring, roster, defining wall and helper question.

use super::common;

use std::io::Read;
use std::path::PathBuf;

use quiddity::Part;
use quiddity::features::Context;
use quiddity::features::passage_compat::principal_projection;
use quiddity::features::passages::{Passage, legacy_roster, same_legacy_passage_geometry};
use quiddity::features::rings::rings;
use quiddity::kernel::step::read_step_file;
use serde_json::{Value, json};

fn dir() -> PathBuf {
    common::fixtures().join("captured/passages")
}

fn load_gz(name: &str) -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(dir().join(name)).unwrap())
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}

fn floats(v: &Value) -> Vec<f64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_f64().unwrap())
        .collect()
}

fn v3(v: &Value) -> [f64; 3] {
    floats(v).try_into().unwrap()
}

fn points(v: &Value) -> Vec<[f64; 2]> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|p| floats(p).try_into().unwrap())
        .collect()
}

fn passage(v: &Value) -> Option<Passage> {
    (!v.is_null()).then(|| Passage {
        axis: v["axis"].as_str().unwrap().into(),
        sides: v["sides"].as_u64().unwrap() as usize,
        length: v["length"].as_f64().unwrap(),
        at: v3(&v["at"]),
        section: points(&v["section"]),
    })
}

#[test]
fn rust_snapshot_passage_calls() {
    let calls: Value =
        serde_json::from_str(&std::fs::read_to_string(dir().join("calls.json")).unwrap()).unwrap();
    let mut parts: std::collections::BTreeMap<String, Part> = Default::default();
    let actual: Vec<Value> = calls["calls"].as_array().unwrap().iter().map(|c| {
        let function = c["function"].as_str().unwrap();
        let file = c["file"].as_str().unwrap();
        let part = parts.entry(file.into()).or_insert_with(|| read_step_file(&dir().join(file)).unwrap());
        json!({"function":function,"file":file,"test":c["test"],"options":c["options"],"result":common::recognise(function,part,&c["options"])})
    }).collect();
    common::snapshot::check(&common::fixtures(), "passage_calls", &json!(actual));
}

/// The port's answer to a captured helper call.
fn helper_answer(call: &Value) -> Value {
    let given = call["given"].as_array().unwrap();
    match call["kind"].as_str().unwrap() {
        "principal_projection" => {
            let interval = floats(&given[4]);
            match principal_projection(
                v3(&given[0]),
                v3(&given[1]),
                v3(&given[2]),
                v3(&given[3]),
                (interval[0], interval[1]),
                &points(&given[5]),
            ) {
                Some(p) => json!([p.axis, p.sides, p.length, p.at, p.section]),
                None => Value::Null,
            }
        }
        "same_legacy_passage_geometry" => json!(same_legacy_passage_geometry(
            passage(&given[0]).as_ref(),
            &passage(&given[1]).unwrap(),
            (!given[2].is_null()).then(|| v3(&given[2])),
        )),
        other => panic!("unknown helper call {other}"),
    }
}

fn check_part(run: &Value, part: &Part) -> Value {
    let ctx = Context::new(part);
    let mut found: Vec<Value> = rings(&ctx).into_iter().map(|r| json!({"nodes":r.nodes,"section":r.section,"axis":r.axis,"low":r.low,"high":r.high,"caps":[r.cap_nodes.0,r.cap_nodes.1]})).collect();
    found.sort_by_key(|r| (r["axis"].as_u64(), r["nodes"].to_string()));
    let roster: Vec<Vec<usize>> = legacy_roster(&ctx)
        .into_iter()
        .map(|(_, mut nodes)| {
            nodes.sort_unstable();
            nodes
        })
        .collect();
    let records = quiddity::recognise_section_passages(part).map_err(|e| e.to_string());
    let defining = quiddity::features::passages::discover_verified(&ctx)
        .map(|found| found.into_iter().map(|o| o.defining).collect::<Vec<_>>())
        .map_err(|e| e.to_string());
    let calls: Vec<Value> = run["calls"]
        .as_array()
        .unwrap()
        .iter()
        .map(helper_answer)
        .collect();
    json!({"rings":found,"roster":roster,"records":records,"defining":defining,"calls":calls})
}

#[test]
fn rust_snapshot_passage_helpers() {
    let helpers = load_gz("helpers.json.gz");
    let corpus = common::corpus_dir();
    assert!(corpus.is_some() || std::env::var_os("QUIDDITY_CORPUS_REQUIRED").is_none());
    let runs = helpers["runs"].as_array().unwrap();
    common::parallel::map(runs, |run| {
        let file = run["file"].as_str().unwrap();
        let source = run["source"].as_str().unwrap();
        let path = match source {
            "test" => common::fixtures().join("captured").join(file),
            "fixture" => common::fixtures().join(file),
            _ => {
                let Some(dir) = &corpus else { return };
                dir.join(file)
            }
        };
        let actual = check_part(run, &read_step_file(&path).unwrap());
        common::snapshot::check(
            &common::fixtures(),
            &format!("passages/{source}/{file}"),
            &actual,
        );
    });
}
