//! Corpus loops split into slices, so a test runner can spread one long loop over several jobs
//! (`.github/workflows/ci.yml` shards the suite with `cargo nextest`). Slice *k* of *n* is the
//! corpus files at positions `k`, `k + n`, `k + 2n`, … in `corpus.json`'s order: every file is
//! in exactly one slice, and the costliest parts (cgb202, cgb203, cgb242, cgb243, near the
//! start of the list) land in different slices.
//!
//! A listed exception is keyed by its file, so it is checked by the slice that has its file: a
//! slice reports an entry of its own files that no longer differs (or was not checked) exactly as
//! the whole loop did. An entry naming a file in no slice (not in `corpus.json`) is slice 0's, so
//! it is still reported.

#![allow(dead_code)]

/// The files of slice *k* of *n*.
pub fn slice(files: &[String], k: usize, n: usize) -> Vec<String> {
    assert!(k < n, "slice {k} of {n}");
    files.iter().skip(k).step_by(n).cloned().collect()
}

/// Whether slice *k* of *n* checks a listed entry naming *file*: the file is in the slice, or it
/// is in none and *k* is 0.
pub fn owns(files: &[String], k: usize, n: usize, file: &str) -> bool {
    match files.iter().position(|f| f == file) {
        Some(i) => i % n == k,
        None => k == 0,
    }
}

/// That slices `0..n` together are *files*, each once.
pub fn check_cover(files: &[String], n: usize) {
    let mut all: Vec<String> = (0..n).flat_map(|k| slice(files, k, n)).collect();
    let mut want = files.to_vec();
    all.sort();
    want.sort();
    assert_eq!(all, want, "the slices are not the corpus");
    want.dedup();
    assert_eq!(want.len(), files.len(), "corpus.json lists a file twice");
}

/// `sliced!(name, check, [0 => slice_0, 1 => slice_1, …])`: a module *name* with one test per
/// slice, each calling `check(k, n)` (*n* being the number of slices listed), and
/// `slices_cover_the_corpus`, which fails unless the slices listed are exactly `0..n` and together
/// are the corpus (`super::corpus_files()`), each file once.
macro_rules! sliced {
    ($name:ident, $check:path, [$($k:literal => $test:ident),* $(,)?]) => {
        mod $name {
            const SLICES: usize = [$($k),*].len();
            $(
                #[test]
                fn $test() {
                    $check($k, SLICES);
                }
            )*
            #[test]
            fn slices_cover_the_corpus() {
                assert_eq!([$($k as usize),*], std::array::from_fn::<usize, SLICES, _>(|k| k));
                super::slices::check_cover(&super::corpus_files(), SLICES);
            }
        }
    };
}
