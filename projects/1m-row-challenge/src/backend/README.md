# 1-million-row challenge: C vs C++ vs Rust vs Go vs JavaScript vs Python

This directory contains six deliberately simple, comparable implementations of the
classic [One Billion Row Challenge](https://github.com/gunnarmorling/1brc) task,
scaled down to **1,000,000 rows**, plus a generator and a single benchmark runner.
All six programs must print **byte-for-byte identical** output so the results can
be cross-checked.

The **project root** used throughout this file is `projects/1m-row-challenge` in the
h2h-playground repository; run every command from that folder.

```
src/backend/
  README.md                 this file
  generate_measurements.py  optional dependency-free dataset generator
  weather_c.c               C implementation
  weather_cpp.cpp           C++ implementation
  weather_rust.rs           Rust implementation
  weather_go.go             Go implementation
  weather_js.mjs            JavaScript implementation (Node.js, no build step)
  weather_python.py         Python implementation (standard library, no build step)
  Cargo.toml                Rust release build configuration
  run_benchmark.py          THE runner (builds, times, verifies, writes the HTML)
  results.json              generated, gitignored
  bin/                      generated binaries, gitignored
```

## 1. Dataset

### Source and why

We use the data format and station catalogue from the original, Apache-2.0 licensed
**One Billion Row Challenge** repository: <https://github.com/gunnarmorling/1brc>
(the station means come from Wikipedia's *List of cities by average temperature*,
as credited in
[`CreateMeasurements.java`](https://github.com/gunnarmorling/1brc/blob/main/src/main/java/dev/morling/onebrc/CreateMeasurements.java)).

Why this source:

* It is the canonical, open-source definition of this exact benchmark: one
  `station;temperature` measurement per line, temperature with exactly one decimal.
* 413 real station names including UTF-8 names (`Abéché`, `İzmir`, `Wrocław`), so
  parsing/sorting is non-trivial but still realistic.
* The expected aggregation (min / mean / max, sorted by station) has a well-known
  reference output format we can compare across languages.
* At 1M rows (~16 MB) a run takes well under a second on modern hardware, so
  several repetitions are cheap.

### Option A — generate with the upstream Java generator (canonical data)

Needs only a JDK 17+ (`javac` and `java`); **no Maven needed** because the generator
class has no external dependencies. From the project root:

```sh
git clone --depth 1 https://github.com/gunnarmorling/1brc.git /tmp/1brc
mkdir -p /tmp/1brc/build
javac -d /tmp/1brc/build /tmp/1brc/src/main/java/dev/morling/onebrc/CreateMeasurements.java
(cd /tmp/1brc && java -cp build dev.morling.onebrc.CreateMeasurements 1000000)
mkdir -p data
mv /tmp/1brc/measurements.txt "$PWD/data/measurements.txt"
```

> `CreateMeasurements` writes `./measurements.txt` into its current directory; that
> is why the `java` command is wrapped in a subshell `cd`. If `javac` works but
> `java` prints "Unable to locate a Java Runtime", your JDK has no JRE (this is the
> case on the machine where this repo was built) — use Option B instead.

Check the result:

```sh
wc -l data/measurements.txt     # 1000000
head -3 data/measurements.txt   # e.g. Zürich;9.3
```

### Option B — no JVM: use the bundled Python generator

This repository's generator uses the same canonical station catalogue and the same
one-decimal format, with a fixed seed option for reproducibility. From the project
root:

```sh
python3 src/backend/generate_measurements.py --rows 1000000 --out data/measurements.txt
```

(`--seed 42` makes the file reproducible; `--help` lists all options.)

### Where the file goes / how the path is configured

Keep the data anywhere you like, but the recommended location is
`<project-root>/data/measurements.txt`. The runner **never** hard-codes it: it reads
`DATA_FILE` from a `.env` file in the project root.

```sh
cp .env.example .env
printf 'DATA_FILE=%s\n' "$PWD/data/measurements.txt" > .env   # absolute path, no quotes needed
cat .env
```

Example `.env`:

```dotenv
DATA_FILE=/Users/you/projects/h2h-playground/projects/1m-row-challenge/data/measurements.txt
```

Both `.env` and `data/` are gitignored — neither the private config nor the ~16 MB
dataset is committed.

## 2. Toolchains

| Language | Required commands | Tested here |
| --- | --- | --- |
| Python 3 (runner, generator + implementation) | `python3` | 3.x |
| C | `cc` (or `gcc`/`clang`) | Apple clang 15 |
| C++ | `g++` (or `clang++`) | Apple clang 15, C++17 |
| Rust | `cargo` and `rustc` | 1.76.0 |
| Go | `go` | 1.27.4 |
| JavaScript | `node` | v24.22.0 |

Builds use optimizations: `-O3 -march=native` for C/C++, `--release` for Rust, and
Go's default optimizer (`-trimpath -ldflags=-s -w`). JavaScript and Python have no
build step: the runner just needs `node` / `python3` on PATH and invokes
`weather_js.mjs` / `weather_python.py` directly. Missing toolchains (including `node`
and `python3`) are reported by the runner and the remaining languages are still
benchmarked, with the HTML showing which ones were skipped.

## 3. Run the benchmark — one command

From the project root:

```sh
python3 src/backend/run_benchmark.py
```

Optional knobs: `--runs 5` (default 5), `--data /abs/path/other.txt` (overrides
`.env`). The runner:

1. reads `DATA_FILE` from the root `.env` and checks the file exists;
2. builds the four compiled programs into `src/backend/bin/` and locates the
   JavaScript implementation (`weather_js.mjs`) and its `node` runtime plus the
   Python implementation (`weather_python.py`) and its `python3` runtime — no build
   step for JavaScript or Python;
3. runs each program `--runs` times, timing only the process execution;
4. compares every captured stdout byte-for-byte against the C output;
5. writes `src/backend/results.json` and regenerates
   `../frontend/index.html` (a static, framework-free results page with a table of
   per-run / best / average times and a simple bar comparison);
6. prints the same table to stdout and exits non-zero if the outputs differ.

Open `src/frontend/index.html` in a browser — no server required.

## 4. Shared specification (this is what makes results comparable)

Every implementation must:

* take exactly one argument: the absolute path to the measurements file (the runner
  passes the `DATA_FILE` value; programs do **not** parse `.env` themselves);
* read every line `station;temperature`, where temperature has one fractional digit
  and may be negative (e.g. `-23.0`);
* accumulate per station: minimum, maximum, and sum in **integer tenths of a
  degree** (`-23.0` → `-230`) using 64-bit integers;
* compute the mean as `round_half_away_from_zero(sum / count)` in tenths, i.e. ties
  go away from zero; format with exactly one fractional digit and normalise
  negative zero to `0.0`;
* print one line to stdout: `{Abha=-23.0/18.0/59.2, Abidjan=..., ...}`, stations
  separated by `, `, sorted by station name in UTF-8 byte order; trailing newline;
* ignore blank lines and fail loudly on malformed lines.

Using integer tenths instead of floating point for the mean makes all six outputs
identical regardless of libc / language formatting differences. Results are
therefore both a performance comparison and a correctness cross-check.
