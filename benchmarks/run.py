#!/usr/bin/env python3
"""Run the same work through fastlane and through shlane, and time it.

Both tools read an equivalent lane definition — fastlane/Fastfile and
shlane.yaml — so each scenario is doing the same thing on both sides.

Usage:
    GEM_HOME=... PATH=...  ./run.py [--runs N] [--shlane PATH]
"""

import argparse
import json
import os
import resource
import shutil
import statistics
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))

# Everything that would otherwise reach the network or write into the repo,
# turned off. All of these make fastlane faster, not slower.
FASTLANE_ENV = {
    "FASTLANE_SKIP_UPDATE_CHECK": "1",
    "FASTLANE_OPT_OUT_USAGE": "1",
    "FASTLANE_DISABLE_COLORS": "1",
    "FASTLANE_SKIP_ACTION_SUMMARY": "1",
    "LANG": "C.UTF-8",
    "LC_ALL": "C.UTF-8",
}

SCENARIOS = [
    # (name, what it measures, fastlane argv, shlane argv)
    ("version", "process start-up, nothing else",
     ["fastlane", "--version"], ["--version"]),
    ("list", "parse the lane definitions and list them",
     ["fastlane", "lanes"], ["list"]),
    ("noop", "a lane with one step that does nothing",
     ["fastlane", "android", "noop"], ["run", "noop"]),
    ("many", "a lane with 20 shell steps",
     ["fastlane", "android", "many"], ["run", "many"]),
    ("interpolate", "a parameter, an env var, and a value passed between steps",
     ["fastlane", "android", "interpolate", "target:prod"],
     ["run", "interpolate", "target=prod"]),
]


def measure(argv, env):
    """Run argv once. Returns (wall seconds, peak RSS in MB)."""
    start = time.perf_counter()
    proc = subprocess.Popen(
        argv, env=env, cwd=HERE,
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    _, status, usage = os.wait4(proc.pid, 0)
    wall = time.perf_counter() - start
    if status != 0:
        raise SystemExit(f"{argv[0]} failed ({status}): {' '.join(argv)}")
    return wall, usage.ru_maxrss / 1024.0


def bench(argv, env, runs):
    measure(argv, env)  # one warm-up, not recorded
    times, rss = [], []
    for _ in range(runs):
        w, r = measure(argv, env)
        times.append(w)
        rss.append(r)
    return {
        "min": min(times),
        "median": statistics.median(times),
        "mean": statistics.fmean(times),
        "max": max(times),
        "rss_mb": max(rss),
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--runs", type=int, default=10)
    ap.add_argument("--shlane", default=os.path.join(HERE, "..", "target", "release", "shlane"))
    ap.add_argument("--json", metavar="PATH", help="also write the raw numbers here")
    args = ap.parse_args()

    shlane = os.path.abspath(args.shlane)
    if not os.path.exists(shlane):
        raise SystemExit(f"no shlane binary at {shlane} — cargo build --release")
    if not shutil.which("fastlane"):
        raise SystemExit("fastlane is not on PATH")

    env = dict(os.environ, BENCH_ENV="hello", **FASTLANE_ENV)

    results = {}
    for name, what, fl_argv, sl_argv in SCENARIOS:
        print(f"  {name} ...", end="", flush=True)
        fl = bench(fl_argv, env, args.runs)
        sl = bench([shlane] + sl_argv, env, args.runs)
        results[name] = {"what": what, "fastlane": fl, "shlane": sl}
        print(f" fastlane {fl['median']:.3f}s   shlane {sl['median']:.3f}s"
              f"   {fl['median'] / sl['median']:.0f}x")

    print()
    print(f"{'scenario':<14} {'fastlane':>10} {'shlane':>10} {'faster by':>11}"
          f" {'fl RSS':>9} {'sl RSS':>9}")
    print("-" * 68)
    for name, r in results.items():
        f, s = r["fastlane"], r["shlane"]
        print(f"{name:<14} {f['median']:>9.3f}s {s['median']:>9.3f}s"
              f" {f['median'] / s['median']:>10.0f}x"
              f" {f['rss_mb']:>8.0f}M {s['rss_mb']:>8.0f}M")

    if args.json:
        with open(args.json, "w") as fh:
            json.dump({"runs": args.runs, "results": results}, fh, indent=2)
        print(f"\nraw numbers: {args.json}")


if __name__ == "__main__":
    main()
