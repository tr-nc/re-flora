from pathlib import Path
import unittest


REPO_ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = REPO_ROOT / ".github" / "workflows" / "shader-validation.yml"


class LightingModeAcceptanceCiTests(unittest.TestCase):
    def test_shader_validation_routes_e2_sources_to_cpu_contract_gates(self) -> None:
        workflow = WORKFLOW.read_text()

        for path in (
            "scripts/analyze_lighting_mode_acceptance.py",
            "scripts/check_lighting_mode_acceptance.sh",
            "scripts/check_lighting_mode_acceptance_source_contract.py",
            "scripts/tests/test_analyze_lighting_mode_acceptance.py",
            "scripts/tests/test_check_lighting_mode_acceptance.py",
            "scripts/tests/test_lighting_mode_acceptance_ci.py",
            "scripts/tests/test_lighting_mode_acceptance_source_contract.py",
            "docs/lighting_mode_acceptance.md",
            "src/**",
        ):
            self.assertEqual(workflow.count(f'- "{path}"'), 2, path)
        for redundant_rust_route in (
            "src/app/mod.rs",
            "src/app/core/lighting_mode_acceptance.rs",
            "src/app/core/mod.rs",
            "src/tracer/buffer_updater.rs",
            "src/tracer/mod.rs",
        ):
            self.assertNotIn(f'- "{redundant_rust_route}"', workflow)
        self.assertIn(
            "python3 -m unittest scripts.tests.test_analyze_lighting_mode_acceptance",
            workflow,
        )
        self.assertIn(
            "python3 -m unittest scripts.tests.test_check_lighting_mode_acceptance",
            workflow,
        )
        self.assertIn(
            "python3 -m unittest scripts.tests.test_lighting_mode_acceptance_source_contract",
            workflow,
        )
        self.assertIn("cargo test --locked lighting_mode_acceptance", workflow)
        self.assertIn(
            "cargo test --locked startup_log_tests::run_log_binding_marker_uses_the_existing_absolute_path",
            workflow,
        )


if __name__ == "__main__":
    unittest.main()
