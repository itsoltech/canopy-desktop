#!/usr/bin/env python3
"""Summarize GPUI draw/submission events, separately for each capture and window."""
import csv
import json
import sys
from collections import defaultdict
from pathlib import Path


def percentile(values, q):
    ordered = sorted(values)
    position = (len(ordered) - 1) * q
    lower = int(position)
    upper = min(lower + 1, len(ordered) - 1)
    return ordered[lower] + (ordered[upper] - ordered[lower]) * (position - lower)


def summarize(path):
    groups = defaultdict(list)
    with path.open() as source:
        for row in csv.DictReader(source):
            for metric in ('work_ms', 'dirty_to_draw_ms', 'submission_ms', 'animation_submission_interval_ms'):
                if row[metric]:
                    groups[(row['window'], metric)].append(float(row[metric]))
    return [dict(capture=str(path), window=window, metric=metric, samples=len(values),
                 p50=percentile(values, .5), p95=percentile(values, .95),
                 p99=percentile(values, .99), maximum=max(values),
                 over_8_333_ms_pct=sum(v > 1000 / 120 for v in values) / len(values) * 100)
            for (window, metric), values in groups.items()]


if __name__ == '__main__':
    print(json.dumps([summary for arg in sys.argv[1:] for summary in summarize(Path(arg))], indent=2))
