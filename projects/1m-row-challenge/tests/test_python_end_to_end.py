import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
BACKEND = ROOT / "src" / "backend"


def copy_backend(destination: Path) -> Path:
    isolated_backend = destination / "src" / "backend"
    shutil.copytree(
        BACKEND,
        isolated_backend,
        ignore=shutil.ignore_patterns("bin", "target", "results.json", "__pycache__"),
    )
    return isolated_backend


@unittest.skipUnless(shutil.which("cc"), "cc is not installed")
class PythonEndToEndTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp_dir = tempfile.TemporaryDirectory()
        cls.addClassCleanup(cls.temp_dir.cleanup)
        cls.temp_path = Path(cls.temp_dir.name)
        cls.c_binary = cls.temp_path / "weather_c"
        subprocess.run(
            ["cc", "-std=c11", "-O2", str(BACKEND / "weather_c.c"), "-o", str(cls.c_binary)],
            check=True,
            capture_output=True,
        )
        cls.generated = cls.temp_path / "data" / "measurements.txt"
        subprocess.run(
            [
                sys.executable,
                str(BACKEND / "generate_measurements.py"),
                "--rows",
                "10000",
                "--seed",
                "42",
                "--out",
                str(cls.generated),
            ],
            check=True,
            capture_output=True,
        )

    def run_program(self, command, data):
        return subprocess.run([*command, str(data)], capture_output=True, timeout=30)

    def assert_matches_c(self, data, expected=None):
        c = self.run_program([str(self.c_binary)], data)
        python = self.run_program([sys.executable, str(BACKEND / "weather_python.py")], data)
        self.assertEqual(c.returncode, 0, c.stderr)
        self.assertEqual(python.returncode, 0, python.stderr)
        self.assertEqual(python.stdout, c.stdout)
        if expected is not None:
            self.assertEqual(python.stdout, expected)

    def run_benchmark(self, name, env, data=None):
        root = self.temp_path / name
        backend = copy_backend(root)
        (root / ".env").write_text(f"DATA_FILE={data or self.generated}\n", encoding="utf-8")
        env["CARGO_TARGET_DIR"] = str(self.temp_path / "cargo-target")
        result = subprocess.run(
            [sys.executable, str(backend / "run_benchmark.py"), "--runs", "1"],
            capture_output=True,
            timeout=180,
            env=env,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        payload = json.loads((backend / "results.json").read_text(encoding="utf-8"))
        page = (root / "src" / "frontend" / "index.html").read_text(encoding="utf-8")
        return result, payload, page

    def assert_row_count(self, result, payload, page, expected):
        self.assertIn(f"   Rows: {expected:,}\n".encode(), result.stdout)
        self.assertEqual(payload["rows"], expected)
        self.assertIn(f" &middot; {expected:,} rows &middot; ", page)

    def test_seeded_generated_dataset_matches_c(self):
        self.assertEqual(len(self.generated.read_bytes().splitlines()), 10000)
        self.assert_matches_c(self.generated)

    def test_edge_cases_match_expected_bytes(self):
        data = self.temp_path / "edge_cases.txt"
        data.write_bytes(
            (
                "𐀀;1.0\n"
                "é;-2.5\n"
                "NegativeTie;-0.1\nNegativeTie;0.0\n"
                "PositiveTie;0.0\nPositiveTie;0.1\n"
                "NegativeMean;-0.1\nNegativeMean;0.0\nNegativeMean;0.0\n"
                "Zero;-0.0\n"
                "\n \t\v\f\r\n"
                "\ue000;2.0\r\n"
                "Z;-3.2"
            ).encode("utf-8")
        )
        expected = (
            "{NegativeMean=-0.1/0.0/0.0, NegativeTie=-0.1/-0.1/0.0, "
            "PositiveTie=0.0/0.1/0.1, Z=-3.2/-3.2/-3.2, "
            "Zero=0.0/0.0/0.0, é=-2.5/-2.5/-2.5, "
            "\ue000=2.0/2.0/2.0, 𐀀=1.0/1.0/1.0}\n"
        ).encode("utf-8")
        self.assert_matches_c(data, expected)

    def test_blank_only_input(self):
        data = self.temp_path / "blank.txt"
        data.write_bytes(b"\n \t\v\f\r\n\r\n")
        self.assert_matches_c(data, b"{}\n")

    def test_malformed_lines_fail(self):
        data = self.temp_path / "malformed.txt"
        for malformed in (
            b"missing separator",
            b";1.0",
            b"Station;1.00",
            b"Station;1.0;2.0",
            b"Station;1",
            b"Station;.5",
            b"Station;99999999999999999999.0",
        ):
            with self.subTest(line=malformed):
                data.write_bytes(b"Valid;1.0\n\n" + malformed + b"\n")
                for command in ([str(self.c_binary)], [sys.executable, str(BACKEND / "weather_python.py")]):
                    result = self.run_program(command, data)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn(b"malformed input at line 3", result.stderr)

    def test_temperature_range_matches_c(self):
        data = self.temp_path / "temperature-range.txt"
        cases = (
            (b"A;00000000000000000001.0\n", 0, b"{A=1.0/1.0/1.0}\n"),
            (b"A;-00000000000000000001.0\n", 0, b"{A=-1.0/-1.0/-1.0}\n"),
            (b"A;99999999999999999999x\n", 1, b""),
            (b"A;-99999999999999999999x\n", 1, b""),
            (
                b"A;922337203685477580.7\n",
                0,
                b"{A=922337203685477580.7/922337203685477580.7/922337203685477580.7}\n",
            ),
            (
                b"A;-922337203685477580.7\n",
                0,
                b"{A=-922337203685477580.7/-922337203685477580.7/-922337203685477580.7}\n",
            ),
            (b"A;922337203685477580.8\n", 1, b""),
            (b"A;-922337203685477580.8\n", 1, b""),
        )
        for line, expected_code, expected_stdout in cases:
            with self.subTest(line=line):
                data.write_bytes(line)
                c = self.run_program([str(self.c_binary)], data)
                python = self.run_program([sys.executable, str(BACKEND / "weather_python.py")], data)
                self.assertEqual(python.returncode, c.returncode)
                self.assertEqual(python.stdout, c.stdout)
                self.assertEqual(c.returncode, expected_code, c.stderr)
                self.assertEqual(c.stdout, expected_stdout)
                expected_error = (
                    b"malformed input at line 1: temperature out of range\n"
                    if expected_code
                    else b""
                )
                self.assertEqual(c.stderr, b"weather_c: " + expected_error if expected_error else b"")
                self.assertEqual(
                    python.stderr, b"weather_python: " + expected_error if expected_error else b""
                )

    def test_benchmark_reports_matching_python_and_page(self):
        result, payload, page = self.run_benchmark("benchmark", os.environ.copy())
        self.assert_row_count(result, payload, page, 10000)
        self.assertTrue(payload["outputs_match"])
        records = {record["language"]: record for record in payload["results"]}
        self.assertEqual(records["C"]["status"], "ok", records["C"])
        self.assertEqual(records["Python"]["status"], "ok", records["Python"])
        self.assertRegex(page, r"<title>[^<]*\bPython\b[^<]*</title>")
        self.assertRegex(page, r'<tr><td class="language">Python</td>.*?</tr>')
        self.assertRegex(
            page,
            r'<div class="bar-row"><span class="label">Python</span>'
            r'<div class="bar"[^>]*></div><span class="value">[0-9.]+s</span></div>',
        )

    def test_benchmark_counts_nonblank_rows(self):
        data = self.temp_path / "rows-with-blanks.txt"
        data.write_bytes(b"\nOslo;1.0\r\n \t\v\f\r\nLima;-2.5\n\r\nOslo;3.0")
        result, payload, page = self.run_benchmark("benchmark-rows", os.environ.copy(), data)
        self.assert_row_count(result, payload, page, 3)

    def test_benchmark_skips_python_without_python3(self):
        env = os.environ.copy()
        paths = [
            entry
            for entry in env.get("PATH", "").split(os.pathsep)
            if entry and shutil.which("python3", path=entry) is None
        ]
        tools = self.temp_path / "tools"
        tools.mkdir()
        (tools / "cc").symlink_to(shutil.which("cc"))
        env["PATH"] = os.pathsep.join([str(tools), *paths])
        self.assertIsNone(shutil.which("python3", path=env["PATH"]))
        result, payload, page = self.run_benchmark("benchmark-no-python", env)
        self.assertIn(b"[Python] skipped: python3 not found on PATH", result.stderr)
        self.assertTrue(payload["outputs_match"])
        records = {record["language"]: record for record in payload["results"]}
        self.assertEqual(records["C"]["status"], "ok", records["C"])
        self.assertEqual(records["Python"]["status"], "skipped")
        self.assertEqual(records["Python"]["reason"], "python3 not found on PATH")
        self.assertIn("<li>Python: python3 not found on PATH</li>", page)


if __name__ == "__main__":
    unittest.main()
