import importlib.util
import subprocess
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CHECKER = ROOT / "scripts/check_render_frame_input_contract.py"
SPEC = importlib.util.spec_from_file_location("render_frame_input_contract", CHECKER)
assert SPEC is not None and SPEC.loader is not None
CONTRACT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CONTRACT)


class RenderFrameInputContractTests(unittest.TestCase):
    def test_repository_satisfies_the_typed_frame_contract(self) -> None:
        result = subprocess.run(
            [sys.executable, str(CHECKER)],
            cwd=ROOT,
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_flat_parameter_regression_is_rejected(self) -> None:
        tracer = """
pub struct TerrainFrameInput;
pub struct MaterialFrameInput;
pub struct VegetationFrameInput;
pub struct WindFrameInput;
pub struct EnvironmentFrameInput;
pub fn update_buffers(&mut self, a: u32, b: u32, c: u32, d: u32, e: u32,
                      f: u32, g: u32, h: u32, i: u32, j: u32) {}
"""
        updater = "pub fn update_gui_input(a: u32) {}"
        app = "TerrainFrameInput MaterialFrameInput VegetationFrameInput WindFrameInput EnvironmentFrameInput"
        errors = CONTRACT.validate(tracer, updater, app)
        self.assertIn("Tracer::update_buffers exposes 11 parameters (max 10)", errors)


if __name__ == "__main__":
    unittest.main()
