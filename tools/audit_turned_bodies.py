#!/usr/bin/env python3
"""Compare examples/turned_body_audit JSONL dumps against #123's permitted delta.

Pass BEFORE AFTER REPORT.json. All existing raw and document records are compared
exactly, including defining faces and evidence status. Body face accounting changes
are expected; the sole allowed existing-record removal is a proved OD boss.
"""
import collections
import json
import pathlib
import sys


def load(path):
    return {row['path']: row for row in map(json.loads, open(path))}


def name(path):
    if '/tests/corpus/' in path:
        return 'corpus/' + path.split('/tests/corpus/', 1)[1]
    if '/tests/fixtures/' in path:
        return 'fixture/' + path.split('/tests/fixtures/', 1)[1]
    return 'repro/' + path.rsplit('/', 1)[-1]


def audit(before, after):
    exceptions, bodies, bosses, failures, steps = [], [], [], [], []
    assert before.keys() == after.keys(), 'different input inventories'
    for path in sorted(before):
        a, b = before[path], after[path]
        if 'error' in a or 'error' in b:
            if a != b:
                exceptions.append([name(path), 'read', a.get('error'), b.get('error')])
            failures.append({'part': name(path), 'error': a.get('error')})
            continue
        a, b = a['value'], b['value']
        raw_a, raw_b = a['raw'], b['raw']
        has_steps = bool(raw_a['features']['turned_steps'])
        if has_steps:
            steps.append({'part': name(path), 'turned': bool(raw_b['features'].get('turned_bodies'))})
        for scope, old, new in [('raw', raw_a['features'], raw_b['features']),
                                ('document', a['document'], b['document'])]:
            if scope == 'document':
                if 'Ok' not in old or 'Ok' not in new:
                    if old != new:
                        exceptions.append([name(path), scope, 'document result changed'])
                    continue
                old = collections.defaultdict(list)
                new = collections.defaultdict(list)
                for r in a['document']['Ok']['records']:
                    old[r['family']].append({k: v for k, v in r.items() if k != 'id'})
                for r in b['document']['Ok']['records']:
                    new[r['family']].append({k: v for k, v in r.items() if k != 'id'})
            turned = new.get('turned_bodies', [])
            for i, body in enumerate(turned):
                params = body['parameters'] if scope == 'document' else body
                bodies.append({'part': name(path), 'scope': scope, 'index': i,
                               **{k: params[k] for k in ('od', 'length', 'axis')}})
            for family in sorted(set(old) | set(new)):
                if family == 'turned_bodies':
                    continue
                prev, now = old.get(family, []), new.get(family, [])
                if prev == now:
                    continue
                if family != 'bosses':
                    exceptions.append([name(path), scope, family, 'records changed'])
                    continue
                remaining = list(prev)
                for record in now:
                    if record not in remaining:
                        exceptions.append([name(path), scope, family, 'boss added or changed'])
                        break
                    remaining.remove(record)
                for boss in remaining:
                    p = boss['parameters'] if scope == 'document' else boss
                    owned = boss['faces'] if scope == 'document' else raw_a['defining']['bosses'][prev.index(boss)]
                    evidence = None
                    for body in turned:
                        body = body['parameters'] if scope == 'document' else body
                        cylinder = [s for s in body['profile'] if s['face'] in owned and s['kind'] == 'cylinder']
                        if len(cylinder) != len(owned):
                            continue
                        lo = min(s['start'][0] for s in cylinder)
                        hi = max(s['end'][0] for s in cylinder)
                        # Exact suppression is checked with model precision in Rust; this
                        # audit also checks the published dimensions and raw profile span.
                        if abs(p['diameter'] - body['od']) < 1e-6 and hi-lo >= .9*body['length']-1e-6:
                            evidence = {'diameter': p['diameter'], 'od': body['od'], 'height': p['height'],
                                        'span': [lo, hi], 'length': body['length'], 'faces': owned}
                            break
                    if evidence is None:
                        exceptions.append([name(path), scope, family, 'unproved suppression', p])
                    else:
                        bosses.append({'part': name(path), 'scope': scope, **evidence})
            if scope == 'raw':
                for family in set(raw_a['defining']) | set(raw_b['defining']):
                    if family not in ('turned_bodies', 'bosses') and raw_a['defining'].get(family) != raw_b['defining'].get(family):
                        exceptions.append([name(path), 'raw defining', family])
    count = lambda rows, scope: sum(r['scope'] == scope for r in rows)
    return {'reference': '42649a258f616301f7faefee53af68d3ada3f049',
            'inputs': len(before), 'readable': len(before)-len(failures),
            'new_bodies': {s: count(bodies,s) for s in ['raw','document']},
            'boss_suppressions': {s: count(bosses,s) for s in ['raw','document']},
            'turned_steps_parts': {'total':len(steps), 'turned':sum(s['turned'] for s in steps),
                                   'not_turned':sum(not s['turned'] for s in steps)},
            'exceptions':exceptions, 'bodies':bodies, 'suppressed_bosses':bosses,
            'turned_steps':steps, 'baseline_read_failures':failures}


if __name__ == '__main__':
    report = audit(load(sys.argv[1]), load(sys.argv[2]))
    pathlib.Path(sys.argv[3]).write_text(json.dumps(report, indent=2, sort_keys=True)+'\n')
    print(json.dumps({k:v for k,v in report.items() if k not in ('bodies','suppressed_bosses','turned_steps','baseline_read_failures')},indent=2))
    sys.exit(bool(report['exceptions']))
