from __future__ import annotations

import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SCRIPTS = ROOT / "scripts"
WORKFLOW = ROOT / ".github" / "workflows" / "shader-validation.yml"
sys.path.insert(0, str(SCRIPTS))

import shader_validation_workflow_contract as contract  # noqa: E402


class ShaderValidationWorkflowTests(unittest.TestCase):
    def test_owner_changes_route_to_executable_fedora_evidence_tests(self) -> None:
        self.assertEqual(
            contract.workflow_contract_failures(
                WORKFLOW.read_text(encoding="utf-8")
            ),
            [],
        )

    def test_glob_removal_reports_the_actual_unrouted_owner(self) -> None:
        source = WORKFLOW.read_text(encoding="utf-8")
        missing_ddgi_route = source.replace('      - "src/ddgi/**"\n', "")

        failures = contract.workflow_contract_failures(missing_ddgi_route)

        self.assertIn(
            "pull_request does not route src/ddgi/resources.rs", failures
        )
        self.assertIn("push does not route src/ddgi/resources.rs", failures)

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
        }

        for mutation, mutated_source in mutations.items():
            with self.subTest(mutation=mutation):
                self.assertIn(
                    f"Fedora job does not run {command}",
                    contract.workflow_contract_failures(mutated_source),
                )


if __name__ == "__main__":
    unittest.main()
