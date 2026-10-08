#!/usr/bin/env python3
"""Lightweight per-process ps samples; %CPU uses macOS ps averaging, not per-frame CPU."""
import csv
import subprocess
import sys
import time

pid, destination = sys.argv[1:3]
seconds = int(sys.argv[3]) if len(sys.argv) > 3 else 30
with open(destination, 'w') as output:
    writer = csv.writer(output)
    writer.writerow(['elapsed_s', 'ps_cpu_percent', 'rss_kib'])
    started = time.monotonic()
    for _ in range(seconds):
        result = subprocess.run(['ps', '-p', pid, '-o', '%cpu=', '-o', 'rss='], capture_output=True, text=True, check=True)
        cpu, rss = result.stdout.split()
        writer.writerow([round(time.monotonic() - started, 3), cpu, rss])
        output.flush()
        time.sleep(1)
