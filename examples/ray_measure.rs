//! Temporary (uncommitted): time and fingerprint inventory over the corpus and the
//! section-recess documents.
use sha2::Digest;
use std::io::Read;
fn main() {
    let corpus = std::path::PathBuf::from(std::env::var("QUIDDITY_CORPUS").unwrap());
    let listing: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string("tests/fixtures/corpus.json").unwrap())
            .unwrap();
    let files: Vec<_> = listing["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| corpus.join(f["file"].as_str().unwrap()))
        .collect();
    let mut h = sha2::Sha256::new();
    let t0 = std::time::Instant::now();
    for f in &files {
        if std::env::var("SKIP_CORPUS").is_ok() {
            break;
        }
        let part = quiddity::read_step_file(f).unwrap();
        h.update(format!("{:?}", quiddity::features::inventory(&part)).as_bytes());
    }
    let t1 = t0.elapsed();
    let dir = std::path::PathBuf::from("tests/fixtures/captured/section_recesses");
    let mut text = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(dir.join("documents.json.gz")).unwrap())
        .read_to_string(&mut text)
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let mut n = 0;
    for r in v["runs"].as_array().unwrap() {
        if std::env::var("SKIP_RECESS").is_ok() {
            break;
        }
        if r["document"].get("timeout").is_some() {
            continue;
        }
        let file = r["file"].as_str().unwrap();
        let path = match r["source"].as_str().unwrap() {
            "test" => dir.join(file),
            "fixture" => std::path::PathBuf::from("tests/fixtures").join(file),
            _ => corpus.join(file),
        };
        let part = quiddity::read_step_file(&path).unwrap();
        h.update(
            format!(
                "{:?}",
                quiddity::features::section_recess_family::build_section_recess_document(&part)
            )
            .as_bytes(),
        );
        n += 1;
        if std::env::var("LIMIT").is_ok_and(|l| n >= l.parse().unwrap()) {
            break;
        }
    }
    println!(
        "corpus {} files {:.2?}; section recess {n} runs {:.2?}; digest {}",
        files.len(),
        t1,
        t0.elapsed() - t1,
        h.finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
}
