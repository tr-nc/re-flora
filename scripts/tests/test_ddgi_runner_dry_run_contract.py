from __future__ import annotations

import os
import stat
import subprocess
import tempfile
import tomllib
import unittest
from pathlib import Path

import sys


SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

from rfirr_production_runner_contract import (  # noqa: E402
    production_runner_invocation_failures,
)
RUNNERS = tuple(sorted(SCRIPTS.glob("check_ddgi*.sh")))
APP_BINARY = tomllib.loads((SCRIPTS.parent / "Cargo.toml").read_text())["package"][
    "name"
]
GUARDED_LAUNCH_COMMANDS = ("cargo", APP_BINARY)
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
            '        if ! analyze_current_capture "${final_analysis[@]}"; then',
            '        if false && ! analyze_current_capture "${final_analysis[@]}"; then',
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
        old = '''if ! analyze_current_capture \\
    "$capture" --max-luminance 0.00005 >"$analysis_output"; then
    fail "closed scene retained stale light"
fi'''
        return source.replace(old, f"if false; then\n{old}\nfi", 1)
    if runner == "check_ddgi_runtime_terrain_edits.sh":
        return source.replace(
            '    if ! analyze_current_capture "${analysis[@]}"; then',
            '    if false && ! analyze_current_capture "${analysis[@]}"; then',
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
        return source.replace(
            '    if ! execute_analysis "$json" "${arguments[@]}"; then',
            '    if false && ! execute_analysis "$json" "${arguments[@]}"; then',
            1,
        )
    raise AssertionError(f"missing mutation for {runner}")


def _dry_run_only_analysis_mutation(runner: str, source: str) -> str:
    if runner == "check_ddgi_correctness.sh":
        return source.replace(
            '        if ! analyze_current_capture "${final_analysis[@]}"; then',
            '        if $dry_run; then\n'
            '            analyze_current_capture "${final_analysis[@]}"\n'
            '        elif false; then',
            1,
        )
    if runner == "check_ddgi_inflight_terrain_edits.sh":
        old = '''    if ! analyze_current_capture \\
        "$capture" --min-luminance-p99 0.10; then'''
        new = '''    if $dry_run; then
        analyze_current_capture \\
            "$capture" --min-luminance-p99 0.10
    elif false; then'''
        return source.replace(old, new, 1)
    if runner == "check_ddgi_lifecycle_acceptance.sh":
        old = '''    analyze_current_capture "$capture" \\
        --expect-spacing-voxels "$spacing_voxels" \\
        --expect-geometry-revision "$geometry_revision"'''
        return source.replace(old, "    if $dry_run; then\n" + old, 1).replace(
            '        --require-nonnegative-rgb >"$analysis_output" || return 1',
            '        --require-nonnegative-rgb >"$analysis_output" || return 1\n'
            '    elif false; then\n'
            '        true\n'
            '    fi',
            1,
        )
    if runner == "check_ddgi_local_terrain_convergence.sh":
        old = '''if ! analyze_current_capture \\
    "$capture" --max-luminance 0.00005 >"$analysis_output"; then'''
        new = '''if $dry_run; then
    analyze_current_capture \\
        "$capture" --max-luminance 0.00005 >"$analysis_output"
elif false; then'''
        return source.replace(old, new, 1)
    if runner == "check_ddgi_runtime_terrain_edits.sh":
        return source.replace(
            '    if ! analyze_current_capture "${analysis[@]}"; then',
            '    if $dry_run; then\n'
            '        analyze_current_capture "${analysis[@]}"\n'
            '    elif false; then',
            1,
        )
    if runner == "check_ddgi_terrain_edit_cycle.sh":
        old = '''        if ! analyze_current_capture \\
            "$capture" --max-luminance 0.00005; then'''
        new = '''        if $dry_run; then
            analyze_current_capture \\
                "$capture" --max-luminance 0.00005
        elif false; then'''
        return source.replace(old, new, 1)
    if runner == "check_ddgi_transport_acceptance.sh":
        old = '''    if ! execute_analysis "$json" "${arguments[@]}"; then
        echo "[DDGI_TRANSPORT] FAIL analysis label=$label" >&2
        return 1
    fi'''
        return source.replace(
            old,
            "    if $dry_run; then\n" + old + "\n    elif false; then\n"
            "        true\n    fi",
            1,
        )
    raise AssertionError(f"missing dry-run-only mutation for {runner}")


def _multiline_dry_run_only_analysis_mutation(runner: str, source: str) -> str:
    mutated = _dry_run_only_analysis_mutation(runner, source)
    mutated = mutated.replace("if $dry_run; then", "if $dry_run\nthen")
    mutated = mutated.replace("elif false; then", "elif false\nthen")
    return mutated


def _parameter_expansion_dry_run_only_analysis_mutation(
    runner: str, source: str
) -> str:
    mutated = _dry_run_only_analysis_mutation(runner, source)
    false_branch = mutated.index("elif false; then")
    dry_header = mutated.rfind("if $dry_run; then", 0, false_branch)
    if dry_header == -1:
        raise AssertionError(f"missing inserted dry-run header for {runner}")
    return (
        mutated[:dry_header]
        + "if ${dry_run:-false}; then"
        + mutated[dry_header + len("if $dry_run; then") :]
    )


def _function_source(source: str, function_name: str) -> str:
    lines = source.splitlines()
    start = next(
        index
        for index, line in enumerate(lines)
        if line.strip() == f"{function_name}() {{"
    )
    end = next(
        index
        for index in range(start + 1, len(lines))
        if lines[index].strip() == "}"
    )
    return "\n".join(lines[start : end + 1])


def _tree_manifest(root: Path) -> dict[Path, tuple[object, ...]]:
    manifest: dict[Path, tuple[object, ...]] = {}

    def visit(path: Path) -> None:
        metadata = path.lstat()
        relative = path.relative_to(root) if path != root else Path(".")
        mode = stat.S_IMODE(metadata.st_mode)
        if stat.S_ISLNK(metadata.st_mode):
            manifest[relative] = ("symlink", mode, os.readlink(path))
        elif stat.S_ISDIR(metadata.st_mode):
            manifest[relative] = ("directory", mode)
            for child in sorted(path.iterdir(), key=lambda item: item.name):
                visit(child)
        elif stat.S_ISREG(metadata.st_mode):
            manifest[relative] = ("file", mode, path.read_bytes())
        else:
            manifest[relative] = ("other", metadata.st_mode, metadata.st_rdev)

    visit(root)
    return manifest


def _assert_no_guarded_command(marker: Path) -> None:
    if marker.exists():
        raise AssertionError(
            "command-called: " + marker.read_text(encoding="utf-8").strip()
        )


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
                    (scripts / runner.name).read_text(encoding="utf-8"),
                    runner.read_text(encoding="utf-8"),
                )
                self.assertNotEqual(
                    len(_analysis_lines(result)), EXPECTED_ANALYSES[runner.name]
                )

    def test_dry_run_only_analysis_branches_are_rejected_statically(self) -> None:
        for runner in RUNNERS:
            with self.subTest(runner=runner.name), tempfile.TemporaryDirectory() as root:
                scripts = Path(root) / "scripts"
                scripts.mkdir()
                for production_runner in RUNNERS:
                    source = production_runner.read_text(encoding="utf-8")
                    if production_runner.name == runner.name:
                        source = _dry_run_only_analysis_mutation(
                            production_runner.name, source
                        )
                        failures = production_runner_invocation_failures(
                            production_runner.name, source
                        )
                        self.assertTrue(
                            any(
                                "controlled by dry_run" in failure
                                for failure in failures
                            ),
                            failures,
                        )
                    target = scripts / production_runner.name
                    target.write_text(source, encoding="utf-8")
                    os.chmod(target, 0o755)
                result = self.run_runner(scripts / runner.name)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(
                    len(_analysis_lines(result)), EXPECTED_ANALYSES[runner.name]
                )

    def test_multiline_dry_run_only_analysis_branches_are_rejected(self) -> None:
        for runner in RUNNERS:
            with self.subTest(runner=runner.name), tempfile.TemporaryDirectory() as root:
                scripts = Path(root) / "scripts"
                scripts.mkdir()
                for production_runner in RUNNERS:
                    source = production_runner.read_text(encoding="utf-8")
                    if production_runner.name == runner.name:
                        source = _multiline_dry_run_only_analysis_mutation(
                            production_runner.name, source
                        )
                        failures = production_runner_invocation_failures(
                            production_runner.name, source
                        )
                        self.assertTrue(
                            any("controlled by dry_run" in failure for failure in failures),
                            failures,
                        )
                    target = scripts / production_runner.name
                    target.write_text(source, encoding="utf-8")
                    os.chmod(target, 0o755)
                result = self.run_runner(scripts / runner.name)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(
                    len(_analysis_lines(result)), EXPECTED_ANALYSES[runner.name]
                )

    def test_parameter_expansion_dry_run_branches_are_rejected(self) -> None:
        for runner in RUNNERS:
            with self.subTest(runner=runner.name):
                source = runner.read_text(encoding="utf-8")
                mutated = _parameter_expansion_dry_run_only_analysis_mutation(
                    runner.name, source
                )
                failures = production_runner_invocation_failures(
                    runner.name, mutated
                )
                self.assertTrue(
                    any("controlled by dry_run" in failure for failure in failures),
                    failures,
                )

    def test_single_line_dry_run_short_circuit_is_rejected(self) -> None:
        runner_name = "check_ddgi_correctness.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        mutated = source.replace(
            '        if ! analyze_current_capture "${final_analysis[@]}"; then',
            '        if $dry_run && ! analyze_current_capture "${final_analysis[@]}"; then',
            1,
        )
        failures = production_runner_invocation_failures(runner_name, mutated)
        self.assertTrue(
            any("controlled by dry_run" in failure for failure in failures),
            failures,
        )

    def test_backslash_continued_dry_run_header_is_rejected(self) -> None:
        runner_name = "check_ddgi_correctness.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        call = 'analyze_current_capture "${final_analysis[@]}"'
        mutated = source.replace(
            f"        if ! {call}; then",
            "        if $dry_run \\\n"
            "            && true; then\n"
            f"            {call}\n"
            "        elif false; then",
            1,
        )
        failures = production_runner_invocation_failures(runner_name, mutated)
        self.assertTrue(
            any("controlled by dry_run" in failure for failure in failures),
            failures,
        )

    def test_dry_run_controlled_elif_and_else_are_rejected(self) -> None:
        runner_name = "check_ddgi_correctness.sh"
        source = (SCRIPTS / runner_name).read_text(encoding="utf-8")
        call = 'analyze_current_capture "${final_analysis[@]}"'
        mutations = (
            source.replace(
                f"        if ! {call}; then",
                "        if false; then\n"
                "            true\n"
                "        elif $dry_run; then\n"
                f"            {call}\n"
                "        elif false; then",
                1,
            ),
            source.replace(
                f"        if ! {call}; then",
                "        if ! $dry_run; then\n"
                "            true\n"
                "        else\n"
                f"            {call}\n"
                "        fi\n"
                "        if false; then",
                1,
            ),
        )
        for mutated in mutations:
            with self.subTest():
                failures = production_runner_invocation_failures(
                    runner_name, mutated
                )
                self.assertTrue(
                    any(
                        "controlled by dry_run" in failure
                        for failure in failures
                    ),
                    failures,
                )

    def test_dry_run_creates_no_capture_tree_or_files(self) -> None:
        for runner in RUNNERS:
            with self.subTest(runner=runner.name), tempfile.TemporaryDirectory() as root:
                scripts = Path(root) / "scripts"
                scripts.mkdir()
                for production_runner in RUNNERS:
                    target = scripts / production_runner.name
                    target.write_bytes(production_runner.read_bytes())
                    os.chmod(target, 0o755)
                (Path(root) / "correctness-runner").symlink_to(
                    "scripts/check_ddgi_correctness.sh"
                )

                before = _tree_manifest(Path(root))
                result = self.run_runner(scripts / runner.name)
                after = _tree_manifest(Path(root))

                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(after, before)

    def test_empty_directory_side_effect_is_observable(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            scripts = Path(root) / "scripts"
            scripts.mkdir()
            for production_runner in RUNNERS:
                source = production_runner.read_text(encoding="utf-8")
                if production_runner.name == "check_ddgi_correctness.sh":
                    source = source.replace(
                        'cd "$repo_root"',
                        'cd "$repo_root"\nmkdir "$repo_root/scratch"',
                        1,
                    )
                target = scripts / production_runner.name
                target.write_text(source, encoding="utf-8")
                os.chmod(target, 0o755)

            before = _tree_manifest(Path(root))
            result = self.run_runner(scripts / "check_ddgi_correctness.sh")
            after = _tree_manifest(Path(root))

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertNotEqual(after, before)
            self.assertEqual(after[Path("scratch")][0], "directory")

    def test_dry_run_never_launches_cargo_or_the_app_binary(self) -> None:
        for runner in RUNNERS:
            with self.subTest(runner=runner.name), tempfile.TemporaryDirectory() as root:
                root_path = Path(root)
                scripts = root_path / "scripts"
                sentinels = root_path / "sentinels"
                scripts.mkdir()
                sentinels.mkdir()
                marker = root_path / "command-called"
                for command in GUARDED_LAUNCH_COMMANDS:
                    executable = sentinels / command
                    executable.write_text(
                        "#!/usr/bin/env bash\n"
                        'printf \'%s\\n\' "$0 $*" >>"$DDGI_SENTINEL_MARKER"\n'
                        "exit 97\n",
                        encoding="utf-8",
                    )
                    os.chmod(executable, 0o755)
                absolute_app = root_path / "target" / "release" / APP_BINARY
                absolute_app.parent.mkdir(parents=True)
                absolute_app.write_text(
                    "#!/usr/bin/env bash\n"
                    'printf \'%s\\n\' "$0 $*" >>"$DDGI_SENTINEL_MARKER"\n'
                    "exit 97\n",
                    encoding="utf-8",
                )
                os.chmod(absolute_app, 0o755)
                for production_runner in RUNNERS:
                    target = scripts / production_runner.name
                    target.write_bytes(production_runner.read_bytes())
                    os.chmod(target, 0o755)
                env = dict(os.environ)
                env["PATH"] = f"{sentinels}:{env['PATH']}"
                env["DDGI_SENTINEL_MARKER"] = str(marker)
                before = _tree_manifest(root_path)

                result = subprocess.run(
                    [str(scripts / runner.name), "--dry-run"],
                    check=False,
                    capture_output=True,
                    text=True,
                    timeout=20,
                    env=env,
                )

                _assert_no_guarded_command(marker)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(_tree_manifest(root_path), before)

    def test_transport_production_sink_bypasses_tee_function_shadow(self) -> None:
        source = (SCRIPTS / "check_ddgi_transport_acceptance.sh").read_text(
            encoding="utf-8"
        )
        execute_analysis = _function_source(source, "execute_analysis")
        with tempfile.TemporaryDirectory() as root:
            root_path = Path(root)
            output = root_path / "analysis.json"
            harness = root_path / "sink-contract.sh"
            harness.write_text(
                "#!/usr/bin/env bash\n"
                "set -euo pipefail\n"
                "dry_run=false\n"
                "command() { printf command-shadow >&2; }\n"
                "cargo() { printf cargo-shadow >&2; }\n"
                "tee() { cat; }\n"
                "analyze_current_capture() { printf '%s' '{\"schema\":10}'; }\n"
                f"{execute_analysis}\n"
                f'execute_analysis "{output}"\n',
                encoding="utf-8",
            )
            os.chmod(harness, 0o755)

            result = subprocess.run(
                [str(harness)],
                check=False,
                capture_output=True,
                text=True,
                timeout=20,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout, '{"schema":10}')
            self.assertEqual(output.read_text(encoding="utf-8"), '{"schema":10}')

    def test_env_cargo_bypasses_function_shadow_and_respects_path(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            root_path = Path(root)
            sentinel_directory = root_path / "bin"
            sentinel_directory.mkdir()
            cargo = sentinel_directory / "cargo"
            cargo.write_text(
                "#!/usr/bin/env bash\n"
                "printf 'path-cargo:%s' \"$*\"\n",
                encoding="utf-8",
            )
            os.chmod(cargo, 0o755)
            env = dict(os.environ)
            env["PATH"] = f"{sentinel_directory}:{env['PATH']}"

            result = subprocess.run(
                [
                    "bash",
                    "-c",
                    "command() { printf command-shadow; }; "
                    "cargo() { printf function-cargo; }; "
                    "/usr/bin/env cargo --version",
                ],
                check=False,
                capture_output=True,
                text=True,
                timeout=20,
                env=env,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout, "path-cargo:--version")

    def test_env_python_bypasses_function_shadow(self) -> None:
        result = subprocess.run(
            [
                "bash",
                "-c",
                "python3() { printf function-python; }; "
                "/usr/bin/env python3 -c 'print(\"env-python\")'",
            ],
            check=False,
            capture_output=True,
            text=True,
            timeout=20,
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "env-python\n")

    def test_dry_run_cargo_launch_mutation_has_explicit_diagnostic(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            root_path = Path(root)
            scripts = root_path / "scripts"
            sentinels = root_path / "sentinels"
            scripts.mkdir()
            sentinels.mkdir()
            marker = root_path / "command-called"
            cargo = sentinels / "cargo"
            cargo.write_text(
                "#!/usr/bin/env bash\n"
                'printf \'cargo %s\\n\' "$*" >>"$DDGI_SENTINEL_MARKER"\n'
                "exit 97\n",
                encoding="utf-8",
            )
            os.chmod(cargo, 0o755)
            for production_runner in RUNNERS:
                source = production_runner.read_text(encoding="utf-8")
                if production_runner.name == "check_ddgi_correctness.sh":
                    source = source.replace(
                        "capture_specs=(",
                        'if $dry_run; then cargo --version; fi\n\ncapture_specs=(',
                        1,
                    )
                target = scripts / production_runner.name
                target.write_text(source, encoding="utf-8")
                os.chmod(target, 0o755)
            env = dict(os.environ)
            env["PATH"] = f"{sentinels}:{env['PATH']}"
            env["DDGI_SENTINEL_MARKER"] = str(marker)

            subprocess.run(
                [str(scripts / "check_ddgi_correctness.sh"), "--dry-run"],
                check=False,
                capture_output=True,
                text=True,
                timeout=20,
                env=env,
            )

            with self.assertRaisesRegex(
                AssertionError, r"command-called: cargo --version"
            ):
                _assert_no_guarded_command(marker)

    def test_dry_run_absolute_app_launch_mutation_has_explicit_diagnostic(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            root_path = Path(root)
            scripts = root_path / "scripts"
            scripts.mkdir()
            marker = root_path / "command-called"
            absolute_app = root_path / "target" / "release" / APP_BINARY
            absolute_app.parent.mkdir(parents=True)
            absolute_app.write_text(
                "#!/usr/bin/env bash\n"
                'printf \'%s\\n\' "$0 $*" >>"$DDGI_SENTINEL_MARKER"\n'
                "exit 97\n",
                encoding="utf-8",
            )
            os.chmod(absolute_app, 0o755)
            for production_runner in RUNNERS:
                source = production_runner.read_text(encoding="utf-8")
                if production_runner.name == "check_ddgi_correctness.sh":
                    source = source.replace(
                        "capture_specs=(",
                        'if $dry_run; then "$repo_root/target/release/'
                        f'{APP_BINARY}" --version || true; fi\n\ncapture_specs=(',
                        1,
                    )
                    failures = production_runner_invocation_failures(
                        production_runner.name, source
                    )
                    self.assertTrue(
                        any("unauthorized process launch" in failure for failure in failures),
                        failures,
                    )
                target = scripts / production_runner.name
                target.write_text(source, encoding="utf-8")
                os.chmod(target, 0o755)
            env = dict(os.environ)
            env["DDGI_SENTINEL_MARKER"] = str(marker)

            subprocess.run(
                [str(scripts / "check_ddgi_correctness.sh"), "--dry-run"],
                check=False,
                capture_output=True,
                text=True,
                timeout=20,
                env=env,
            )

            with self.assertRaisesRegex(
                AssertionError, rf"command-called: .*/target/release/{APP_BINARY} --version"
            ):
                _assert_no_guarded_command(marker)

    def test_dry_run_external_absolute_app_launch_is_statically_and_dynamically_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as root, tempfile.TemporaryDirectory() as outside:
            root_path = Path(root)
            scripts = root_path / "scripts"
            scripts.mkdir()
            marker = root_path / "command-called"
            external_app = Path(outside) / APP_BINARY
            external_app.write_text(
                "#!/usr/bin/env bash\n"
                'printf \'%s\\n\' "$0 $*" >>"$DDGI_SENTINEL_MARKER"\n'
                "exit 97\n",
                encoding="utf-8",
            )
            os.chmod(external_app, 0o755)
            for production_runner in RUNNERS:
                source = production_runner.read_text(encoding="utf-8")
                if production_runner.name == "check_ddgi_correctness.sh":
                    source = source.replace(
                        "capture_specs=(",
                        f'if $dry_run; then "{external_app}" --version || true; fi\n\n'
                        "capture_specs=(",
                        1,
                    )
                    failures = production_runner_invocation_failures(
                        production_runner.name, source
                    )
                    self.assertTrue(
                        any("unauthorized process launch" in failure for failure in failures),
                        failures,
                    )
                target = scripts / production_runner.name
                target.write_text(source, encoding="utf-8")
                os.chmod(target, 0o755)
            env = dict(os.environ)
            env["DDGI_SENTINEL_MARKER"] = str(marker)

            subprocess.run(
                [str(scripts / "check_ddgi_correctness.sh"), "--dry-run"],
                check=False,
                capture_output=True,
                text=True,
                timeout=20,
                env=env,
            )

            with self.assertRaisesRegex(
                AssertionError, rf"command-called: .*/{APP_BINARY} --version"
            ):
                _assert_no_guarded_command(marker)


if __name__ == "__main__":
    unittest.main()
