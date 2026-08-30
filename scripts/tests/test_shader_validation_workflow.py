from __future__ import annotations

import ast
import re
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / ".github/workflows/shader-validation.yml"
OWNER_PATHS = (
    ".github/workflows/shader-validation.yml",
    "Cargo.lock",
    "Cargo.toml",
    "config/ddgi_convergence_acceptance.toml",
    "docs/ddgi_convergence_calibration.md",
    "docs/ddgi_indirect_transport_spec.md",
    "docs/ddgi_migration_plan.md",
    "docs/ddgi_transport_acceptance.md",
    "docs/global_illumination_approaches_research.md",
    "scripts/check_ddgi_local_terrain_convergence.sh",
    "scripts/check_ddgi_transport_acceptance.sh",
    "scripts/check_latest_run_log.py",
    "scripts/perf_suite.py",
    "scripts/runtime_log_diagnostics.py",
    "scripts/summarize_ddgi_convergence.py",
    "scripts/ddgi_evidence/plan.py",
    "scripts/ddgi_evidence/executor.py",
    "scripts/ddgi_evidence/validation.py",
    "scripts/tests/test_ddgi_indirect_transport_spec.py",
    "scripts/tests/test_summarize_ddgi_convergence.py",
    "scripts/tests/test_shader_validation_workflow.py",
    "scripts/tests/test_analyze_environment_irradiance_capture.py",
    "scripts/tests/test_ddgi_evidence_plan.py",
    "scripts/tests/test_ddgi_evidence_validation.py",
    "src/ddgi/mod.rs",
    "src/ddgi/resources.rs",
    "src/ddgi/runtime.rs",
    "src/tracer/mod.rs",
)


class ConvergenceShaderValidationWorkflowTests(unittest.TestCase):
    @staticmethod
    def yaml_sequence(workflow: str, path: tuple[str, ...]) -> list[str]:
        lines = workflow.splitlines()
        start = 0
        end = len(lines)
        for depth, key in enumerate(path):
            indent = depth * 2
            header = " " * indent + key + ":"
            match = next(
                index
                for index in range(start, end)
                if lines[index].split("#", 1)[0].rstrip() == header
            )
            start = match + 1
            end = next(
                (
                    index
                    for index in range(start, end)
                    if (content := lines[index].split("#", 1)[0].rstrip())
                    and len(content) - len(content.lstrip()) <= indent
                ),
                end,
            )
        item_indent = " " * (len(path) * 2)
        values = []
        for line in lines[start:end]:
            content = line.split("#", 1)[0].rstrip()
            if content.startswith(item_indent + "- "):
                value = content[len(item_indent) + 2 :]
                if value.startswith(('"', "'")):
                    value = ast.literal_eval(value)
                values.append(value)
        return values

    @staticmethod
    def path_pattern(pattern: str) -> re.Pattern[str]:
        if any(character in pattern for character in "?[]{}\\") or re.search(
            r"(?:@|\+|\?|\*|!)\(", pattern
        ):
            raise ValueError(f"unsupported workflow path syntax: {pattern}")
        expression = []
        index = 0
        while index < len(pattern):
            if pattern.startswith("**", index):
                expression.append(".*")
                index += 2
            elif pattern[index] == "*":
                expression.append("[^/]*")
                index += 1
            else:
                expression.append(re.escape(pattern[index]))
                index += 1
        return re.compile("^" + "".join(expression) + "$")

    @classmethod
    def path_is_included(cls, patterns: list[str], path: str) -> bool:
        included = False
        for pattern in patterns:
            if pattern.startswith("!("):
                raise ValueError(f"unsupported workflow path syntax: {pattern}")
            excluded = pattern.startswith("!")
            candidate = pattern[1:] if excluded else pattern
            if not candidate:
                raise ValueError("empty workflow path pattern")
            if cls.path_pattern(candidate).fullmatch(path):
                included = not excluded
        return included

    @staticmethod
    def append_event_path(workflow: str, event: str, pattern: str, quote: str) -> str:
        event_start = workflow.index(f"  {event}:\n")
        paths_start = workflow.index("    paths:\n", event_start)
        next_boundary = re.search(
            r"(?m)^(?:  [A-Za-z_][^:]*:|[^\s#])", workflow[paths_start + 1 :]
        )
        if next_boundary is None:
            raise AssertionError(f"{event} paths have no enclosing block boundary")
        end = paths_start + 1 + next_boundary.start()
        return workflow[:end] + f"      - {quote}{pattern}{quote}\n" + workflow[end:]

    def test_policy_inputs_and_targeted_rust_gates_are_continuous(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")

        self.assertEqual(workflow.count('      - "src/environment_lighting.rs"'), 2)
        self.assertEqual(workflow.count('      - "src/ddgi/**"'), 2)
        for path in (
            "config/camera_snapshots.toml",
            "config/ddgi_convergence_acceptance.toml",
            "docs/ddgi_convergence_calibration.md",
            "docs/ddgi_indirect_transport_spec.md",
            "docs/ddgi_transport_acceptance.md",
            "scripts/analyze_bd2_blue_voxel.py",
            "scripts/check_bd2_blue_voxel.sh",
            "scripts/check_ddgi_sky_normalization_evidence.py",
            "scripts/check_latest_run_log.py",
            "scripts/perf_suite.py",
            "scripts/runtime_log_diagnostics.py",
            "scripts/summarize_ddgi_convergence.py",
            "scripts/ddgi_evidence/**",
            "src/app/core/mod.rs",
            "src/app/core/environment_lighting_test_scene.rs",
            "src/app/core/environment_irradiance_capture.rs",
            "src/app/core/loading.rs",
            "src/app/core/visible_terrain.rs",
            "src/main.rs",
            "src/run_log.rs",
            "src/tracer/mod.rs",
        ):
            self.assertEqual(workflow.count(f'      - "{path}"'), 2, path)
        self.assertIn('      - "shader/**"', workflow)
        self.assertIn('      - "scripts/check_ddgi*.sh"', workflow)
        self.assertIn('      - "scripts/ddgi_evidence/**"', workflow)
        self.assertIn('      - "scripts/tests/**"', workflow)
        self.assertIn("timeout 10m cargo test --locked environment_lighting::tests::", workflow)
        self.assertIn(
            "timeout 10m cargo test --locked environment_lighting_test_scene::tests:: --",
            workflow,
        )
        self.assertIn(
            "--skip app::core::environment_lighting_test_scene::tests::"
            "patt_seam_replay_uses_the_saved_snapshot_and_only_punches_the_roof",
            workflow,
        )
        self.assertIn("timeout 10m cargo test --locked startup_log_tests::", workflow)
        for gate in (
            "ddgi_convergence_evidence_tests::",
            "runtime_convergence_budget_matches_the_acceptance_contract",
            "private_evidence_lines_preserve_exact_count_content_and_order",
            "validation_wire_labels_bind_to_their_distinct_runtime_facts",
        ):
            self.assertIn(f"timeout 10m cargo test --locked {gate}", workflow)
        for gate in (
            "scripts.tests.test_ddgi_evidence_plan",
            "scripts.tests.test_ddgi_evidence_cli",
            "scripts.tests.test_ddgi_evidence_validation",
            "scripts.tests.test_validate_ddgi_density_lifecycle",
            "scripts.tests.test_validate_ddgi_radiance_lifecycle",
            "scripts.tests.test_analyze_environment_irradiance_capture."
            "AnalyzeEnvironmentIrradianceCaptureTests."
            "test_production_cli_accepts_only_current_without_a_version_surface",
            "scripts.tests.test_analyze_environment_irradiance_capture."
            "AnalyzeEnvironmentIrradianceCaptureTests."
            "test_cli_defaults_to_current_and_requires_explicit_compatibility",
            "scripts.tests.test_summarize_ddgi_convergence",
            "scripts.tests.test_ddgi_indirect_transport_spec",
            "scripts.tests.test_runtime_log_diagnostics",
            "scripts.tests.test_check_latest_run_log",
            "scripts.tests.test_perf_suite",
        ):
            self.assertIn(gate, workflow)
        self.assertIn("timeout-minutes: 45", workflow)
        self.assertIn("timeout-minutes: 10", workflow)

    def test_all_convergence_owners_route_through_pull_request_and_push(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        for event in ("pull_request", "push"):
            patterns = self.yaml_sequence(workflow, ("on", event, "paths"))
            future_rust_owner = "src/app/reviewer_fixture.rs"
            self.assertTrue(
                self.path_is_included(patterns, future_rust_owner),
                f"{event} paths do not include future Rust owners through src/**",
            )
            excluded_rust = self.append_event_path(workflow, event, "!src/**", '"')
            self.assertFalse(
                self.path_is_included(
                    self.yaml_sequence(excluded_rust, ("on", event, "paths")),
                    future_rust_owner,
                ),
                f"{event} ignored a trailing all-Rust exclusion",
            )
            for owner in OWNER_PATHS:
                self.assertTrue(
                    self.path_is_included(patterns, owner),
                    f"{event} paths do not finally include {owner}",
                )

                for quote in ('"', "'"):
                    excluded = self.append_event_path(workflow, event, f"!{owner}", quote)
                    self.assertFalse(
                        self.path_is_included(
                            self.yaml_sequence(excluded, ("on", event, "paths")),
                            owner,
                        ),
                        f"{event} ignored a trailing {quote}-quoted exclusion for {owner}",
                    )

    def test_workflow_globs_are_slash_aware_and_unsupported_syntax_fails_closed(self) -> None:
        self.assertTrue(self.path_is_included(["src/ddgi/**"], "src/ddgi/runtime.rs"))
        self.assertFalse(self.path_is_included(["src/*"], "src/ddgi/runtime.rs"))
        self.assertTrue(self.path_is_included(["src/**"], "src/ddgi/runtime.rs"))
        unsupported = (
            "src/[dt]*",
            "src/ddgi/?.rs",
            "src/@(ddgi|tracer)/**",
            "src/+(ddgi)/**",
            "src/?(ddgi)/**",
            "src/*(ddgi)/**",
            "!(src/ddgi/**)",
        )
        for pattern in unsupported:
            with self.subTest(pattern=pattern), self.assertRaises(ValueError):
                self.path_is_included([pattern], "src/ddgi/runtime.rs")
SCRIPTS = ROOT / "scripts"
sys.path.insert(0, str(SCRIPTS))

import shader_validation_workflow_contract as contract  # noqa: E402


class RfirrShaderValidationWorkflowTests(unittest.TestCase):
    def remove_event_route(self, source: str, event: str, route: str) -> str:
        lines = source.splitlines(keepends=True)
        inside_event = False
        for index, line in enumerate(lines):
            if line == f"  {event}:\n":
                inside_event = True
                continue
            if inside_event and line.strip() and len(line) - len(line.lstrip()) <= 2:
                break
            if inside_event and line == route:
                lines[index] = ""
                return "".join(lines)
        self.fail(f"route {route.strip()} not found under {event}")

    def test_owner_changes_route_to_executable_fedora_evidence_tests(self) -> None:
        required_runners = {
            "scripts/check_ddgi_correctness.sh",
            "scripts/check_ddgi_inflight_terrain_edits.sh",
            "scripts/check_ddgi_lifecycle_acceptance.sh",
            "scripts/check_ddgi_local_terrain_convergence.sh",
            "scripts/check_ddgi_runtime_terrain_edits.sh",
            "scripts/check_ddgi_terrain_edit_cycle.sh",
            "scripts/check_ddgi_transport_acceptance.sh",
        }
        self.assertTrue(required_runners.issubset(contract.REQUIRED_OWNER_PATHS))
        self.assertEqual(
            contract.workflow_contract_failures(
                WORKFLOW.read_text(encoding="utf-8")
            ),
            [],
        )

    def test_broad_source_route_supersedes_the_dedicated_ddgi_glob(self) -> None:
        source = WORKFLOW.read_text(encoding="utf-8")
        missing_ddgi_route = source.replace('      - "src/ddgi/**"\n', "")

        self.assertEqual(
            contract.workflow_contract_failures(missing_ddgi_route), []
        )

        missing_all_ddgi_routes = missing_ddgi_route.replace(
            '      - "src/**"\n', ""
        )
        failures = contract.workflow_contract_failures(missing_all_ddgi_routes)

        self.assertIn(
            "pull_request does not route src/ddgi/resources.rs", failures
        )
        self.assertIn("push does not route src/ddgi/resources.rs", failures)

    def test_broad_source_route_covers_the_scene_submodule_for_each_event(self) -> None:
        source = WORKFLOW.read_text(encoding="utf-8")
        route = '      - "src/app/core/environment_lighting_test_scene/**"\n'
        representative = (
            "src/app/core/environment_lighting_test_scene/local_light_scaling.rs"
        )

        missing_pull_request_dedicated = source.replace(route, "", 1)
        push_offset = source.index("  push:")
        missing_push_dedicated = source[:push_offset] + source[push_offset:].replace(
            route, "", 1
        )

        self.assertEqual(
            contract.workflow_contract_failures(missing_pull_request_dedicated), []
        )
        self.assertEqual(
            contract.workflow_contract_failures(missing_push_dedicated), []
        )

        missing_pull_request = self.remove_event_route(
            missing_pull_request_dedicated,
            "pull_request",
            '      - "src/**"\n',
        )
        missing_push = self.remove_event_route(
            missing_push_dedicated, "push", '      - "src/**"\n'
        )

        pull_request_failures = contract.workflow_contract_failures(
            missing_pull_request
        )
        push_failures = contract.workflow_contract_failures(missing_push)
        self.assertIn(
            f"pull_request does not route {representative}",
            pull_request_failures,
        )
        self.assertNotIn(f"push does not route {representative}", pull_request_failures)
        self.assertIn(f"push does not route {representative}", push_failures)
        self.assertNotIn(
            f"pull_request does not route {representative}", push_failures
        )

    def test_runner_and_test_owners_require_their_routes_for_each_event(self) -> None:
        source = WORKFLOW.read_text(encoding="utf-8")
        cases = (
            (
                '      - "scripts/check_ddgi*.sh"\n',
                "scripts/check_ddgi_correctness.sh",
            ),
            (
                '      - "scripts/tests/**"\n',
                "scripts/tests/test_ddgi_evidence_plan.py",
            ),
        )
        for event in ("pull_request", "push"):
            for route, representative in cases:
                with self.subTest(event=event, representative=representative):
                    mutated = self.remove_event_route(source, event, route)
                    failures = contract.workflow_contract_failures(mutated)
                    self.assertIn(
                        f"{event} does not route {representative}", failures
                    )

    def test_runner_glob_cannot_be_narrowed_to_only_correctness(self) -> None:
        source = WORKFLOW.read_text(encoding="utf-8")
        route = '      - "scripts/check_ddgi*.sh"\n'
        narrowed = '      - "scripts/check_ddgi_correctness.sh"\n'
        for event in ("pull_request", "push"):
            with self.subTest(event=event):
                mutated = self.remove_event_route(source, event, route)
                event_header = f"  {event}:\n"
                event_offset = mutated.index(event_header)
                paths_offset = mutated.index("    paths:\n", event_offset)
                insertion = paths_offset + len("    paths:\n")
                mutated = mutated[:insertion] + narrowed + mutated[insertion:]
                failures = contract.workflow_contract_failures(mutated)
                self.assertIn(
                    f"{event} does not route scripts/check_ddgi_inflight_terrain_edits.sh",
                    failures,
                )

    def test_sky_normalization_dependency_requires_its_own_route(self) -> None:
        source = WORKFLOW.read_text(encoding="utf-8")
        route = '      - "scripts/check_ddgi_sky_normalization_evidence.py"\n'
        owner = "scripts/check_ddgi_sky_normalization_evidence.py"
        for event in ("pull_request", "push"):
            with self.subTest(event=event):
                mutated = self.remove_event_route(source, event, route)
                self.assertIn(
                    f"{event} does not route {owner}",
                    contract.workflow_contract_failures(mutated),
                )

    def test_on_and_jobs_must_be_unique_root_parents(self) -> None:
        source = WORKFLOW.read_text(encoding="utf-8")
        command = contract.REQUIRED_FEDORA_COMMANDS[-1]
        mutations = {
            "renamed-on": source.replace("on:\n", "decoy:\n", 1),
            "renamed-jobs": source.replace("jobs:\n", "decoy:\n", 1),
            "decoy-events": source.replace(
                "  pull_request:\n", "  disabled_pull_request:\n", 1
            ).replace(
                "on:\n",
                "decoy:\n"
                "  pull_request:\n"
                "    paths:\n"
                '      - "scripts/**"\n'
                "  push:\n"
                "    paths:\n"
                '      - "scripts/**"\n'
                "on:\n",
                1,
            ),
            "decoy-fedora": source.replace(
                "  fedora:\n", "  disabled_fedora:\n", 1
            ).replace(
                "jobs:\n",
                "decoy:\n"
                "  fedora:\n"
                "    container: fedora:43\n"
                "    steps:\n"
                "      - name: Decode Rust DDGI evidence fixture\n"
                f"        run: {command}\n"
                "jobs:\n",
                1,
            ),
        }
        for name, mutated in mutations.items():
            with self.subTest(name=name):
                self.assertNotEqual(contract.workflow_contract_failures(mutated), [])

    def test_glob_subset_supports_zero_directory_double_star_and_rejects_specials(
        self,
    ) -> None:
        source = WORKFLOW.read_text(encoding="utf-8")
        zero_directory = source.replace(
            '      - "scripts/check_ddgi*.sh"\n',
            '      - "scripts/**/check_ddgi*.sh"\n',
        )
        for event in ("pull_request", "push"):
            self.assertNotIn(
                f"{event} does not route scripts/check_ddgi_correctness.sh",
                contract.workflow_contract_failures(zero_directory),
            )

        route_tail = '      - "src/tracer/**"\n'
        for pattern in ("src/ddgi/?", "src/ddgi/+", "src/ddgi/[a]", "src/@(ddgi)/**"):
            with self.subTest(pattern=pattern):
                mutated = source.replace(
                    route_tail,
                    route_tail + f'      - "{pattern}"\n',
                    1,
                )
                self.assertIn(
                    f"pull_request uses unsupported route pattern {pattern}",
                    contract.workflow_contract_failures(mutated),
                )

    def test_later_exclusions_override_positive_owner_routes(self) -> None:
        source = WORKFLOW.read_text(encoding="utf-8")
        representative_scene_module = (
            "src/app/core/environment_lighting_test_scene/local_light_scaling.rs"
        )
        self.assertIn(
            representative_scene_module, contract.REQUIRED_OWNER_PATHS
        )

        mutations = {
            "ddgi": (
                '      - "!src/ddgi/**"\n',
                "pull_request does not route src/ddgi/resources.rs",
            ),
            "scene-submodule": (
                '      - "!src/app/core/environment_lighting_test_scene/**"\n',
                f"pull_request does not route {representative_scene_module}",
            ),
        }
        route_tail = '      - "src/tracer/**"\n'
        for mutation, (exclusion, expected_failure) in mutations.items():
            with self.subTest(mutation=mutation):
                excluded = source.replace(route_tail, route_tail + exclusion)
                failures = contract.workflow_contract_failures(excluded)
                self.assertIn(expected_failure, failures)
                self.assertIn(
                    expected_failure.replace("pull_request", "push"), failures
                )

        reincluded = source.replace(
            route_tail,
            route_tail
            + '      - "!src/ddgi/**"\n'
            + '      - "src/ddgi/resources.rs"\n',
        )
        reincluded_failures = contract.workflow_contract_failures(reincluded)
        self.assertNotIn(
            "pull_request does not route src/ddgi/resources.rs",
            reincluded_failures,
        )
        self.assertNotIn(
            "push does not route src/ddgi/resources.rs", reincluded_failures
        )

    def test_comments_other_fields_other_jobs_and_disabled_steps_are_not_runs(
        self,
    ) -> None:
        source = WORKFLOW.read_text(encoding="utf-8")
        command = contract.REQUIRED_FEDORA_COMMANDS[-1]
        step_name = "Decode Rust DDGI evidence fixture"
        step_header = f"      - name: {step_name}\n"
        run_line = f"        run: {command}"

        mutations = {
            "comment": source.replace(run_line, f"        # run: {command}", 1),
            "other-field": source.replace(
                run_line,
                "        env:\n"
                f'          MOVED_COMMAND: "{command}"',
                1,
            ),
            "other-job": source.replace(run_line, "        run: true", 1).replace(
                "      - name: Run Slang CPU tests\n",
                "      - name: Unrelated desktop command\n"
                f"        run: {command}\n\n"
                "      - name: Run Slang CPU tests\n",
                1,
            ),
            "disabled-step": source.replace(
                step_header, step_header + "        if: false\n",
                1,
            ),
            "disabled-job": source.replace(
                "  fedora:\n", "  fedora:\n    if: false\n", 1
            ),
            "multiline-disabled-job": source.replace(
                "  fedora:\n", "  fedora:\n    if: |\n      false\n", 1
            ),
            "multiline-disabled-step": source.replace(
                step_header, step_header + "        if: |\n          false\n",
                1,
            ),
            "continue-on-error": source.replace(
                step_header, step_header + "        continue-on-error: true\n",
                1,
            ),
            "expression-false": source.replace(
                step_header, step_header + "        if: ${{ 1 == 0 }}\n", 1
            ),
            "expression-continue-on-error": source.replace(
                step_header,
                step_header + "        continue-on-error: ${{ 1 == 1 }}\n",
                1,
            ),
            "custom-shell": source.replace(
                step_header, step_header + "        shell: echo {0}\n", 1
            ),
            "step-path": source.replace(
                step_header,
                step_header + "        env:\n          PATH: /tmp/fake\n",
                1,
            ),
            "step-bash-env": source.replace(
                step_header,
                step_header + "        env:\n          BASH_ENV: /tmp/fake\n",
                1,
            ),
            "working-directory": source.replace(
                step_header, step_header + "        working-directory: /tmp\n", 1
            ),
            "job-expression-false": source.replace(
                "  fedora:\n", "  fedora:\n    if: ${{ 1 == 0 }}\n", 1
            ),
            "job-continue-on-error": source.replace(
                "  fedora:\n", "  fedora:\n    continue-on-error: true\n", 1
            ),
            "job-env": source.replace(
                "  fedora:\n", "  fedora:\n    env:\n      PATH: /tmp/fake\n", 1
            ),
            "container-drift": source.replace(
                "    container: fedora:43", "    container: ubuntu:latest", 1
            ),
            "job-default-shell": source.replace(
                "  fedora:\n",
                "  fedora:\n    defaults:\n      run:\n        shell: echo {0}\n",
                1,
            ),
            "workflow-default-shell": source.replace(
                "permissions:\n",
                "defaults:\n  run:\n    shell: echo {0}\n\npermissions:\n",
                1,
            ),
            "workflow-path": source.replace(
                "env:\n  CARGO_TERM_COLOR: always\n",
                "env:\n  CARGO_TERM_COLOR: always\n  PATH: /tmp/fake\n",
                1,
            ),
            "workflow-bash-env": source.replace(
                "env:\n  CARGO_TERM_COLOR: always\n",
                "env:\n  CARGO_TERM_COLOR: always\n  BASH_ENV: /tmp/fake\n",
                1,
            ),
            "wrapper-and-exit": source.replace(
                run_line,
                "        run: |\n"
                "          evidence() { true; }\n"
                "          evidence\n"
                "          exit 0\n"
                f"          {command}",
                1,
            ),
            "block-scalar": source.replace(
                run_line,
                "        run: |\n" f"          {command}",
                1,
            ),
            "duplicate-valid-step": source.replace(
                step_header,
                step_header
                + f"        run: {command}\n\n"
                + "      - name: Duplicate current fixture decoder\n",
                1,
            ),
        }

        for mutation, mutated_source in mutations.items():
            with self.subTest(mutation=mutation):
                self.assertIn(
                    f"Fedora job does not run {command}",
                    contract.workflow_contract_failures(mutated_source),
                )

    def test_quoted_step_keys_are_normalized_before_capability_checks(self) -> None:
        source = WORKFLOW.read_text(encoding="utf-8")
        command = contract.REQUIRED_FEDORA_COMMANDS[-1]
        step_header = "      - name: Decode Rust DDGI evidence fixture\n"
        quoted_valid = source.replace(
            step_header,
            '      - "name": Decode Rust DDGI evidence fixture\n',
            1,
        ).replace(f"        run: {command}", f'        "run": {command}', 1)
        self.assertEqual(contract.workflow_contract_failures(quoted_valid), [])

        mutations = {
            "if": '        "if": false\n',
            "env": '        "env": {"PATH": "/tmp/fake"}\n',
            "shell": '        "shell": echo {0}\n',
            "continue": '        "continue-on-error": true\n',
            "working-directory": '        "working-directory": /tmp\n',
        }
        for name, field in mutations.items():
            with self.subTest(name=name):
                mutated = source.replace(step_header, step_header + field, 1)
                self.assertIn(
                    f"Fedora job does not run {command}",
                    contract.workflow_contract_failures(mutated),
                )
if __name__ == "__main__":
    unittest.main()
