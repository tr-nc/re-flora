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
    RUNNER_INVOCATION_INVENTORY,
    RUNNER_PRODUCTION_DEPENDENCIES,
    production_evidence_dependencies,
    production_runner_invocation_failures,
)
from shader_validation_workflow_contract import REQUIRED_OWNER_PATHS  # noqa: E402


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
                self.assertEqual(
                    production_runner_invocation_failures(runner.name, source), []
                )

        self.assertEqual(
            set(RUNNER_INVOCATION_INVENTORY),
            {runner.name for runner in PRODUCTION_RUNNERS},
        )

    def test_comments_assignments_and_unused_functions_are_not_invocations(self) -> None:
        runner_name = "check_ddgi_correctness.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
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
                self.assertNotEqual(
                    production_runner_invocation_failures(runner_name, mutated), []
                )

    def test_every_inventory_rejects_deleted_or_replaced_analysis_branches(self) -> None:
        for runner in PRODUCTION_RUNNERS:
            source = runner.read_text(encoding="utf-8")
            invocation = next(
                line
                for line in source.splitlines()
                if "analyze_current_capture" in line
                and "()" not in line
                and "analyze_current_environment" not in line
            )
            for replacement in (f"# {invocation}", invocation.replace("analyze_current_capture", "true", 1)):
                with self.subTest(runner=runner.name, replacement=replacement.strip()):
                    mutated = source.replace(invocation, replacement, 1)
                    self.assertNotEqual(
                        production_runner_invocation_failures(runner.name, mutated), []
                    )

    def test_unused_helper_call_cannot_replace_a_real_inventory_branch(self) -> None:
        runner_name = "check_ddgi_runtime_terrain_edits.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        real_call = "        analyze_current_capture"
        mutated = source.replace(real_call, "        true", 1)
        mutated += "\nunused_analysis() {\n    analyze_current_capture \"$@\"\n}\n"

        self.assertNotEqual(
            production_runner_invocation_failures(runner_name, mutated), []
        )

    def test_alternate_or_late_function_overrides_are_rejected(self) -> None:
        runner_name = "check_ddgi_transport_acceptance.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        mutations = (
            source + "\nanalyze_current_capture(){ true; }\n",
            source + "\nanalyze_current_capture () { true; }\n",
            source + "\nfunction analyze_current_capture { true; }\n",
            source + "\nfunction analyze_current_capture() { true; }\n",
            source + "\nalias analyze_current_capture=true\n",
            source + "\nanalyze_current_capture=true\n",
        )
        for mutated in mutations:
            with self.subTest():
                self.assertNotEqual(
                    production_runner_invocation_failures(runner_name, mutated), []
                )

    def test_canonical_function_accepts_equivalent_bash_definition_forms(self) -> None:
        runner_name = "check_ddgi_transport_acceptance.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        for header in (
            "analyze_current_capture(){",
            "analyze_current_capture () {",
            "function analyze_current_capture {",
            "function analyze_current_capture() {",
        ):
            with self.subTest(header=header):
                mutated = source.replace("analyze_current_capture() {", header, 1)
                mutated = mutated.replace(
                    '    "$repo_root/scripts/analyze_current_environment_irradiance_capture.py" "$@"',
                    '  "$repo_root/scripts/analyze_current_environment_irradiance_capture.py"   "$@"',
                    1,
                )
                self.assertEqual(
                    production_runner_invocation_failures(runner_name, mutated), []
                )

    def test_dependency_inventory_accepts_braced_repo_root_literals(self) -> None:
        runner_name = "check_ddgi_transport_acceptance.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        mutated = source.replace("$repo_root/scripts/", "${repo_root}/scripts/")
        self.assertEqual(
            production_runner_invocation_failures(runner_name, mutated), []
        )

    def test_scope_inventory_accepts_equivalent_helper_definition_forms(self) -> None:
        runner_name = "check_ddgi_transport_acceptance.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        for function_name in ("execute_analysis", "run_analysis"):
            canonical = f"{function_name}() {{"
            for header in (
                f"{function_name}(){{",
                f"{function_name} () {{",
                f"function {function_name} {{",
                f"function {function_name}() {{",
            ):
                with self.subTest(function=function_name, header=header):
                    mutated = source.replace(canonical, header, 1)
                    self.assertEqual(
                        production_runner_invocation_failures(
                            runner_name, mutated
                        ),
                        [],
                    )

    def test_indented_function_closing_brace_ends_scope(self) -> None:
        runner_name = "check_ddgi_inflight_terrain_edits.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        mutated = source.replace(
            '    echo "[DDGI_INFLIGHT_EDIT] PASS spacing=$spacing repeat=$repeat active revision skipped $obsolete_revision and reached $replacement_revision"\n'
            "    fi\n"
            "}\n\n"
            'for spacing in "${spacings[@]}"; do',
            '    echo "[DDGI_INFLIGHT_EDIT] PASS spacing=$spacing repeat=$repeat active revision skipped $obsolete_revision and reached $replacement_revision"\n'
            "    fi\n"
            "    }\n\n"
            'for spacing in "${spacings[@]}"; do',
            1,
        )
        self.assertEqual(
            production_runner_invocation_failures(runner_name, mutated), []
        )

    def test_runner_dependency_inventory_closes_over_all_production_helpers(self) -> None:
        sources = {
            runner.name: runner.read_text(encoding="utf-8")
            for runner in PRODUCTION_RUNNERS
        }
        self.assertEqual(set(RUNNER_PRODUCTION_DEPENDENCIES), set(sources))
        dependencies = production_evidence_dependencies(sources)
        self.assertIn("scripts/check_ddgi_sky_normalization_evidence.py", dependencies)
        self.assertTrue(dependencies.issubset(REQUIRED_OWNER_PATHS))

        for runner_name, source in sources.items():
            with self.subTest(runner=runner_name):
                self.assertEqual(
                    production_runner_invocation_failures(runner_name, source), []
                )

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
