from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
FIXTURES = Path(__file__).with_name("fixtures")
CURRENT_ANALYZER = SCRIPTS / "analyze_current_environment_irradiance_capture.py"
COMPATIBILITY_ANALYZER = SCRIPTS / "analyze_environment_irradiance_capture.py"
PRODUCTION_RUNNERS = tuple(sorted(SCRIPTS.glob("check_ddgi*.sh")))
sys.path.insert(0, str(SCRIPTS))

from rfirr_production_runner_contract import (  # noqa: E402
    production_runner_invocation_failures,
)


class RfirrCurrentVersionContractTests(unittest.TestCase):
    def run_analyzer(
        self, analyzer: Path, capture: Path, *arguments: str
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(analyzer), str(capture), *arguments],
            check=False,
            capture_output=True,
            text=True,
        )

    def test_production_runners_have_direct_current_schema_invocations(self) -> None:
        for runner in PRODUCTION_RUNNERS:
            source = runner.read_text(encoding="utf-8")
            with self.subTest(runner=runner.name):
                self.assertEqual(production_runner_invocation_failures(source), [])

    def test_comments_assignments_and_unused_functions_are_not_invocations(self) -> None:
        source = (SCRIPTS / "check_ddgi_correctness.sh").read_text(encoding="utf-8")
        direct_call = (
            '    "$repo_root/scripts/'
            'analyze_current_environment_irradiance_capture.py" "$@"'
        )
        mutations = (
            source.replace(direct_call, f"    # {direct_call.strip()}", 1),
            source.replace(
                direct_call,
                '    unused_analyzer="$repo_root/scripts/'
                'analyze_current_environment_irradiance_capture.py"',
                1,
            ),
            source.replace('analyze_current_capture "', 'true "'),
        )
        for mutated in mutations:
            with self.subTest():
                self.assertNotEqual(production_runner_invocation_failures(mutated), [])

    def test_current_schema_entry_accepts_current_and_rejects_v9(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            current = root / "current.rfirr"
            current.write_bytes(
                bytes.fromhex((FIXTURES / "ddgi_filter_evidence_v10.hex").read_text())
            )
            historical = root / "historical.rfirr"
            historical.write_bytes(
                bytes.fromhex((FIXTURES / "ddgi_filter_evidence_v9.hex").read_text())
            )

            current_result = self.run_analyzer(CURRENT_ANALYZER, current)
            historical_result = self.run_analyzer(CURRENT_ANALYZER, historical)

        self.assertEqual(current_result.returncode, 0, current_result.stderr)
        self.assertEqual(json.loads(current_result.stdout)["capture"]["version"], 10)
        self.assertEqual(historical_result.returncode, 1, historical_result.stderr)
        self.assertIn(
            "version: expected 10, got 9",
            json.loads(historical_result.stdout)["validation_failures"],
        )

    def test_current_schema_entry_has_no_numeric_or_dynamic_version_surface(self) -> None:
        fixture = bytes.fromhex(
            (FIXTURES / "ddgi_filter_evidence_v9.hex").read_text()
        )
        with tempfile.TemporaryDirectory() as directory:
            capture = Path(directory) / "historical.rfirr"
            capture.write_bytes(fixture)
            mutations = (
                ("--expect-version", "9"),
                ("--expect-version=9",),
                # Escaped and dynamically expanded shell spellings produce this argv.
                ("--expect-version", "current"),
            )
            for arguments in mutations:
                with self.subTest(arguments=arguments):
                    result = self.run_analyzer(CURRENT_ANALYZER, capture, *arguments)
                    self.assertEqual(result.returncode, 2)
                    self.assertIn("unrecognized arguments", result.stderr)

    def test_shell_escaping_and_expansion_cannot_select_a_schema(self) -> None:
        fixture = bytes.fromhex(
            (FIXTURES / "ddgi_filter_evidence_v9.hex").read_text()
        )
        with tempfile.TemporaryDirectory() as directory:
            capture = Path(directory) / "historical.rfirr"
            capture.write_bytes(fixture)
            commands = (
                '"$1" "$2" --expect\\-version 9',
                'flag=--expect-version; "$1" "$2" "$flag" 9',
            )
            for command in commands:
                with self.subTest(command=command):
                    result = subprocess.run(
                        [
                            "bash",
                            "-c",
                            command,
                            "production-current-schema-test",
                            str(CURRENT_ANALYZER),
                            str(capture),
                        ],
                        check=False,
                        capture_output=True,
                        text=True,
                    )
                    self.assertEqual(result.returncode, 2)
                    self.assertIn("unrecognized arguments", result.stderr)

    def test_compatibility_entry_retains_explicit_numeric_decode(self) -> None:
        fixture = bytes.fromhex(
            (FIXTURES / "ddgi_filter_evidence_v9.hex").read_text()
        )
        with tempfile.TemporaryDirectory() as directory:
            capture = Path(directory) / "historical.rfirr"
            capture.write_bytes(fixture)
            result = self.run_analyzer(
                COMPATIBILITY_ANALYZER, capture, "--expect-version", "9"
            )

        self.assertEqual(result.returncode, 0, result.stderr)

    def test_python_consumers_do_not_own_a_numeric_current_version(self) -> None:
        for consumer_name in (
            "validate_ddgi_radiance_lifecycle.py",
            "summarize_ddgi_convergence.py",
        ):
            source = (SCRIPTS / consumer_name).read_text(encoding="utf-8")
            with self.subTest(consumer=consumer_name):
                self.assertNotRegex(source, r"\.version\s*(?:==|!=)\s*\d+")


if __name__ == "__main__":
    unittest.main()
