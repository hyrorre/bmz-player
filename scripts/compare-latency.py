#!/usr/bin/env python3
"""Print the last cumulative BMZ_LATENCY_JSON sample of each kind per log.

Use a fresh process/stream and equal warm-up/measurement duration for each run.
This does not sum unrelated distributions or infer physical latency.
"""
import json
import sys

for path in sys.argv[1:]:
    latest = {}
    with open(path, encoding="utf-8") as source:
        for line in source:
            if "BMZ_LATENCY_JSON " not in line:
                continue
            payload = line.split("BMZ_LATENCY_JSON ", 1)[1]
            try:
                sample, _ = json.JSONDecoder().raw_decode(payload)
            except ValueError:
                continue
            latest[sample["kind"]] = sample
    print(json.dumps({"file": path, "samples": latest}, ensure_ascii=False))
