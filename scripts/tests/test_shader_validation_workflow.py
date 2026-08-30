from __future__ import annotations

import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / ".github/workflows/shader-validation.yml"


class ShaderValidationWorkflowTests(unittest.TestCase):
    @staticmethod
    def event_route(workflow: str, event: str, following: str) -> str:
        start = workflow.index(f"  {event}:\n")
        following_header = (
            f"\n{following}:\n"
            if following == "permissions"
            else f"  {following}:\n"
        )
        end = workflow.index(following_header, start)
        return workflow[start:end]

    def test_policy_inputs_and_targeted_rust_gates_are_continuous(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")

        self.assertEqual(workflow.count('      - "src/environment_lighting.rs"'), 2)
        self.assertEqual(workflow.count('      - "src/ddgi/**"'), 2)
        for path in (
            "config/camera_snapshots.toml",
            "config/ddgi_convergence_acceptance.toml",
            "docs/ddgi_convergence_calibration.md",
            "docs/ddgi_indirect_transport_spec.md",
            "docs/ddgi_migration_plan.md",
            "docs/ddgi_transport_acceptance.md",
            "scripts/analyze_bd2_blue_voxel.py",
            "scripts/check_bd2_blue_voxel.sh",
            "scripts/check_ddgi_sky_normalization_evidence.py",
            "scripts/summarize_ddgi_convergence.py",
            "scripts/validate_capture_process_evidence.py",
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
        self.assertIn('      - "scripts/lib/**"', workflow)
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
            "validated_publication_capsule_owns_field_validation_and_terminal_identity",
        ):
            self.assertIn(f"timeout 10m cargo test --locked {gate}", workflow)
        for gate in (
            "scripts.tests.test_validate_capture_process_evidence",
            "scripts.tests.test_summarize_ddgi_convergence",
            "scripts.tests.test_check_ddgi_transport_acceptance",
            "scripts.tests.test_ddgi_capture_process_integration",
            "scripts.tests.test_ddgi_indirect_transport_spec",
        ):
            self.assertIn(gate, workflow)
        self.assertIn("timeout-minutes: 45", workflow)
        self.assertIn("timeout-minutes: 10", workflow)

    def test_migration_document_routes_through_pull_request_and_push(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        path = '      - "docs/ddgi_migration_plan.md"'
        pull_request = self.event_route(workflow, "pull_request", "push")
        push = self.event_route(workflow, "push", "permissions")

        self.assertEqual(pull_request.count(path), 1)
        self.assertEqual(push.count(path), 1)
        for event, following in (("pull_request", "push"), ("push", "permissions")):
            route = self.event_route(workflow, event, following)
            mutated = workflow.replace(route, route.replace(path, "", 1), 1)
            with self.assertRaises(AssertionError):
                self.assertEqual(
                    self.event_route(mutated, event, following).count(path), 1
                )


if __name__ == "__main__":
    unittest.main()
