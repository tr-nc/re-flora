from __future__ import annotations

import os
import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
RUNNERS = tuple(sorted(SCRIPTS.glob("check_ddgi*.sh")))
EXPECTED_ANALYSES = {
    "check_ddgi_correctness.sh": 44,
    "check_ddgi_inflight_terrain_edits.sh": 6,
    "check_ddgi_lifecycle_acceptance.sh": 3,
    "check_ddgi_local_terrain_convergence.sh": 1,
    "check_ddgi_runtime_terrain_edits.sh": 11,
    "check_ddgi_terrain_edit_cycle.sh": 4,
    "check_ddgi_transport_acceptance.sh": 78,
}
EXPECTED_ARGUMENTS = {
    "check_ddgi_correctness.sh": (
        "--expect-debug-view final",
        "--expect-debug-view moment-visibility",
        "--expect-debug-view exact-visibility",
        "--expect-debug-view exact-irradiance",
        "--expect-debug-view unoccluded-irradiance",
        "--expect-debug-view equal-weight-irradiance",
        "--expect-debug-view raw-cage-irradiance",
    ),
    "check_ddgi_inflight_terrain_edits.sh": (
        "--min-luminance-p99 0.10",
        "--compare-direct-light",
    ),
    "check_ddgi_lifecycle_acceptance.sh": (
        "radiance-changes-spacing-32.rfirr",
        "radiance-changes-spacing-16.rfirr",
        "density-changes.rfirr",
    ),
    "check_ddgi_local_terrain_convergence.sh": (
        "closed-spacing32.rfirr --max-luminance 0.00005",
    ),
    "check_ddgi_runtime_terrain_edits.sh": (
        "--require-filter-history-retain-blend",
        "inflight-stale-active-spacing32-final-a.rfirr",
        "inflight-stale-active-spacing16-final-a.rfirr",
        "flora-consumer-spacing32-final.rfirr",
    ),
    "check_ddgi_terrain_edit_cycle.sh": (
        "closed.rfirr --max-luminance 0.00005",
        "reopened.rfirr --min-luminance-p99 0.10",
    ),
    "check_ddgi_transport_acceptance.sh": (
        "target/ddgi-transport-acceptance",
        "--require-filter-history-retain-blend",
        "target/ddgi-correctness",
        "target/ddgi-runtime-terrain-edits",
        "target/ddgi-lifecycle-acceptance",
    ),
}


def _analysis_lines(result: subprocess.CompletedProcess[str]) -> list[str]:
    return [
        line
        for line in (result.stdout + "\n" + result.stderr).splitlines()
        if line.startswith("analyze_current_capture ")
    ]


def _replace_nth(source: str, old: str, new: str, occurrence: int) -> str:
    offset = -1
    for _ in range(occurrence):
        offset = source.find(old, offset + 1)
        if offset == -1:
            raise AssertionError(f"missing mutation anchor {old!r}")
    return source[:offset] + new + source[offset + len(old) :]


def _if_false_mutation(runner: str, source: str) -> str:
    if runner == "check_ddgi_correctness.sh":
        return source.replace(
            '            print_command "${final_analysis[@]}"',
            '            if false; then print_command "${final_analysis[@]}"; fi',
            1,
        )
    if runner == "check_ddgi_inflight_terrain_edits.sh":
        old = '''    if ! analyze_current_capture \\
        "$capture" --min-luminance-p99 0.10; then
        echo "[DDGI_INFLIGHT_EDIT] FAIL spacing=$spacing repeat=$repeat final reopened portal is not lit" >&2
        return 1
    fi'''
        return source.replace(old, f"    if false; then\n{old}\n    fi", 1)
    if runner == "check_ddgi_lifecycle_acceptance.sh":
        old = '''    analyze_current_capture "$capture" \\
        --expect-spacing-voxels "$spacing_voxels" \\
        --expect-geometry-revision "$geometry_revision"'''
        return source.replace(old, "    if false; then\n" + old, 1).replace(
            '        --require-nonnegative-rgb >"$analysis_output" || return 1',
            '        --require-nonnegative-rgb >"$analysis_output" || return 1\n    fi',
            1,
        )
    if runner == "check_ddgi_local_terrain_convergence.sh":
        old = '''if [[ "$dry_run" == true || -f "$capture" ]] && ! analyze_current_capture \\
    "$capture" --max-luminance 0.00005 >"$analysis_output"; then
    fail "closed scene retained stale light"
fi'''
        return source.replace(old, f"if false; then\n{old}\nfi", 1)
    if runner == "check_ddgi_runtime_terrain_edits.sh":
        return source.replace(
            '        printf \'%q \' "${analysis[@]}"',
            '        if false; then printf \'%q \' "${analysis[@]}"; fi',
            1,
        )
    if runner == "check_ddgi_terrain_edit_cycle.sh":
        old = '''    if [[ "$mode" == "closed" ]]; then
        if ! analyze_current_capture'''
        return source.replace(old, "    if false; then\n" + old, 1).replace(
            '    echo "[DDGI_TERRAIN_EDIT] PASS spacing=$spacing mode=$mode exact revisions ready before capture"',
            '    fi\n    echo "[DDGI_TERRAIN_EDIT] PASS spacing=$spacing mode=$mode exact revisions ready before capture"',
            1,
        )
    if runner == "check_ddgi_transport_acceptance.sh":
        old = '        print_command "${command[@]}"'
        return _replace_nth(
            source, old, '        if false; then print_command "${command[@]}"; fi', 2
        )
    raise AssertionError(f"missing mutation for {runner}")


class DdgiRunnerDryRunContractTests(unittest.TestCase):
    def run_runner(self, runner: Path) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [str(runner), "--dry-run"],
            check=False,
            capture_output=True,
            text=True,
            timeout=20,
        )

    def test_every_runner_emits_its_current_analysis_branch_inventory(self) -> None:
        self.assertEqual(set(EXPECTED_ANALYSES), {runner.name for runner in RUNNERS})
        for runner in RUNNERS:
            with self.subTest(runner=runner.name):
                result = self.run_runner(runner)
                self.assertEqual(result.returncode, 0, result.stderr)
                lines = _analysis_lines(result)
                self.assertEqual(len(lines), EXPECTED_ANALYSES[runner.name])
                joined = "\n".join(lines)
                for expected in EXPECTED_ARGUMENTS[runner.name]:
                    self.assertIn(expected, joined)

    def test_if_false_around_real_analysis_execution_is_observable(self) -> None:
        for runner in RUNNERS:
            with self.subTest(runner=runner.name), tempfile.TemporaryDirectory() as root:
                scripts = Path(root) / "scripts"
                scripts.mkdir()
                for production_runner in RUNNERS:
                    source = production_runner.read_text(encoding="utf-8")
                    if production_runner.name == runner.name:
                        source = _if_false_mutation(production_runner.name, source)
                    target = scripts / production_runner.name
                    target.write_text(source, encoding="utf-8")
                    os.chmod(target, 0o755)
                result = self.run_runner(scripts / runner.name)
                self.assertNotEqual(
                    len(_analysis_lines(result)), EXPECTED_ANALYSES[runner.name]
                )


if __name__ == "__main__":
    unittest.main()
