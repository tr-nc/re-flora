from __future__ import annotations

import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / ".github/workflows/shader-validation.yml"


class ShaderValidationWorkflowTests(unittest.TestCase):
    def test_policy_inputs_and_targeted_rust_gates_are_continuous(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")

        self.assertEqual(workflow.count('      - "src/environment_lighting.rs"'), 2)
        for path in (
            "config/camera_snapshots.toml",
            "scripts/analyze_bd2_blue_voxel.py",
            "scripts/check_bd2_blue_voxel.sh",
            "scripts/summarize_ddgi_convergence.py",
            "scripts/validate_capture_process_evidence.py",
            "src/app/core/environment_lighting_test_scene.rs",
            "src/app/core/loading.rs",
            "src/main.rs",
            "src/tracer/mod.rs",
        ):
            self.assertEqual(workflow.count(f'      - "{path}"'), 2, path)
        self.assertIn('      - "shader/**"', workflow)
        self.assertIn('      - "scripts/check_ddgi*.sh"', workflow)
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
        self.assertIn("timeout-minutes: 45", workflow)
        self.assertIn("timeout-minutes: 10", workflow)


if __name__ == "__main__":
    unittest.main()
