from __future__ import annotations

import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / ".github/workflows/shader-validation.yml"


class ShaderValidationWorkflowTests(unittest.TestCase):
    def test_policy_inputs_and_targeted_rust_gates_are_continuous(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")

        self.assertEqual(workflow.count('      - "src/environment_lighting.rs"'), 2)
        self.assertIn('      - "shader/**"', workflow)
        self.assertIn('      - "scripts/check_ddgi*.sh"', workflow)
        self.assertIn('      - "scripts/tests/**"', workflow)
        self.assertIn("timeout 10m cargo test --locked environment_lighting::tests::", workflow)
        self.assertIn("timeout-minutes: 45", workflow)
        self.assertIn("timeout-minutes: 10", workflow)


if __name__ == "__main__":
    unittest.main()
