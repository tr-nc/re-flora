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

    def test_comments_other_fields_other_jobs_and_disabled_steps_are_not_runs(
        self,
    ) -> None:
        source = WORKFLOW.read_text(encoding="utf-8")
        command = contract.REQUIRED_FEDORA_COMMANDS[-1]
        run_line = f"          {command}"

        mutations = {
            "comment": source.replace(run_line, f"          # {command}", 1),
            "other-field": source.replace(
                run_line,
                "          true\n"
                "        env:\n"
                f'          MOVED_COMMAND: "{command}"',
                1,
            ),
            "other-job": source.replace(run_line, "          true", 1).replace(
                "      - name: Run Slang CPU tests\n",
                "      - name: Unrelated desktop command\n"
                f"        run: {command}\n\n"
                "      - name: Run Slang CPU tests\n",
                1,
            ),
            "disabled-step": source.replace(
                "      - name: Run DDGI owner evidence codec tests\n",
                "      - name: Run DDGI owner evidence codec tests\n"
                "        if: false\n",
                1,
            ),
            "disabled-job": source.replace(
                "  fedora:\n", "  fedora:\n    if: false\n", 1
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
