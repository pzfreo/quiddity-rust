#!/usr/bin/env python3
"""Compare recognition CLI CPU time using two already-built release binaries.

Usage: benchmark_threads.py BASELINE_BINARY CURRENT_BINARY CORPUS_DIR [ROUNDS]
Includes STEP import, recognition, fingerprints, document construction and JSON
serialisation; stdout is discarded. Run on an otherwise idle machine. Alternates
binary order and reports medians and individual CPU/wall observations.
"""
import json
import pathlib
import resource
import statistics
import subprocess
import sys
import time

FILES = [
    'cadgenbench/threaded_connector_109.step',
    'cadgenbench_inputs/cgb243.step.gz',
    'cadgenbench_inputs/cgb202.step.gz',
    'cadgenbench_inputs/cgb203.step',
    'cadgenbench_inputs/cgb242.step.gz',
]


def run(binary, path):
    before = resource.getrusage(resource.RUSAGE_CHILDREN)
    start = time.perf_counter()
    subprocess.run([str(binary), str(path)], stdout=subprocess.DEVNULL,
                   stderr=subprocess.PIPE, check=True)
    wall = time.perf_counter() - start
    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    cpu = (after.ru_utime + after.ru_stime) - (before.ru_utime + before.ru_stime)
    return {'cpu_seconds': cpu, 'wall_seconds': wall}


def main():
    binaries = [pathlib.Path(p).resolve() for p in sys.argv[1:3]]
    corpus = pathlib.Path(sys.argv[3])
    rounds = int(sys.argv[4]) if len(sys.argv) > 4 else 3
    for name in FILES:
        samples = [[], []]
        for i in range(rounds):
            for which in ([0, 1] if i % 2 == 0 else [1, 0]):
                samples[which].append(run(binaries[which], corpus / name))
        medians = [statistics.median(s['cpu_seconds'] for s in side) for side in samples]
        print(json.dumps({'file': name, 'baseline_cpu': medians[0],
                          'current_cpu': medians[1],
                          'change_percent': 100*(medians[1]/medians[0]-1),
                          'samples': samples}), flush=True)


if __name__ == '__main__':
    main()
