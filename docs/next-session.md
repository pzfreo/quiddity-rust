# G7a: section recess publication (2026-10-11)

The maintainer assigned publication of `SectionRecess` records to quiddity-rust. The existing
projection already discovers native recesses, projects uncovered prismatic pockets, passages,
edge-open prismatic/circular recesses, rectangular/round-bottom blind slots, and finally proved
pockets/channels, in Python's order. It keeps the first occurrence of each body-local
constituent-face region. Publication now uses that same run and its existing frame mapping.

Overturnable judgement calls:

- Keep `quiddity-rust/recognition/2` and the package version: this is additive, follows the
  step-level/riser precedent, and preserves the requested old document bytes. A strict family
  allowlist reader must explicitly admit `section_recesses`, `SectionRecess` and `SectionRecessRefusal` before moving its tag pin
  (G7b). This version decision preserves the acceptance requirement's existing document bytes;
  the strict reader's family allowlist still makes the addition a coordinated reader change.
- Retain all existing source families beside the new family. The review correctly overturned
  the refusal omission: occurrences and source refusals both belong under `section_recesses`.
  Derived patterns remain in the dedicated section-recess API. The carried projection refusal on
  13975/14052 and the missing local-degradation retry are unchanged.
- Use the committed Python capture at `6d544e060cd0eeb4442d2196e5df757b5a15ea55`, which matches
  the read-only oracle's HEAD. Its 15 NIST channel omissions have rust-correct verdicts backed
  by independent classifier/probe evidence; publishing them would restore Python's known bug.
  The document parity test requires every relevant verdict to remain exercised.

`tests/fixtures/section_recess_publication_baseline.json` lists the exact 30 files from
draftwright-rust's `tools/quality_parts.txt`. Its SHA256s are of compact serde bytes at
quiddity-rust `4583c740f74bf872a0214ef104feb48a6fed01a2`, before changing publication: raw
`Features`, caller-space document, and default framed document. Removing precisely the new
family must recover those bytes. No existing parity fixtures or verdicts are relaxed.

Before (caller-space recognition on the 30 quality parts):

| Metric | Records | Parts |
|---|---:|---:|
| Internal pockets | 88 | 19 |
| Internal channels | 2 | 2 |
| Projected section recesses in memory | 92 | 21 |
| Published section recesses | 0 | 0 |

After (the same source records, published through the existing unified projection):

| Published classification | Records | Parts |
|---|---:|---:|
| Pocket | 40 | 19 |
| Channel | 33 | 13 |
| Passage | 14 | 9 |
| Edge-open recess | 5 | 4 |
| Total section recesses | 92 | 21 |

Python publishes 107 occurrences: 40 pockets, 48 channels, 14 passages and 5 edge-open recesses.
The 15 extra channels are the existing rust-correct NIST verdicts. All 92 remaining occurrences
match, including fields, geometry frames, evidence, dense indices and order. All 30 raw
`Features` snapshots and 60 document snapshots (caller and inferred frames) preserve their
previous bytes after removing only the addition.

The three new tests each failed on unchanged production code (0 passed, 3 failed):
`quality_part_section_recesses_match_python` (missing records),
`inferred_frame_section_recesses_are_published` (missing framed records), and
`publication_preserves_existing_document_bytes` (missing new key). They then passed (3 passed,
0 failed). The existing four-slice `recognition_carries_section_recesses` test now also checks
every published record's fields, defining faces and frame labels on all 100 corpus parts.

Publication exposed 69 previously untested caller-space versus inferred-frame section-recess
comparisons in `tests/recognition.rs`. No tolerances or comparisons were relaxed. The added
`known_framed_document.json` verdicts are 66 equivalent section-basis/order representations and
3 undetermined placement-dependent proofs. An independent reconstruction compared the section
boundary vertices, arc midpoints and low/high end-surface samples of all 222 matched records on
these 69 files in file coordinates: maximum coordinate difference 0.001557470029111883 mm, below
the existing 0.01 mm grid bound; body, classification and both evidence lists agree.

The read-only Python oracle was also run directly on `10000` and `10047` (run/end reversal and
order changes), and on all three unmatched-region parts (`10060`, `10103`, `10119`). It
reproduces all three: on 10060, caller space issues a channel on [11, 13] and the inferred frame
an edge-open obround on [11, 12, 13, 14]; on 10103 the inferred frame issues the edge-open
obround on [24, 25, 26, 27] while caller space refuses source [24, 26]; on 10119 it issues the
obround pocket on [15, 16, 17, 18] while caller space refuses source [15, 18]. The first two
inherit the existing source-family undetermined verdicts; the third is likewise undetermined.
These are pre-existing recognition/projection behaviors, now visible in the document. Which
placement-dependent proof is right remains an overturnable judgement call; this stream does
not change those recognisers.

Final required checks passed on this worktree, with `CARGO_BUILD_JOBS=8`,
`RUST_TEST_THREADS=8` and `QUIDDITY_CORPUS=/Users/paul/repos/quiddity/tests/corpus` (one cargo
command at a time):

- `cargo fmt --all --check`: passed.
- `cargo clippy --workspace --all-targets --release -- -D warnings`: passed.
- `QUIDDITY_CORPUS_REQUIRED=1 cargo test --workspace --release`: passed, including haecceity;
  **481 passed, 0 failed, 3 ignored**, summed over 60 result lines. The ignores are the existing
  development aids (`export_written_files`, `dump`, `face_moment_evidence`). All 31 recognition
  document tests, the 3 new publication tests and all 9 existing section-recess family tests
  passed. `git diff --check` is clean.

Files changed: `src/features/mod.rs`, `src/recognition.rs`,
`src/correspondence/fingerprint.rs`, `tests/recognition.rs`,
`tests/section_recess_publication.rs`, `tests/fixtures/section_recess_publication_baseline.json`,
`tests/fixtures/known_framed_document.json`, `README.md`, `CHANGELOG.md`, `docs/alpha1.md` and
this file. No commits or pushes. No Python or draftwright repository changes.

## G7a fix round (2026-10-11)

`Features::section_recess_refusals` now retains the same projection's source refusals.
`recognition::families` appends them after occurrences under `section_recesses`, and the
document publishes their Python class and defining faces. Refusals carry no geometry and
receive no correspondence fingerprint or local geometry labels; their body and face indices
are unchanged by the inferred-frame mapping. The whole-projection error on 13975/14052 is
distinct and still carried provisionally, as before. The quality-part parity test now compares
all 12 refusals on 5 parts against Python's capture, in order, as well as all occurrences.

The earlier independent reconstruction is now enforced by
`tests/support/recess_samples.rs` in the caller-space document tests for every equivalent
section-recess verdict. It compares body, classification, both evidence lists and symmetric
file-space boundary/end samples (vertices and bulged-edge midpoints), allowing only the
existing 0.01 mm serialization bound. This checks equivalent regions despite section basis,
run direction and record-order changes. The three undetermined proof verdicts remain open.

Overturnable judgement call: retain schema v2 for this fix, preserving all previous document
bytes apart from the new family as required. G7b must admit both classes before draftwright
updates its pin; a future incompatible schema revision should be coordinated there.

Refusal publication exposed three verdict-count changes: 10103 and 10119 now include their
caller-only source refusal for the already documented placement-dependent proof (+1 each).
On holdout 163, the same refused constituent region [10, 11, 12, 13, 32] has caller defining
faces [11, 12] and framed defining faces [10, 11, 12, 13] (+2 unmatched records). A direct
read-only Python run at 6d544e0 reproduces both evidence associations and the same refusal
reason. Overturnable judgement call: retain its equivalent verdict because the refused region
and warning are identical; the committed check pins both alternative source associations.
No comparison tolerance changed; bounds increased only by these newly published records.

Fix-round final validation: `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets --release -- -D warnings`, and
`QUIDDITY_CORPUS_REQUIRED=1 cargo test --workspace --release` all passed with
`CARGO_BUILD_JOBS=8`, `RUST_TEST_THREADS=8` and the corpus path above, one Cargo command at a
time. Workspace totals: **481 passed, 0 failed, 3 existing ignored tests**, across 60 result
lines, including haecceity. All 31 recognition tests and all 3 publication tests passed.
`git diff --check` is clean. No commits or pushes; the pre-existing staged changes were left
untouched. No Python or draftwright repository files changed.
