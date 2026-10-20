# Contributing to quiddity-rust

Contributions are welcome: bug reports, fixes, features and documentation.

## Rust-first

Rust recognition is judged against independent references: OpenCascade, analytic
fixtures, MFInstSeg/MFCAD++ labels, and motion invariance. Committed Python
captures are a frozen regression record, not the target for new behaviour. Keep
all parity harnesses flagging unlisted differences; record deliberate departures
supported by independent evidence as `rust-correct`, citing that evidence. Do not
add new Python captures. Do not remove output to improve metrics unless the
information still reaches the output, or weaken tests or known-difference entries
to get green. Remove stale entries only when the harness reports they no longer
match, and report their removal.

## Contributor License Agreement

quiddity-rust is released under the AGPL-3.0 and is also offered under separate
commercial terms by the copyright owner. So that contributions can be included
in both, every contributor agrees to the
[Contributor License Agreement](CLA.md). You keep the copyright to your work;
the CLA grants a licence, not an assignment.

On your first pull request a bot asks you to sign by replying:

> I have read the CLA Document and I hereby sign the CLA

Pull requests cannot merge until the CLA is signed.
