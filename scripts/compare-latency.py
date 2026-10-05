#!/usr/bin/env python3
"""Print cumulative BMZ_LATENCY_JSON samples, keeping separate epochs/streams.

Use a fresh process/stream and equal warm-up/measurement duration for each run.
This does not sum unrelated distributions or infer physical latency.
"""
import json
import sys

def read_samples(source):
    latest = {}
    rejected = 0
    for line in source:
        if "BMZ_LATENCY_JSON " not in line:
            continue
        try:
            sample, _ = json.JSONDecoder().raw_decode(line.split("BMZ_LATENCY_JSON ", 1)[1])
            kind = sample["kind"]
            stream = sample.get("stream") or {}
            key = (kind, stream.get("id"), sample.get("epoch"), sample.get("generation"))
            latest[key] = sample
        except (ValueError, KeyError, TypeError, AttributeError):
            rejected += 1
    return {"segments": list(latest.values()), "rejected_lines": rejected}


def main():
    for path in sys.argv[1:]:
        with open(path, encoding="utf-8") as source:
            result = read_samples(source)
        print(json.dumps({"file": path, **result}, ensure_ascii=False))


if __name__ == "__main__":
    main()
