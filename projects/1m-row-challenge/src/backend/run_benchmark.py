#!/usr/bin/env python3
"""THE benchmark runner: build, time, verify, then write results.json + HTML.

Usage:
    python3 src/backend/run_benchmark.py [--runs N] [--data /abs/path/file.txt]

With no --data, the dataset path is read from DATA_FILE in the project-root .env.
"""

from __future__ import annotations

import argparse
import html
import json
import os
import shutil
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

# run_benchmark.py lives in <root>/src/backend; <root> is the project root.
BACKEND_DIR = Path(__file__).resolve().parent
PROJECT_ROOT = BACKEND_DIR.parent.parent
BIN_DIR = BACKEND_DIR / "bin"

LANGUAGES = ("C", "C++", "Rust", "Go", "JavaScript", "Python")
DATA_BLANK_BYTES = b" \t\r\n\v\f"


class BuildError(Exception):
    """A toolchain is missing or a build failed; that language is skipped."""


def describe_error(stderr: str | None) -> str:
    """A compact, single-line reason from compiler output."""
    if not stderr:
        return "unknown error"
    lines = [line.strip() for line in stderr.splitlines() if line.strip()]
    return lines[-1][:300] if lines else "unknown error"


def require_tool(tool: str) -> str:
    found = shutil.which(tool)
    if found is None:
        raise BuildError(f"{tool} not found on PATH")
    return found


def build_command(args: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(args, cwd=BACKEND_DIR, capture_output=True, encoding="utf-8")


def read_data_file_from_env() -> str | None:
    """Return DATA_FILE from <root>/.env, or None if absent/empty/unreadable."""
    env_path = PROJECT_ROOT / ".env"
    try:
        contents = env_path.read_text(encoding="utf-8")
    except FileNotFoundError:
        return None
    except OSError as error:
        print(f"error: cannot read {env_path}: {error}", file=sys.stderr)
        return None

    value: str | None = None
    for raw_line in contents.splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, candidate = line.split("=", 1)
        if key.strip() != "DATA_FILE":
            continue
        candidate = candidate.strip()
        if len(candidate) >= 2 and candidate[0] == candidate[-1] and candidate[0] in ("'", '"'):
            candidate = candidate[1:-1].strip()
        if candidate:
            value = candidate
    return value


def count_data_rows(path: str) -> int:
    """Count non-blank measurement lines, using the implementations' blank rules."""
    count = 0
    with open(path, "rb") as handle:
        for line in handle:
            if line.strip(DATA_BLANK_BYTES):
                count += 1
    return count


def build_c() -> list[str]:
    compiler = require_tool("cc")
    binary = BIN_DIR / "weather_c"
    result = build_command(
        [compiler, "-O3", "-march=native", "-std=c11", "-o", str(binary), str(BACKEND_DIR / "weather_c.c")]
    )
    if result.returncode != 0:
        raise BuildError(f"cc build failed: {describe_error(result.stderr)}")
    return [str(binary)]


def build_cpp() -> list[str]:
    compiler = require_tool("g++")
    binary = BIN_DIR / "weather_cpp"
    result = build_command(
        [compiler, "-O3", "-march=native", "-std=c++17", "-o", str(binary), str(BACKEND_DIR / "weather_cpp.cpp")]
    )
    if result.returncode != 0:
        raise BuildError(f"g++ build failed: {describe_error(result.stderr)}")
    return [str(binary)]


def build_rust() -> list[str]:
    cargo = require_tool("cargo")
    result = build_command([cargo, "build", "--release", "--manifest-path", str(BACKEND_DIR / "Cargo.toml")])
    if result.returncode != 0:
        raise BuildError(f"cargo build failed: {describe_error(result.stderr)}")

    configured_target = os.environ.get("CARGO_TARGET_DIR")
    if configured_target:
        target_dir = Path(configured_target)
        if not target_dir.is_absolute():
            target_dir = BACKEND_DIR / target_dir
    else:
        target_dir = BACKEND_DIR / "target"
    source = target_dir / "release" / "weather_rust"
    if not source.is_file():
        raise BuildError(f"cargo build produced no binary at {source}")

    binary = BIN_DIR / "weather_rust"
    shutil.copy2(source, binary)
    os.chmod(binary, 0o755)
    return [str(binary)]


def build_go() -> list[str]:
    compiler = require_tool("go")
    binary = BIN_DIR / "weather_go"
    result = build_command(
        [compiler, "build", "-trimpath", "-ldflags=-s -w", "-o", str(binary), str(BACKEND_DIR / "weather_go.go")]
    )
    if result.returncode != 0:
        raise BuildError(f"go build failed: {describe_error(result.stderr)}")
    return [str(binary)]


def build_javascript() -> list[str]:
    """No build step: the command is just `node weather_js.mjs`."""
    node = require_tool("node")
    return [node, str(BACKEND_DIR / "weather_js.mjs")]


def build_python() -> list[str]:
    """No build step: the command is just `python3 weather_python.py`."""
    python = require_tool("python3")
    return [python, str(BACKEND_DIR / "weather_python.py")]


# Each builder returns the command prefix that runs its program; the runner appends
# the dataset path. A BuildError (missing toolchain or failed build) skips the language.
BUILDERS = {
    "C": build_c,
    "C++": build_cpp,
    "Rust": build_rust,
    "Go": build_go,
    "JavaScript": build_javascript,
    "Python": build_python,
}


def format_time(seconds: float) -> str:
    return html.escape(f"{seconds:.3f}")


def render_html(payload: dict, outputs_match: bool) -> str:
    """Render a standalone, framework-free results page."""
    esc = html.escape
    results = payload["results"]
    runs = payload["runs"]
    rows_text = esc(f"{payload['rows']:,}")

    meta = (
        f'Dataset: <code>{esc(payload["dataset"])}</code> &middot; '
        f'{rows_text} rows &middot; '
        f'{esc(str(runs))} runs per language &middot; Generated: {esc(payload["generated_at"])}'
    )

    if outputs_match:
        ok_count = sum(1 for record in results if record["status"] == "ok")
        status = f'<p class="status ok">All {ok_count} outputs matched byte-for-byte</p>'
    else:
        status = (
            '<p class="status fail">OUTPUT MISMATCH: the programs did NOT produce identical '
            "results. Check results.json and the captured stderr output.</p>"
        )

    headers = ["Language", *(f"Run {index + 1}" for index in range(runs)), "Best", "Average"]
    header_html = "".join(f"<th>{esc(header)}</th>" for header in headers)

    table_rows: list[str] = []
    for record in results:
        language = esc(record["language"])
        reason = esc(record.get("reason", ""))
        title = f' title="{reason}"' if reason else ""
        cells = [f'<td class="language"{title}>{language}</td>']
        if record["status"] == "ok":
            cells.extend(f"<td>{format_time(value)}</td>" for value in record["runs"])
            cells.append(f'<td class="best">{format_time(record["best"])}</td>')
            cells.append(f'<td>{format_time(record["average"])}</td>')
        else:
            cells.extend(['<td class="skipped">skipped</td>'] * (runs + 2))
        table_rows.append("<tr>" + "".join(cells) + "</tr>")

    best_values = [
        record["best"] for record in results if record["status"] == "ok" and record["best"] is not None
    ]
    fastest = min(best_values) if best_values else None

    bar_rows: list[str] = []
    for record in results:
        label = esc(record["language"])
        if record["status"] == "ok" and fastest:
            width = max(2, round(fastest / record["best"] * 100))
            width_text = esc(str(width))
            value = f"{format_time(record['best'])}s"
        else:
            width_text = "2"
            value = "skipped"
        bar_rows.append(
            f'<div class="bar-row"><span class="label">{label}</span>'
            f'<div class="bar" style="width:{width_text}%"></div>'
            f'<span class="value">{value}</span></div>'
        )

    skipped = [record for record in results if record["status"] != "ok"]
    notes = ""
    if skipped:
        items = "".join(
            f"<li>{esc(record['language'])}: {esc(record.get('reason', 'skipped'))}</li>" for record in skipped
        )
        notes = f'<p class="notes">Skipped languages:</p><ul class="notes">{items}</ul>'

    return f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>1M Row Challenge — {esc(' vs '.join(LANGUAGES))}</title>
<style>
  :root {{ color-scheme: light; }}
  body {{ margin: 0; padding: 2rem 1rem; background: #f5f7fa; color: #1c2430;
         font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif; }}
  main {{ max-width: 960px; margin: 0 auto; }}
  h1 {{ font-size: 2rem; margin: 0 0 0.5rem; }}
  h2 {{ font-size: 1.25rem; margin: 2rem 0 1rem; }}
  .meta {{ color: #5b6675; font-size: 0.9rem; margin: 0 0 1.25rem; }}
  .meta code {{ background: #e8ecf1; padding: 0.1rem 0.3rem; border-radius: 3px; }}
  .status {{ padding: 0.75rem 1rem; border-radius: 6px; font-weight: 600; margin: 0 0 1.5rem; }}
  .status.ok {{ background: #e3f6e8; color: #17602c; }}
  .status.fail {{ background: #fde7e7; color: #8b1d1d; }}
  table {{ width: 100%; border-collapse: collapse; background: #fff; box-shadow: 0 1px 3px rgba(0,0,0,.08); }}
  th, td {{ padding: 0.55rem 0.7rem; text-align: right; border-bottom: 1px solid #e4e8ee; }}
  th:first-child, td:first-child {{ text-align: left; }}
  th {{ background: #eef2f7; font-size: 0.85rem; text-transform: uppercase; letter-spacing: .03em; }}
  td.best {{ font-weight: 700; color: #17602c; }}
  td.skipped {{ color: #8a94a3; font-style: italic; }}
  .bar-row {{ display: grid; grid-template-columns: 6.5rem 1fr 5rem; align-items: center;
              gap: 0.75rem; margin: 0.5rem 0; }}
  .label {{ font-weight: 600; }}
  .bar {{ background: linear-gradient(90deg, #3b82f6, #22c55e); height: 1.25rem;
          border-radius: 4px; min-width: 2%; }}
  .value {{ text-align: right; font-variant-numeric: tabular-nums; }}
  .notes {{ color: #5b6675; font-size: 0.85rem; margin: 1.5rem 0 0; }}
  ul.notes {{ color: #5b6675; font-size: 0.85rem; }}
</style>
</head>
<body>
<main>
<h1>1 Million Row Challenge</h1>
<p class="meta">{meta}</p>
{status}
<table>
<thead><tr>{header_html}</tr></thead>
<tbody>
{chr(10).join(table_rows)}
</tbody>
</table>
<h2>Best time comparison</h2>
<div class="bars">
{chr(10).join(bar_rows)}
</div>
{notes}
</main>
</body>
</html>
"""


def print_summary(payload: dict) -> None:
    print(f"Dataset: {payload['dataset']}   Runs: {payload['runs']}   Rows: {payload['rows']:,}")
    print()
    header = f"{'Language':<10} {'Best':>8} {'Average':>8}  {'Runs (s)':<34} Status"
    print(header)
    print("-" * len(header))
    for record in payload["results"]:
        language = record["language"]
        if record["status"] == "ok":
            run_values = ", ".join(f"{value:.3f}" for value in record["runs"])
            print(
                f"{language:<10} {record['best']:>8.3f} {record['average']:>8.3f}  {run_values:<34} ok"
            )
        else:
            print(
                f"{language:<10} {'n/a':>8} {'n/a':>8}  {'skipped':<34} "
                f"skipped: {record.get('reason', '')}"
            )
    print()


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Build, benchmark and cross-check the "
            f"{', '.join(LANGUAGES[:-1])} and {LANGUAGES[-1]} implementations."
        )
    )
    parser.add_argument("--runs", type=int, default=5, metavar="N", help="runs per language (default: 5)")
    parser.add_argument("--data", default=None, metavar="PATH", help="dataset path (overrides .env)")
    args = parser.parse_args(argv)

    if args.runs < 1:
        parser.error("--runs must be at least 1")

    data_path = args.data if args.data is not None else read_data_file_from_env()
    if data_path is None:
        print(
            "error: no dataset configured. Pass --data PATH or set DATA_FILE=<absolute path> "
            f"in {PROJECT_ROOT / '.env'}",
            file=sys.stderr,
        )
        return 2
    if not os.path.isfile(data_path):
        print(f"error: dataset file does not exist: {data_path}", file=sys.stderr)
        return 2
    try:
        rows = count_data_rows(data_path)
    except OSError as error:
        print(f"error: cannot read {data_path}: {error}", file=sys.stderr)
        return 2

    BIN_DIR.mkdir(parents=True, exist_ok=True)

    results: list[dict] = []
    outputs: dict[str, str] = {}

    for language in LANGUAGES:
        record: dict = {"language": language, "runs": [], "best": None, "average": None, "status": "skipped"}
        try:
            command = [*BUILDERS[language](), data_path]
        except (BuildError, OSError) as error:
            record["reason"] = str(error)
            results.append(record)
            print(f"[{language}] skipped: {record['reason']}", file=sys.stderr)
            continue


        timings: list[float] = []
        stdout: str | None = None
        failure: str | None = None
        for run_index in range(args.runs):
            start = time.perf_counter()
            try:
                process = subprocess.run(command, capture_output=True, encoding="utf-8")
            except OSError as error:
                failure = f"failed to launch: {error}"
                break
            elapsed = time.perf_counter() - start
            if process.returncode != 0:
                failure = (
                    f"run {run_index + 1} exited with code {process.returncode}: "
                    f"{describe_error(process.stderr)}"
                )
                break
            timings.append(round(elapsed, 3))
            if stdout is None:
                stdout = process.stdout

        if failure is not None:
            record["reason"] = failure
            results.append(record)
            print(f"[{language}] skipped: {failure}", file=sys.stderr)
            continue

        record["runs"] = timings
        record["best"] = min(timings)
        record["average"] = round(sum(timings) / len(timings), 3)
        record["status"] = "ok"
        outputs[language] = stdout if stdout is not None else ""
        results.append(record)

    ok_languages = [record["language"] for record in results if record["status"] == "ok"]
    reference = "C" if "C" in ok_languages else (ok_languages[0] if ok_languages else None)
    outputs_match = False
    if reference is not None:
        baseline = outputs[reference]
        outputs_match = all(outputs[language] == baseline for language in ok_languages)

    payload = {
        "dataset": data_path,
        "rows": rows,
        "runs": args.runs,
        "generated_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "outputs_match": outputs_match,
        "results": results,
    }

    results_path = BACKEND_DIR / "results.json"
    results_path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")

    frontend_dir = BACKEND_DIR.parent / "frontend"
    frontend_dir.mkdir(parents=True, exist_ok=True)
    html_path = frontend_dir / "index.html"
    html_path.write_text(render_html(payload, outputs_match), encoding="utf-8")

    print_summary(payload)
    if outputs_match:
        print(f"PASS: {len(ok_languages)} output(s) matched byte-for-byte (reference: {reference}).")
    else:
        print(f"FAIL: outputs differ (reference: {reference}).")
    print(f"Wrote {results_path}")
    print(f"Wrote {html_path}")
    return 0 if outputs_match else 1


if __name__ == "__main__":
    sys.exit(main())
