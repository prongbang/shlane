# shlane against fastlane

A benchmark of the thing both tools actually do on every CI job: start up, read a
lane definition, and run some steps.

## What is being compared

[`fastlane/Fastfile`](fastlane/Fastfile) and [`shlane.yaml`](shlane.yaml) define the
same three lanes, step for step. [`run.py`](run.py) runs each scenario through both
tools, discards a warm-up run, and reports the median of ten.

| Scenario | What it measures |
|---|---|
| `version` | process start-up, and nothing else |
| `list` | reading the lane definitions and listing them |
| `noop` | a lane with one step that does nothing |
| `many` | a lane with 20 shell steps |
| `interpolate` | a parameter, an environment variable, and a value passed between steps |

Everything that would make fastlane slower for reasons unrelated to the comparison is
turned off: `FASTLANE_SKIP_UPDATE_CHECK`, `FASTLANE_OPT_OUT_USAGE`,
`FASTLANE_DISABLE_COLORS`, `FASTLANE_SKIP_ACTION_SUMMARY`, and `skip_docs` in the
Fastfile. All of those favour fastlane.

These are CPU-bound scenarios on purpose. Neither tool is doing the work that dominates
a real release — `xcodebuild` takes minutes, and it takes the same minutes either way.
What is measured here is the overhead each tool adds on top of that.

## Results

Ubuntu 24.04, Xeon @ 2.80GHz, 4 cores, 15 GB. fastlane 2.240.1 on Ruby 3.3.6;
shlane 0.1.0 built with rustc 1.94.1 in release mode. Median of 10 runs.

A second run of the whole set put fastlane 5–7% slower and shlane unchanged, so the
ratios below are good to about one significant figure, not two.

### Running a lane

| Scenario | fastlane | shlane | Faster by |
|---|---|---|---|
| `version` | 1.270 s | 0.0020 s | 634× |
| `list` | 1.274 s | 0.0022 s | 582× |
| `noop` | 1.278 s | 0.0044 s | 294× |
| `many` (20 steps) | 1.477 s | 0.0392 s | 38× |
| `interpolate` | 1.294 s | 0.0085 s | 153× |

Run through `bundle exec`, as most CI configurations do, fastlane costs about another
0.15 s: `bundle exec fastlane android noop` takes 1.440 s.

### Where the time goes

Splitting the `noop` and `many` numbers into a fixed cost and a per-step cost:

| | Fixed, per invocation | Per step |
|---|---|---|
| fastlane | ~1.28 s | ~10 ms |
| shlane | ~0.004 s | ~1.8 ms |

The ratio is largest on short lanes because nearly all of fastlane's cost is paying for
the Ruby VM and loading its gems. A lane with a hundred steps would narrow the gap, but
the fixed second is paid by every job, every time.

### Memory

Peak RSS, from `wait4` rusage:

| | Peak RSS |
|---|---|
| fastlane | 78–79 MB |
| shlane | 12 MB |

### Setting it up on CI

The number that matters most on a fresh runner, because it is paid before any build
starts:

| | Time | On disk |
|---|---|---|
| `bundle install` (cold, no cache) | 48.8 s | 112 MB, 81 gems |
| `gem install fastlane` (cold) | 37.1 s | 79 MB |
| shlane: checksum and extract the tarball | 0.064 s | 6.0 MB binary, 2.6 MB tarball |

The shlane figure leaves out the download itself, which depends on the runner's
bandwidth — 2.6 MB is well under a second on any CI network, and `install.sh` caches
nothing because there is nothing worth caching.

## Reproducing it

```bash
cargo build --release

export GEM_HOME=/tmp/bench-gem GEM_PATH=/tmp/bench-gem
export PATH=/tmp/bench-gem/bin:$PATH
gem install fastlane --no-document

cd benchmarks && ./run.py --runs 10
```

`run.py --json out.json` writes the raw numbers, including min and max, rather than
only the medians above.

## What this does not show

- **Nothing here touches Xcode or Gradle.** On a real iOS release the build dominates,
  and the difference between the two tools is a second or two out of ten minutes.
- **fastlane has ~400 actions; shlane has 25.** Speed is not the reason to choose
  between them if the action you need only exists on one side.
- **Linux only.** The iOS actions could not be measured here at all — see the caveats in
  [`docs/plan/15-roadmap.md`](../docs/plan/15-roadmap.md).
