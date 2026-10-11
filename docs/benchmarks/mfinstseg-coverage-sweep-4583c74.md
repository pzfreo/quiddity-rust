# MFInstSeg full-corpus coverage sweep at 4583c74

**This is not an effectiveness baseline.** As with Python quiddity's sweep at `f1b02ab`
(`docs/benchmarks/mfinstseg-full-coverage-sweep-f1b02ab.md` in quiddity), it scores no defining
faces and computes no precision or recall. It asks three things of every model: does it read,
does recognition complete, and does it emit anything. It then compares the counts with Python's.

Canonical evidence: [`mfinstseg-coverage-sweep-4583c74.json`](mfinstseg-coverage-sweep-4583c74.json).

## Provenance

- Package: quiddity-rust, commit `4583c74` (0.1.0-alpha.1), before the kink-budget fix
- Corpus: MFInstSeg, all 62,495 models, lexical order, from the published `data2.zip` (`steps/`
  and `labels/` only). The 9,373 test-split models match the STEP+label hashes in quiddity's
  `effectiveness-mfinstseg-9373-f2cfe4f.json`.
- Entry point: `features::recognise`, through `examples/coverage_sweep.rs` (4 threads), aggregated
  by `tools/coverage_sweep_report.py`
- Records are counted over Python's census keys at `f1b02ab` (`quiddity.census.CENSUS_BINDINGS`,
  section recesses included), so totals compare like for like.
- Environment: Linux x86_64, 4 cores. Median 0.076 s/model, measured under contention.

## Summary

| | Python `f1b02ab` | Rust `4583c74` |
| --- | ---: | ---: |
| models | 62,495 | 62,495 |
| recognition completed | 62,476 | 62,485 |
| failed | 19 | 10 |
| emitted no record at all | 149 | 133 |
| total records | 337,138 | 345,321 |
| median records per labelled instance | 0.727 | 0.875 |

Python's sweep script was not committed, so the records-per-instance figures may not be defined
identically. Here an instance is a non-empty `inst` equivalence class, as in quiddity's
`_instance_components`.

## The 10 failures

| n | kind | detail |
| ---: | --- | --- |
| 7 | unreadable | the file has a `PARABOLA`, which step-io does not model, so the curve and the faces it bounds are dropped ([elgar328/step-io#1](https://github.com/elgar328/step-io/issues/1)) |
| 1 | panicked | `20221123_142528_11595`: section boundary must be canonical and origin-centred. Python fails on the same model with the same message. |
| 1 | panicked | `20221124_154714_5499`: rich passage cannot reproduce its historical legacy value. Python fails on the same model with the same message. |
| 1 | hung | `20221123_142528_16681`: killed after more than 14 minutes. This was `face_mass`'s kink budget, fixed separately. |

Python's other 17 failures complete in Rust: 11 adjacent-section-edge errors, 5 OCCT
`TopoDS::Face` downcasts and 1 legacy projection refusal.

**298 models (0.48%) carry a section-recess refusal** ("lost body authority"). Rust keeps the
other families and emits no section recesses for those models. This is the local-degradation
retry the README lists as not ported. Before this sweep it was known on two corpus parts.

## Families

| family | Python | Rust | delta |
| --- | ---: | ---: | ---: |
| `section_recess` | 139,522 | 143,044 | +3,522 |
| `hole` | 60,343 | 61,540 | +1,197 |
| `plate` | 26,475 | 26,458 | −17 |
| `boss` | 19,709 | 19,712 | +3 |
| `angled_step` | 15,584 | 15,588 | +4 |
| `circular_blind_step` | 15,074 | 14,981 | −93 |
| `through_step` | 8,703 | 12,222 | +3,519 |
| `chamfer` | 12,062 | 12,074 | +12 |
| `blend` | 11,884 | 11,847 | −37 |
| `paired_ramp_step` | 11,126 | 11,109 | −17 |
| `fillet` | 10,773 | 10,866 | +93 |
| `slot` | 4,030 | 4,032 | +2 |
| `oriented_slot` | 1,117 | 1,114 | −3 |
| `countersink` | 563 | 563 | 0 |
| `flat` | 170 | 170 | 0 |
| `step` | 3 | 0 | −3 |
| `hole_pattern` | 0 | 1 | +1 |
| `groove` | 0 | 0 | 0 |

The port follows a later Python than `f1b02ab`, so these differences mix Python's own changes
since then with port behaviour. Separating the two needs a Python run at the port's reference
commit. `through_step` (+40%) is the difference most worth explaining.

Families outside the `f1b02ab` census, not in the totals: oblique through steps 4,133, oriented
chamfers 2,575, pads 1,347, freeform surfaces 832, interior voids 52, thin-wall bodies 6.

Per-class silence is in the JSON. No class moves by more than 14 models.

## Limitations

- As for Python's sweep: no defining-face attribution, synthetic corpus, and labels unreliable at
  feature intersections.
- The runner cannot interrupt a model, so a hang stalls the run. The hung model was found and
  killed by hand.
- 155 models contain a `HYPERBOLA`, which haecceity replaces with a chord, marking the solid
  invalid. Those models count as completed here.
- MFInstSeg is not vendored. Only derived counts are recorded.
