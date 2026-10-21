# Contributing to quiddity-rust

Contributions are welcome: bug reports, fixes, features and documentation.

## Rust-first

The regression record is the Rust snapshot in `tests/fixtures/rust-snapshot/`. Python is
no longer consulted. Python-written STEP/PMI, old notes/fingerprints and helper arguments
remain inputs; they do not supply expected recognition results.

Use the independent references in [README.md](README.md#independent-references-by-family)
to establish correctness. Keep OpenCascade, analytic, label, motion, inventory-floor and NIST
checks and their bands intact. Do not discard output to improve a metric or lower a floor.

For an intentional output change, commit source/input changes, export `QUIDDITY_CORPUS`,
then run `python3 tools/rust_snapshot.py`. Review
`python3 tools/rust_snapshot.py --diff BASE_REF` alongside the implementation and evidence;
commit the compressed expectations and provenance. The tool records the source Rust commit
and release anchor. `python3 tools/rust_snapshot.py --check` must reproduce HEAD exactly.
Normal tests cannot accept a changed output. Do not hand-edit snapshots or restore a Python
oracle. Historical verdicts live in [the archive](docs/history/python-parity.md).

Before review, export `CARGO_BUILD_JOBS=6 RUST_TEST_THREADS=6` and run:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --release -- -D warnings
QUIDDITY_CORPUS_REQUIRED=1 cargo test --workspace --release
```

## Contributor License Agreement

quiddity-rust is released under the Apache License 2.0. So that the copyright
owner can keep licensing contributions on those and any other terms, every
contributor agrees to the
[Contributor License Agreement](CLA.md). You keep the copyright to your work;
the CLA grants a licence, not an assignment.

On your first pull request a bot asks you to sign by replying:

> I have read the CLA Document and I hereby sign the CLA

Pull requests cannot merge until the CLA is signed.
