"""Fast oracle tests; actual receiver values come from separate Release GPU runs."""

import math
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import analyze_terrain_hybrid_lighting as energy


class TerrainHybridEnergyTests(unittest.TestCase):
    def measure(self, isolated, control, *, variation: float = 0.0, reference=False):
        values = {c: [isolated] * 8 for c in energy.ISOLATED}
        values[energy.CONTROL] = [control] * 8
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "hybrid.gui.toml").write_text(f"""[[section]]
name = "fixture"
[[section.param]]
id = "terrain_rock_strength"
[section.param.data]
value = {variation}
[[section.param]]
id = "path_tracing_reference"
[section.param.data]
value = {str(reference).lower()}
""")
            (root / "hybrid.log").write_text("sun_direction=Vec3(0, 0.6, 0.8)")
            with patch.object(energy, "samples", return_value=(values, (960, 540))):
                return energy.measure_current(root)

    def test_surface_mean_uses_cube_area_and_quantized_control_normal(self):
        control = (0.6 + 253 * 0.8) / math.sqrt(253**2 + 2)
        self.assertEqual(self.measure(1.4 / 6, control)["verdict"], "GREEN")
        # Retired upward-fallback brightness must fail, not just render successfully.
        self.assertEqual(self.measure(0.6, control)["verdict"], "RED")

    def test_partial_current_capture_does_not_fall_back_to_old_ab_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "hybrid.log").touch()
            with patch.object(energy, "measure_legacy") as legacy:
                with self.assertRaises(OSError):
                    energy.measure(root)
                legacy.assert_not_called()

    def test_rejects_nonuniform_albedo_reference_and_unlit_control(self):
        with self.assertRaises(ValueError):
            self.measure(0.25, 0.8, variation=0.3)
        with self.assertRaises(ValueError):
            self.measure(0.25, 0.8, reference=True)
        with self.assertRaises(ValueError):
            self.measure(0.25, 0)


if __name__ == "__main__":
    unittest.main()
