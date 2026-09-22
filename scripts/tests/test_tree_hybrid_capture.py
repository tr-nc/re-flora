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


class HybridCaptureTests(unittest.TestCase):
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
                    '[TREE][RASTER_STATIC] mode=B\nApplication exited successfully\n'
                    '[TREE][THIN_WOOD] preserve=true cull=true radius_min=0.251385 '
                    'subminimum_cones=900 subhalf_voxel_cones=200\n'
                    '[TREE][NORMAL_CONFIDENCE] fallback=100 transition=200 reliable=300 '
                    f'single_voxel_cross_sections=123 rest_fingerprint={fingerprint}\n')

            args = ['capture', '--hybrid-lighting', '--time-of-day', '0.3',
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
                    with self.assertRaisesRegex(ValueError, 'changed the thin geometry'):
                        capture.main()
                else:
                    capture.main()
                    if thin:
                        self.assertTrue((root / 'out/thin-geometry.json').is_file())
            self.assertEqual(gui.read_bytes(), source)
            self.assertEqual(camera.read_bytes(), b'original camera\n')
        return observed

    def test_both_modes_keep_raster_geometry_and_only_b_enables_hybrid(self):
        observed = self.run_capture()
        self.assertEqual(len(observed), 4)
        for index, source in enumerate(observed):
            self.assertEqual(capture.setting(source, 'raster_tree_static', 'true'), source)
            self.assertEqual(capture.setting(source, 'raster_tree_hybrid_lighting',
                                            str(index % 2 == 1).lower()), source)
            self.assertEqual(capture.setting(source, 'time_of_day', '0.3'), source)

    def test_thin_geometry_is_enabled_in_both_modes(self):
        for source in self.run_capture(thin=True):
            self.assertEqual(capture.tree_setting(source, 'preserve_thin_branches', 'true'), source)

    def test_changed_ab_geometry_is_rejected_and_configuration_restored(self):
        self.assertEqual(len(self.run_capture(thin=True, changed_mesh=True)), 2)

    def test_guarded_scene_cannot_pass_as_a_thin_geometry_capture(self):
        with self.assertRaisesRegex(ValueError, 'missing actual thin'):
            capture.thin_geometry_evidence('[TREE][THIN_WOOD] preserve=false')
        text = ('[TREE][THIN_WOOD] preserve=true cull=true radius_min=1.05 '
                'subminimum_cones=0 subhalf_voxel_cones=0\n'
                '[TREE][NORMAL_CONFIDENCE] fallback=2 transition=200 reliable=300 '
                'single_voxel_cross_sections=2 rest_fingerprint=1234')
        with self.assertRaisesRegex(ValueError, 'does not exercise actual thin'):
            capture.thin_geometry_evidence(text)
        with patch.object(sys, 'argv', ['capture', '--thin-branches']), \
                contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as error:
            capture.main()
        self.assertEqual(error.exception.code, 2)

    def test_failed_capture_restores_configuration(self):
        self.assertEqual(len(self.run_capture(fail=True)), 1)

    def test_invalid_time_is_rejected_before_running_app(self):
        with patch.object(sys, 'argv', ['capture', '--time-of-day', '1.1']), \
                contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as error:
            capture.main()
        self.assertEqual(error.exception.code, 2)


if __name__ == '__main__':
    unittest.main()
