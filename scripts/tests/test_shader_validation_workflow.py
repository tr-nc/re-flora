from __future__ import annotations

import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / ".github/workflows/shader-validation.yml"
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

    def test_one_parsed_contract_owns_routes_commands_and_fail_closed_results(self) -> None:
        parsed = contract.parse_workflow_contract(
            WORKFLOW.read_text(encoding="utf-8")
        )

        self.assertEqual(parsed.failures, ())
        for event in ("pull_request", "push"):
            self.assertTrue(parsed.routes(event, "src/app/future_owner.rs"))
            self.assertTrue(
                parsed.routes(event, "scripts/ddgi_evidence/validation.py")
            )
            self.assertTrue(
                parsed.routes(event, "scripts/check_ddgi_future_acceptance.sh")
            )
        self.assertEqual(
            set(parsed.fedora_commands).intersection(contract.REQUIRED_FEDORA_COMMANDS),
            set(contract.REQUIRED_FEDORA_COMMANDS),
        )

        unsupported = WORKFLOW.read_text(encoding="utf-8").replace(
            '      - "src/**"\n', '      - "src/[future]/**"\n', 1
        )
        rejected = contract.parse_workflow_contract(unsupported)
        self.assertTrue(rejected.failures)
        self.assertFalse(rejected.routes("pull_request", "src/app/future_owner.rs"))

    def test_same_contract_owns_lighting_routes_and_real_fedora_commands(self) -> None:
        parsed = contract.parse_workflow_contract(
            WORKFLOW.read_text(encoding="utf-8")
        )

        lighting_owners = (
            "docs/lighting_mode_acceptance.md",
            "scripts/analyze_lighting_mode_acceptance.py",
            "scripts/check_lighting_mode_acceptance.sh",
            "scripts/check_lighting_mode_acceptance_source_contract.py",
            "scripts/runtime_log_diagnostics.py",
            "scripts/tests/test_analyze_lighting_mode_acceptance.py",
            "scripts/tests/test_check_lighting_mode_acceptance.py",
            "scripts/tests/test_lighting_mode_acceptance_source_contract.py",
            "src/app/core/lighting_mode_acceptance.rs",
            "src/environment_lighting.rs",
            "src/tracer/buffer_updater.rs",
        )
        for owner in lighting_owners:
            with self.subTest(owner=owner):
                self.assertIn(owner, contract.REQUIRED_OWNER_PATHS)
                self.assertTrue(parsed.routes("pull_request", owner))
                self.assertTrue(parsed.routes("push", owner))

        expected_commands = (
            "python3 -m unittest scripts.tests.test_analyze_lighting_mode_acceptance",
            "python3 -m unittest scripts.tests.test_check_lighting_mode_acceptance",
            "python3 -m unittest scripts.tests.test_lighting_mode_acceptance_source_contract",
            "cargo test --locked lighting_mode_acceptance",
            "cargo test --locked startup_log_tests::run_log_binding_marker_uses_the_existing_absolute_path",
        )
        self.assertEqual(contract.REQUIRED_LIGHTING_COMMANDS, expected_commands)
        for command in expected_commands:
            with self.subTest(command=command):
                self.assertEqual(parsed.fedora_commands.count(command), 1)

    def test_lighting_command_comments_and_disabled_steps_are_only_decoys(self) -> None:
        source = WORKFLOW.read_text(encoding="utf-8")
        command = contract.REQUIRED_LIGHTING_COMMANDS[1]
        step_header = "      - name: Test lighting-mode acceptance runner\n"
        run_line = f"        run: {command}\n"
        mutations = {
            "comment-decoy": source.replace(
                run_line,
                f"        # run: {command}\n"
                f"        # disabled decoy: {command}\n",
                1,
            ),
            "disabled-step": source.replace(
                step_header,
                step_header + "        if: false\n",
                1,
            ),
            "custom-shell": source.replace(
                step_header,
                step_header + "        shell: echo {0}\n",
                1,
            ),
            "step-environment": source.replace(
                step_header,
                step_header + "        env:\n          PATH: /tmp/fake\n",
                1,
            ),
        }

        for mutation, mutated in mutations.items():
            with self.subTest(mutation=mutation):
                self.assertNotEqual(mutated, source)
                self.assertIn(
                    f"Fedora job does not run {command}",
                    contract.workflow_contract_failures(mutated),
                )

    def test_broad_source_route_supersedes_the_dedicated_ddgi_glob(self) -> None:
        source = WORKFLOW.read_text(encoding="utf-8")
        missing_all_ddgi_routes = source.replace('      - "src/**"\n', "")
        failures = contract.workflow_contract_failures(missing_all_ddgi_routes)

        self.assertIn(
            "pull_request does not route src/ddgi/runtime.rs", failures
        )
        self.assertIn("push does not route src/ddgi/runtime.rs", failures)

    def test_runner_and_test_owners_require_their_routes_for_each_event(self) -> None:
        source = WORKFLOW.read_text(encoding="utf-8")
        cases = (
            (
                '      - "scripts/check_ddgi*.sh"\n',
                "scripts/check_ddgi_correctness.sh",
            ),
            (
                '      - "scripts/tests/**"\n',
                "scripts/tests/test_ddgi_evidence_validation.py",
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
        command = contract.REQUIRED_DDGI_COMMANDS[-1]
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

        route_tail = '      - "src/**"\n'
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

    def test_route_syntax_failure_seals_every_parsed_capability(self) -> None:
        source = WORKFLOW.read_text(encoding="utf-8")
        route = '      - "src/**"\n'
        mutations = {
            "unclosed-double-quote": '      - "src/**\n',
            "mismatched-quotes": '      - "src/**\'\n',
            "unclosed-single-quote": "      - 'src/**\n",
            "unsupported-pattern": '      - "src/[ddgi]/**"\n',
        }
        for mutation, replacement in mutations.items():
            with self.subTest(mutation=mutation):
                mutated = source.replace(route, replacement, 1)
                self.assertNotEqual(mutated, source)

                parsed = contract.parse_workflow_contract(mutated)

                self.assertTrue(parsed.failures)
                self.assertEqual(parsed.routes_by_event, ())
                self.assertEqual(parsed.fedora_commands, ())
                self.assertFalse(
                    parsed.routes("pull_request", "src/app/future_owner.rs")
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
                "pull_request does not route src/ddgi/runtime.rs",
            ),
            "scene-submodule": (
                '      - "!src/app/core/environment_lighting_test_scene/**"\n',
                f"pull_request does not route {representative_scene_module}",
            ),
        }
        route_tail = '      - "src/**"\n'
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
            + '      - "src/ddgi/runtime.rs"\n',
        )
        reincluded_failures = contract.workflow_contract_failures(reincluded)
        self.assertNotIn(
            "pull_request does not route src/ddgi/runtime.rs",
            reincluded_failures,
        )
        self.assertNotIn(
            "push does not route src/ddgi/runtime.rs", reincluded_failures
        )

    def test_comments_other_fields_other_jobs_and_disabled_steps_are_not_runs(
        self,
    ) -> None:
        source = WORKFLOW.read_text(encoding="utf-8")
        command = contract.REQUIRED_DDGI_COMMANDS[-1]
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
        command = contract.REQUIRED_DDGI_COMMANDS[-1]
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
