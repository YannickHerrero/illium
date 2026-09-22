#!/usr/bin/env python3
"""Summarize opt-in daemon timings. These are NOT key-to-pixel measurements."""
import argparse
import json
import math
import re

STAGES = {
    "wake": ("event-loop wake delivered", "wake_latency_us"),
    "drain": ("event drain", "elapsed_us"),
    "workspace": ("workspace placement and focus submitted", "elapsed_us"),
    "applet": ("applet placement submitted", "elapsed_us"),
}


def summarize(lines):
    samples = {stage: [] for stage in STAGES}
    for line in lines:
        for stage, (message, field) in STAGES.items():
            if message not in line:
                continue
            match = re.search(r"\b" + field + r"=(\d+)\b", line)
            if match:
                samples[stage].append(int(match.group(1)) / 1000)
    result = {}
    for stage, values in samples.items():
        if not values:
            continue
        values.sort()
        percentile = lambda p: round(values[max(0, math.ceil(p * len(values)) - 1)], 3)
        result[stage] = dict(count=len(values), p50_ms=percentile(.5),
                             p95_ms=percentile(.95), max_ms=round(values[-1], 3))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("log", nargs="?")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        assert summarize([]) == {}
        result = summarize([f"workspace placement and focus submitted elapsed_us={n * 1000}"
                            for n in range(1, 101)] + ["unrelated elapsed_us=999999"])
        assert result == {"workspace": dict(count=100, p50_ms=50, p95_ms=95, max_ms=100)}
        assert summarize(["event-loop wake delivered wake_latency_us=250"])["wake"]["p95_ms"] == .25
        print("Latency summary checks passed")
        return
    if not args.log:
        parser.error("provide a log from a session started with --trace-latency")
    with open(args.log, encoding="utf-8", errors="replace") as log:
        print(json.dumps(summarize(log), indent=2))


if __name__ == "__main__":
    main()
