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


def path_without_node() -> str:
    """The current PATH minus every directory that provides a `node` executable."""
    kept = [
        entry
        for entry in os.environ.get("PATH", "").split(os.pathsep)
        if entry and shutil.which("node", path=entry) is None
    ]
    return os.pathsep.join(kept)


def copy_backend(destination: Path) -> Path:
    """Copy src/backend into an isolated tree so the runner never touches the repo."""
    isolated_backend = destination / "src" / "backend"
    shutil.copytree(
        BACKEND,
        isolated_backend,
        ignore=shutil.ignore_patterns("bin", "target", "results.json", "__pycache__"),
    )
    return isolated_backend


@unittest.skipUnless(shutil.which("node"), "node is not installed")
@unittest.skipUnless(shutil.which("cc"), "cc is not installed")
class JavaScriptEndToEndTests(unittest.TestCase):
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
        cls.generated = cls.temp_path / "measurements.txt"
        subprocess.run(
            [
                sys.executable,
                str(BACKEND / "generate_measurements.py"),
                "--rows",
                "1000",
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
        js = self.run_program(["node", str(BACKEND / "weather_js.mjs")], data)
        self.assertEqual(c.returncode, 0, c.stderr)
        self.assertEqual(js.returncode, 0, js.stderr)
        self.assertEqual(js.stdout, c.stdout)
        if expected is not None:
            self.assertEqual(js.stdout, expected)

    def test_seeded_generated_dataset_matches_c(self):
        self.assertEqual(len(self.generated.read_bytes().splitlines()), 1000)
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
        self.assertNotIn(b"-0.0", expected)

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
                for command in ([str(self.c_binary)], ["node", str(BACKEND / "weather_js.mjs")]):
                    result = self.run_program(command, data)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn(b"malformed input at line 3", result.stderr)

    def test_benchmark_reports_matching_javascript(self):
        isolated_backend = copy_backend(self.temp_path / "benchmark")
        env = os.environ.copy()
        env["CARGO_TARGET_DIR"] = str(self.temp_path / "cargo-target")
        result = subprocess.run(
            [sys.executable, str(isolated_backend / "run_benchmark.py"), "--data", str(self.generated), "--runs", "1"],
            capture_output=True,
            timeout=180,
            env=env,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        payload = json.loads((isolated_backend / "results.json").read_text(encoding="utf-8"))
        self.assertTrue(payload["outputs_match"])
        records = {record["language"]: record for record in payload["results"]}
        self.assertEqual(records["C"]["status"], "ok", records["C"])
        self.assertEqual(records["JavaScript"]["status"], "ok", records["JavaScript"])

    def test_benchmark_skips_javascript_without_node(self):
        isolated_backend = copy_backend(self.temp_path / "benchmark-no-node")
        env = os.environ.copy()
        env["PATH"] = path_without_node()
        env["CARGO_TARGET_DIR"] = str(self.temp_path / "cargo-target")
        self.assertIsNone(shutil.which("node", path=env["PATH"]))
        result = subprocess.run(
            [sys.executable, str(isolated_backend / "run_benchmark.py"), "--data", str(self.generated), "--runs", "1"],
            capture_output=True,
            timeout=180,
            env=env,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn(b"[JavaScript] skipped: node not found on PATH", result.stderr)
        payload = json.loads((isolated_backend / "results.json").read_text(encoding="utf-8"))
        self.assertTrue(payload["outputs_match"])
        records = {record["language"]: record for record in payload["results"]}
        self.assertEqual(records["C"]["status"], "ok", records["C"])
        self.assertEqual(records["JavaScript"]["status"], "skipped")
        self.assertEqual(records["JavaScript"]["reason"], "node not found on PATH")
        page = (isolated_backend.parent / "frontend" / "index.html").read_text(encoding="utf-8")
        self.assertIn("<li>JavaScript: node not found on PATH</li>", page)


if __name__ == "__main__":
    unittest.main()
