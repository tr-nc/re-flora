import contextlib
import io
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import check_terrain_hybrid_lighting as capture
from check_raster_tree_static import setting


class TerrainHybridCaptureTests(unittest.TestCase):
    def run_capture(self, fail=False, irradiance=False):
        original = (capture.ROOT / "config/gui.toml").read_bytes()
        observed = []
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "config").mkdir()
            gui = root / "config/gui.toml"
            gui.write_bytes(original)
            out = root / "out"
            out.mkdir()

            def run(command, **kwargs):
                observed.append(gui.read_text())
                self.assertIn("thin-voxels", command)
                if irradiance:
                    self.assertNotIn("--screenshot", command)
                    artifact = command[
                        command.index("--environment-irradiance-capture") + 1
                    ]
                else:
                    self.assertIn("environment-test-scene", command)
                    self.assertNotIn("--environment-irradiance-capture", command)
                    artifact = command[command.index("--screenshot") + 2]
                if fail:
                    raise subprocess.CalledProcessError(1, command)
                Path(artifact).write_bytes(b"fixture")
                kwargs["stdout"].write("Application exited successfully\n")

            with (
                patch.object(capture, "ROOT", root),
                patch.object(capture.fcntl, "flock"),
                patch.object(capture.subprocess, "run", side_effect=run),
                contextlib.redirect_stdout(io.StringIO()),
            ):
                if fail:
                    with self.assertRaises(subprocess.CalledProcessError):
                        capture.capture(root / "binary", out, irradiance)
                else:
                    capture.capture(root / "binary", out, irradiance)
            self.assertEqual(gui.read_bytes(), original)
        return observed

    def test_single_capture_needs_no_switch_and_restores_configuration(self):
        observed = self.run_capture()
        self.assertEqual(len(observed), 1)
        source = observed[0]
        self.assertNotIn('id = "terrain_hybrid_lighting"', source)
        self.assertEqual(setting(source, "path_tracing_reference", "false"), source)

    def test_linear_capture_uses_uniform_rock_for_the_control_ratio(self):
        source = self.run_capture(irradiance=True)[0]
        self.assertEqual(setting(source, "terrain_rock_strength", "0.0"), source)

    def test_retired_order_and_invalid_repeat_counts_have_recovery_guidance(self):
        for args, message in [
            (["--order", "A,B"], "--order was removed"),
            (["--benchmark", "--runs", "0"], "--runs must be positive"),
            (["--runs", "2"], "require --benchmark"),
        ]:
            errors = io.StringIO()
            with (
                patch.object(sys, "argv", ["capture", *args]),
                contextlib.redirect_stderr(errors),
                self.assertRaises(SystemExit) as exit,
            ):
                capture.main()
            self.assertEqual(exit.exception.code, 2)
            self.assertIn(message, errors.getvalue())

    def test_one_shot_irradiance_does_not_preempt_a_delayed_screenshot(self):
        self.assertEqual(len(self.run_capture(irradiance=True)), 1)
        self.assertEqual(len(self.run_capture(fail=True, irradiance=True)), 1)

    def test_failure_restores_configuration(self):
        self.assertEqual(len(self.run_capture(fail=True)), 1)

    def test_benchmark_records_the_final_actual_render_extents(self):
        text = (
            "[RESIZE] published generation=1 extent=1600x900 tracer_generation=1\n"
            "[GOD_RAY][RESOURCES] scene=800x450 effect=800x450\n"
            "[RESIZE] published generation=2 extent=1920x1080 tracer_generation=2\n"
            "[GOD_RAY][RESOURCES] scene=960x540 effect=960x540\n"
        )
        self.assertEqual(
            capture.render_extents(text), {"surface": "1920x1080", "scene": "960x540"}
        )
        with self.assertRaises(ValueError):
            capture.render_extents("Application exited successfully")

    def test_render_errors_cannot_pass_as_a_successful_capture(self):
        with self.assertRaises(RuntimeError):
            capture.validate_capture_log(
                "Application exited successfully\n ERROR invalid resource"
            )


if __name__ == "__main__":
    unittest.main()
