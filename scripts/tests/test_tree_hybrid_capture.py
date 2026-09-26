"""The historical command name now compares only the new mesh's display modes."""
import contextlib
import io
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import check_raster_tree_static as capture


class TreeDisplayCaptureTests(unittest.TestCase):
    def run_capture(self, fail=False, thin=False, changed_mesh=False):
        source = (capture.ROOT / 'config/gui.toml').read_bytes()
        observed = []
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'config').mkdir()
            gui = root / 'config/gui.toml'
            camera = root / 'config/camera_snapshots.toml'
            gui.write_bytes(source)
            camera.write_bytes(b'original camera\n')

            def run(command, **kwargs):
                observed.append(gui.read_text())
                if fail:
                    raise subprocess.CalledProcessError(1, command)
                image = Path(command[command.index('--screenshot') + 2])
                image.write_bytes(b'fixture, not a real screenshot')
                fingerprint = 'abcd' if changed_mesh and len(observed) == 2 else '1234'
                kwargs['stdout'].write(
                    'Application exited successfully\n'
                    '[TREE][THIN_WOOD] authored=true radius_min=0.251385 subhalf_voxel_cones=200\n'
                    '[TREE][MESH] trees=1 vertices=100 triangles=196 compile_ms=1.0 '
                    f'rest_fingerprint={fingerprint} terrain_voxel_writes=0\n')

            args = ['capture', '--time-of-day', '0.3', '--pixel-size', '7',
                    '--output', str(root / 'out')]
            if thin:
                args.append('--thin-branches')
            with patch.object(capture, 'ROOT', root), patch.object(sys, 'argv', args), \
                    patch.object(capture.subprocess, 'run', side_effect=run), \
                    contextlib.redirect_stdout(io.StringIO()):
                if fail:
                    with self.assertRaises(subprocess.CalledProcessError):
                        capture.main()
                elif changed_mesh:
                    with self.assertRaisesRegex(ValueError, 'display A/B changed'):
                        capture.main()
                else:
                    capture.main()
                    self.assertTrue((root / 'out/geometry.json').is_file())
            self.assertEqual(gui.read_bytes(), source)
            self.assertEqual(camera.read_bytes(), b'original camera\n')
        return observed

    def test_only_display_changes_between_a_and_b(self):
        observed = self.run_capture()
        self.assertEqual(len(observed), 4)
        for index, source in enumerate(observed):
            self.assertEqual(capture.setting(source, 'tree_pixelized', str(index % 2 == 1).lower()), source)
            self.assertEqual(capture.setting(source, 'tree_pixel_size', '7'), source)
            self.assertEqual(capture.setting(source, 'tree_wind', 'false'), source)
            self.assertEqual(capture.setting(source, 'time_of_day', '0.3'), source)
            self.assertNotIn('raster_tree_static', source)
            self.assertNotIn('raster_tree_hybrid_lighting', source)

    def test_thin_validation_uses_continuous_mesh_and_authored_radii(self):
        self.assertEqual(len(self.run_capture(thin=True)), 4)
        with self.assertRaisesRegex(ValueError, 'missing continuous mesh'):
            capture.thin_geometry_evidence('[TREE][NORMAL_CONFIDENCE] fallback=100')

    def test_changed_geometry_is_rejected_without_opt_in_and_config_restored(self):
        self.assertEqual(len(self.run_capture(changed_mesh=True)), 2)

    def test_failed_capture_restores_configuration(self):
        self.assertEqual(len(self.run_capture(fail=True)), 1)

    def test_invalid_settings_and_removed_lighting_mode_are_rejected(self):
        for args in [['--time-of-day', '1.1'], ['--pixel-size', '0'], ['--delay', 'nan'], ['--hybrid-lighting']]:
            with patch.object(sys, 'argv', ['capture', *args]), \
                    contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as error:
                capture.main()
            self.assertEqual(error.exception.code, 2)


if __name__ == '__main__':
    unittest.main()
