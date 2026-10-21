"""Record ratchet improvements from Rust evaluation; never lower a recorded score.

QUIDDITY_MFINSTSEG=/path/to/MFInstSeg python3 tools/update_mfinstseg_baseline.py
The fixed sample, input hashes, mapping and class supports must stay unchanged.
Mapping/dataset changes require a separate evidence-backed baseline review.
"""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
from fractions import Fraction


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def ratio(counts, denominator):
    return Fraction(counts['true_positive'], counts[denominator]) if counts[denominator] else Fraction(0)


def main():
    repo = Path(__file__).resolve().parents[1]
    path = repo / 'tests/fixtures/mfinstseg_baseline.json'
    baseline = json.loads(path.read_text())
    root = Path(os.environ['QUIDDITY_MFINSTSEG'])
    assert digest(repo / 'tests/fixtures/mfinstseg_mapping.json') == baseline['mapping_sha256'], 'mapping changed'
    for model in baseline['models']:
        assert digest(root / 'steps' / (model['id'] + '.step')) == model['step_sha256'], model['id']
        assert digest(root / 'labels' / (model['id'] + '.json')) == model['labels_sha256'], model['id']
    with tempfile.TemporaryDirectory() as tmp:
        ids, out = Path(tmp) / 'ids.txt', Path(tmp) / 'scores.json'
        ids.write_text('\n'.join(m['id'] for m in baseline['models']) + '\n')
        env = dict(os.environ, CARGO_BUILD_JOBS='6', RUST_TEST_THREADS='6')
        subprocess.run(['cargo', 'run', '--release', '--example', 'mfinstseg_score', '--features', 'python-parity', '--', str(root), str(ids), str(out), '6'], cwd=repo, env=env, check=True)
        report = json.loads(out.read_text())
    assert not report['label_errors'] and not report['label_warnings'] and not report['recognition_failures'] and not report['section_recess_refusals']
    scores = [c['counts'] for c in report['classes']]
    assert len(scores) == len(baseline['scores'])
    for i, (got, old) in enumerate(zip(scores, baseline['scores'])):
        for metric in ['faces', 'instances']:
            assert got[metric]['labelled'] == old[metric]['labelled'], (i, metric, 'support changed')
            for denominator in ['predicted', 'labelled']:
                assert ratio(got[metric], denominator) >= ratio(old[metric], denominator), (i, metric, 'regressed')
    assert report['models'] == 200 and len(report['model_accounting']) == 200
    measured = {m['id']: m['accounting'] for m in report['model_accounting']}
    assert len(measured) == 200 and set(measured) == {m['id'] for m in baseline['models']}
    for model in baseline['models']:
        got = measured[model['id']]
        assert got['feature_as_body'] == 0, (model['id'], 'feature hidden as body')
        if 'accounting' in model:
            assert got['stock_unexplained'] <= model['accounting']['stock_unexplained'], (model['id'], 'stock unexplained regressed')
        model['accounting'] = got
    assert report['accounting']['feature_as_body'] == 0
    baseline['schema'] = 2
    baseline['accounting'] = report['accounting']
    baseline['scores'] = scores
    path.write_text(json.dumps(baseline, indent=2) + '\n')
    print('Recorded non-regressing Rust scores; review and commit the baseline diff.')


if __name__ == '__main__':
    main()
